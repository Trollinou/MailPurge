use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::classifier::EmailClassification;
use crate::unsubscribe::{parse_unsubscribe_headers, UnsubscribeMethod};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Invalid classification string: {0}")]
    InvalidClassification(String),
    #[error("Database lock poisoned")]
    LockPoisoned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRecord {
    pub id: i64,
    pub email: String,
    pub provider_type: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailRecord {
    pub account_id: i64,
    pub folder_path: String,
    pub uid: u32,
    pub message_id: Option<String>,
    pub root_domain: String,
    pub full_domain: String,
    pub sender_address: String,
    pub subject: String,
    pub received_at: i64,
    pub size_bytes: u64,
    pub has_unsubscribe: bool,
    pub has_attachment: bool,
    pub attachment_names: Vec<String>,
    pub is_flagged: bool,
    pub classification: EmailClassification,
    pub body_snippet: Option<String>,
    pub list_id: Option<String>,
    pub list_unsubscribe: Option<String>,
    pub list_unsubscribe_post: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainStats {
    pub root_domain: String,
    pub total_emails: u64,
    pub total_size: u64,
    pub purgeable_count: u64,
    pub purgeable_size: u64,
    pub protected_count: u64,
    pub is_personal_domain: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributionListGroup {
    pub list_id: String,
    pub list_name: String,
    pub sender_address: String,
    pub email_count: u64,
    pub total_size: u64,
    pub uids: Vec<u32>,
    pub method: UnsubscribeMethod,
    pub target: Option<String>,
    pub post_body: Option<String>,
}

#[derive(Clone)]
pub struct MailStorageRepository {
    conn: Arc<Mutex<Connection>>,
}

impl MailStorageRepository {
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn init_schema(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email TEXT NOT NULL UNIQUE,
                provider_type TEXT NOT NULL,
                display_name TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS emails (
                account_id INTEGER NOT NULL,
                folder_path TEXT NOT NULL,
                uid INTEGER NOT NULL,
                message_id TEXT,
                root_domain TEXT NOT NULL,
                full_domain TEXT NOT NULL,
                sender_address TEXT NOT NULL,
                subject TEXT NOT NULL,
                received_at INTEGER NOT NULL,
                size_bytes INTEGER NOT NULL,
                has_unsubscribe BOOLEAN NOT NULL,
                has_attachment BOOLEAN NOT NULL,
                attachment_names TEXT,
                is_flagged BOOLEAN NOT NULL,
                classification TEXT NOT NULL,
                body_snippet TEXT,
                list_id TEXT,
                list_unsubscribe TEXT,
                list_unsubscribe_post TEXT,
                PRIMARY KEY (account_id, folder_path, uid)
            );

            CREATE INDEX IF NOT EXISTS idx_domain_stats 
            ON emails(account_id, root_domain, classification);
            "#,
        )?;

        // Migration for databases created before new columns were added
        let _ = conn.execute("ALTER TABLE emails ADD COLUMN body_snippet TEXT;", []);
        let _ = conn.execute("ALTER TABLE emails ADD COLUMN list_id TEXT;", []);
        let _ = conn.execute("ALTER TABLE emails ADD COLUMN list_unsubscribe TEXT;", []);
        let _ = conn.execute(
            "ALTER TABLE emails ADD COLUMN list_unsubscribe_post TEXT;",
            [],
        );

        // Invalidate any legacy previews that contained raw HTML / CSS
        let _ = conn.execute(
            "UPDATE emails SET body_snippet = NULL WHERE body_snippet LIKE '<!DOCTYPE%' OR body_snippet LIKE '<html%' OR body_snippet LIKE '%@media%' OR body_snippet LIKE '%!important%' OR body_snippet LIKE '%{font-family%';",
            [],
        );

        drop(conn);
        let _ = self.repair_unknown_domains();

        Ok(())
    }

    /// Automatically repairs records that were saved with `root_domain = 'unknown'`.
    pub fn repair_unknown_domains(&self) -> Result<usize, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;

        let mut stmt = conn.prepare(
            "SELECT account_id, folder_path, uid, sender_address, list_id FROM emails WHERE root_domain = 'unknown'",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        let mut to_update = Vec::new();
        for r in rows {
            let (account_id, folder, uid, sender, list_id) = r?;
            if let Ok(extracted) = crate::domain_extractor::extract_sender_domain(&sender) {
                if extracted.root_domain != "unknown" {
                    to_update.push((
                        account_id,
                        folder,
                        uid,
                        extracted.root_domain,
                        extracted.full_domain,
                        extracted.email_address,
                    ));
                    continue;
                }
            }
            if let Some(lid) = list_id {
                if let Some(root) = crate::domain_extractor::extract_domain_from_list_id(&lid) {
                    to_update.push((account_id, folder, uid, root.clone(), root, sender));
                }
            }
        }

        let count = to_update.len();
        if count > 0 {
            let mut update_stmt = conn.prepare(
                "UPDATE emails SET root_domain = ?1, full_domain = ?2, sender_address = ?3 WHERE account_id = ?4 AND folder_path = ?5 AND uid = ?6",
            )?;
            for (acc, folder, uid, root, full, sender) in to_update {
                let _ = update_stmt.execute(params![root, full, sender, acc, folder, uid]);
            }
        }

        Ok(count)
    }

    pub fn create_account(
        &self,
        email: &str,
        provider_type: &str,
        display_name: &str,
    ) -> Result<AccountRecord, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute(
            r#"
            INSERT INTO accounts (email, provider_type, display_name)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(email) DO UPDATE SET
                provider_type = excluded.provider_type,
                display_name = excluded.display_name
            "#,
            params![email, provider_type, display_name],
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, email, provider_type, display_name FROM accounts WHERE email = ?1",
        )?;
        let account = stmt.query_row(params![email], |row| {
            Ok(AccountRecord {
                id: row.get(0)?,
                email: row.get(1)?,
                provider_type: row.get(2)?,
                display_name: row.get(3)?,
            })
        })?;
        Ok(account)
    }

    pub fn get_account(&self, id: i64) -> Result<Option<AccountRecord>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn
            .prepare("SELECT id, email, provider_type, display_name FROM accounts WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(AccountRecord {
                id: row.get(0)?,
                email: row.get(1)?,
                provider_type: row.get(2)?,
                display_name: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn save_account(&self, account: &AccountRecord) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute(
            r#"
            INSERT INTO accounts (id, email, provider_type, display_name)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                email = excluded.email,
                provider_type = excluded.provider_type,
                display_name = excluded.display_name
            "#,
            params![
                account.id,
                account.email,
                account.provider_type,
                account.display_name
            ],
        )?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountRecord>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT id, email, provider_type, display_name FROM accounts ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AccountRecord {
                id: row.get(0)?,
                email: row.get(1)?,
                provider_type: row.get(2)?,
                display_name: row.get(3)?,
            })
        })?;

        let mut accounts = Vec::new();
        for acc in rows {
            accounts.push(acc?);
        }
        Ok(accounts)
    }

    /// Deletes an account and all its associated cached emails from storage.
    ///
    /// # Errors
    ///
    /// Returns `StorageError` on SQLite execution failures.
    pub fn delete_account(&self, id: i64) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute("DELETE FROM emails WHERE account_id = ?1", params![id])?;
        conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn upsert_emails(&self, emails: &[EmailRecord]) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let tx = conn.transaction()?;

        {
            let mut stmt = tx.prepare(
                r#"
                INSERT INTO emails (
                    account_id, folder_path, uid, message_id, root_domain, full_domain,
                    sender_address, subject, received_at, size_bytes, has_unsubscribe,
                    has_attachment, attachment_names, is_flagged, classification, body_snippet,
                    list_id, list_unsubscribe, list_unsubscribe_post
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
                ON CONFLICT(account_id, folder_path, uid) DO UPDATE SET
                    message_id = excluded.message_id,
                    root_domain = excluded.root_domain,
                    full_domain = excluded.full_domain,
                    sender_address = excluded.sender_address,
                    subject = excluded.subject,
                    received_at = excluded.received_at,
                    size_bytes = excluded.size_bytes,
                    has_unsubscribe = excluded.has_unsubscribe,
                    has_attachment = excluded.has_attachment,
                    attachment_names = excluded.attachment_names,
                    is_flagged = excluded.is_flagged,
                    classification = excluded.classification,
                    body_snippet = excluded.body_snippet,
                    list_id = excluded.list_id,
                    list_unsubscribe = excluded.list_unsubscribe,
                    list_unsubscribe_post = excluded.list_unsubscribe_post
                "#,
            )?;

            for email in emails {
                let attachments_json = serde_json::to_string(&email.attachment_names)?;
                stmt.execute(params![
                    email.account_id,
                    email.folder_path,
                    email.uid,
                    email.message_id,
                    email.root_domain,
                    email.full_domain,
                    email.sender_address,
                    email.subject,
                    email.received_at,
                    email.size_bytes,
                    email.has_unsubscribe,
                    email.has_attachment,
                    attachments_json,
                    email.is_flagged,
                    email.classification.as_str(),
                    email.body_snippet,
                    email.list_id,
                    email.list_unsubscribe,
                    email.list_unsubscribe_post,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_domain_stats(&self, account_id: i64) -> Result<Vec<DomainStats>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT 
                root_domain,
                COUNT(*) as total_emails,
                COALESCE(SUM(size_bytes), 0) as total_size,
                COALESCE(SUM(CASE WHEN classification = 'PURGEABLE' THEN 1 ELSE 0 END), 0) as purgeable_count,
                COALESCE(SUM(CASE WHEN classification = 'PURGEABLE' THEN size_bytes ELSE 0 END), 0) as purgeable_size,
                COALESCE(SUM(CASE WHEN classification != 'PURGEABLE' THEN 1 ELSE 0 END), 0) as protected_count
            FROM emails
            WHERE account_id = ?1
            GROUP BY root_domain
            ORDER BY purgeable_size DESC
            "#,
        )?;

        let rows = stmt.query_map(params![account_id], |row| {
            let root_domain: String = row.get(0)?;
            let total_emails: i64 = row.get(1)?;
            let total_size: i64 = row.get(2)?;
            let purgeable_count: i64 = row.get(3)?;
            let purgeable_size: i64 = row.get(4)?;
            let protected_count: i64 = row.get(5)?;
            let is_personal_domain =
                crate::personal_domains::is_personal_email_provider(&root_domain);

            Ok(DomainStats {
                root_domain,
                total_emails: total_emails as u64,
                total_size: total_size as u64,
                purgeable_count: purgeable_count as u64,
                purgeable_size: purgeable_size as u64,
                protected_count: protected_count as u64,
                is_personal_domain,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn get_emails_by_domain(
        &self,
        account_id: i64,
        root_domain: &str,
    ) -> Result<Vec<EmailRecord>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT 
                account_id, folder_path, uid, message_id, root_domain, full_domain,
                sender_address, subject, received_at, size_bytes, has_unsubscribe,
                has_attachment, attachment_names, is_flagged, classification, body_snippet,
                list_id, list_unsubscribe, list_unsubscribe_post
            FROM emails
            WHERE account_id = ?1 AND root_domain = ?2
            ORDER BY received_at DESC
            "#,
        )?;

        let rows = stmt.query_map(params![account_id, root_domain], |row| {
            let class_str: String = row.get(14)?;
            let classification = match class_str.as_str() {
                "PURGEABLE" => EmailClassification::Purgeable,
                "PROTECTED_INVOICE" => EmailClassification::ProtectedInvoice,
                "PROTECTED_FLAGGED" => EmailClassification::ProtectedFlagged,
                "PROTECTED_USER" => EmailClassification::ProtectedUser,
                "PROTECTED_PERSONAL" => EmailClassification::ProtectedPersonal,
                "RECENT" => EmailClassification::Recent,
                _ => EmailClassification::Purgeable,
            };

            let att_json: Option<String> = row.get(12)?;
            let attachment_names: Vec<String> = att_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

            let uid: i64 = row.get(2)?;
            let size_bytes: i64 = row.get(9)?;

            Ok(EmailRecord {
                account_id: row.get(0)?,
                folder_path: row.get(1)?,
                uid: uid as u32,
                message_id: row.get(3)?,
                root_domain: row.get(4)?,
                full_domain: row.get(5)?,
                sender_address: row.get(6)?,
                subject: row.get(7)?,
                received_at: row.get(8)?,
                size_bytes: size_bytes as u64,
                has_unsubscribe: row.get(10)?,
                has_attachment: row.get(11)?,
                attachment_names,
                is_flagged: row.get(13)?,
                classification,
                body_snippet: row.get(15)?,
                list_id: row.get(16)?,
                list_unsubscribe: row.get(17)?,
                list_unsubscribe_post: row.get(18)?,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn get_domain_distribution_lists(
        &self,
        account_id: i64,
        root_domain: &str,
    ) -> Result<Vec<DistributionListGroup>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT 
                uid, sender_address, size_bytes, has_unsubscribe,
                list_id, list_unsubscribe, list_unsubscribe_post
            FROM emails
            WHERE account_id = ?1 AND root_domain = ?2 AND (has_unsubscribe = 1 OR list_id IS NOT NULL OR list_unsubscribe IS NOT NULL)
            ORDER BY received_at DESC
            "#,
        )?;

        struct ListAccumulator {
            list_name: String,
            sender_address: String,
            email_count: u64,
            total_size: u64,
            uids: Vec<u32>,
            unsub_header: Option<String>,
            post_header: Option<String>,
        }

        let mut map: std::collections::HashMap<String, ListAccumulator> =
            std::collections::HashMap::new();

        let rows = stmt.query_map(params![account_id, root_domain], |row| {
            let uid: i64 = row.get(0)?;
            let sender_address: String = row.get(1)?;
            let size_bytes: i64 = row.get(2)?;
            let _has_unsubscribe: bool = row.get(3)?;
            let list_id: Option<String> = row.get(4)?;
            let list_unsubscribe: Option<String> = row.get(5)?;
            let list_unsubscribe_post: Option<String> = row.get(6)?;

            Ok((
                uid as u32,
                sender_address,
                size_bytes as u64,
                list_id,
                list_unsubscribe,
                list_unsubscribe_post,
            ))
        })?;

        for item in rows {
            let (uid, sender, size, list_id_opt, unsub_opt, post_opt) = item?;
            let group_key = list_id_opt
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| sender.clone());

            let entry = map.entry(group_key.clone()).or_insert_with(|| {
                let display_name = if let Some(lid) = &list_id_opt {
                    lid.trim()
                        .trim_matches(|c| c == '<' || c == '>')
                        .to_string()
                } else {
                    sender.clone()
                };
                ListAccumulator {
                    list_name: display_name,
                    sender_address: sender.clone(),
                    email_count: 0,
                    total_size: 0,
                    uids: Vec::new(),
                    unsub_header: unsub_opt.clone(),
                    post_header: post_opt.clone(),
                }
            });

            entry.email_count += 1;
            entry.total_size += size;
            entry.uids.push(uid);
            if entry.unsub_header.is_none() && unsub_opt.is_some() {
                entry.unsub_header = unsub_opt;
                entry.post_header = post_opt;
            }
        }

        let mut results = Vec::new();
        for (key, acc) in map {
            let unsub_info =
                parse_unsubscribe_headers(acc.unsub_header.as_deref(), acc.post_header.as_deref());

            // Ne proposer que les groupes disposant d'un mécanisme de désabonnement valide (RFC 8058 ou RFC 2369)
            if unsub_info.method == UnsubscribeMethod::None {
                continue;
            }

            results.push(DistributionListGroup {
                list_id: key,
                list_name: acc.list_name,
                sender_address: acc.sender_address,
                email_count: acc.email_count,
                total_size: acc.total_size,
                uids: acc.uids,
                method: unsub_info.method,
                target: unsub_info.target,
                post_body: unsub_info.post_body,
            });
        }

        results.sort_by_key(|a| std::cmp::Reverse(a.total_size));
        Ok(results)
    }

    pub fn update_email_classification(
        &self,
        account_id: i64,
        folder_path: &str,
        uid: u32,
        new_classification: EmailClassification,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute(
            "UPDATE emails SET classification = ?1 WHERE account_id = ?2 AND folder_path = ?3 AND uid = ?4",
            params![new_classification.as_str(), account_id, folder_path, uid],
        )?;
        Ok(())
    }

    pub fn delete_emails(
        &self,
        account_id: i64,
        folder_path: &str,
        uids: &[u32],
    ) -> Result<(), StorageError> {
        if uids.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let tx = conn.transaction()?;

        {
            let mut stmt = tx.prepare(
                "DELETE FROM emails WHERE account_id = ?1 AND folder_path = ?2 AND uid = ?3",
            )?;

            for &uid in uids {
                stmt.execute(params![account_id, folder_path, uid])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_email_snippet(
        &self,
        account_id: i64,
        folder_path: &str,
        uid: u32,
    ) -> Result<Option<String>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT body_snippet FROM emails WHERE account_id = ?1 AND folder_path = ?2 AND uid = ?3 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![account_id, folder_path, uid as i64])?;
        if let Some(row) = rows.next()? {
            let snippet: Option<String> = row.get(0)?;
            Ok(snippet)
        } else {
            Ok(None)
        }
    }

    pub fn update_body_snippet(
        &self,
        account_id: i64,
        folder_path: &str,
        uid: u32,
        snippet: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute(
            "UPDATE emails SET body_snippet = ?1 WHERE account_id = ?2 AND folder_path = ?3 AND uid = ?4",
            params![snippet, account_id, folder_path, uid as i64],
        )?;
        Ok(())
    }
}
