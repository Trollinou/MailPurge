use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::classifier::EmailClassification;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Invalid classification string: {0}")]
    InvalidClassification(String),
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainStats {
    pub root_domain: String,
    pub total_emails: u64,
    pub total_size: u64,
    pub purgeable_count: u64,
    pub purgeable_size: u64,
    pub protected_count: u64,
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
        let conn = self.conn.lock().unwrap();

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
                PRIMARY KEY (account_id, folder_path, uid)
            );

            CREATE INDEX IF NOT EXISTS idx_domain_stats 
            ON emails(account_id, root_domain, classification);
            "#,
        )?;

        Ok(())
    }

    pub fn save_account(&self, account: &AccountRecord) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, email, provider_type, display_name FROM accounts ORDER BY id ASC")?;
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

    pub fn upsert_emails(&self, emails: &[EmailRecord]) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        {
            let mut stmt = tx.prepare(
                r#"
                INSERT INTO emails (
                    account_id, folder_path, uid, message_id, root_domain, full_domain,
                    sender_address, subject, received_at, size_bytes, has_unsubscribe,
                    has_attachment, attachment_names, is_flagged, classification
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
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
                    classification = excluded.classification
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
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_domain_stats(&self, account_id: i64) -> Result<Vec<DomainStats>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT 
                root_domain,
                COUNT(*) as total_emails,
                COALESCE(SUM(size_bytes), 0) as total_size,
                COALESCE(SUM(CASE WHEN classification = 'PURGEABLE' THEN 1 ELSE 0 END), 0) as purgeable_count,
                COALESCE(SUM(CASE WHEN classification = 'PURGEABLE' THEN size_bytes ELSE 0 END), 0) as purgeable_size,
                COALESCE(SUM(CASE WHEN classification = 'PROTECTED_INVOICE' OR classification = 'PROTECTED_FLAGGED' THEN 1 ELSE 0 END), 0) as protected_count
            FROM emails
            WHERE account_id = ?1
            GROUP BY root_domain
            ORDER BY purgeable_size DESC
            "#,
        )?;

        let rows = stmt.query_map(params![account_id], |row| {
            let total_emails: i64 = row.get(1)?;
            let total_size: i64 = row.get(2)?;
            let purgeable_count: i64 = row.get(3)?;
            let purgeable_size: i64 = row.get(4)?;
            let protected_count: i64 = row.get(5)?;

            Ok(DomainStats {
                root_domain: row.get(0)?,
                total_emails: total_emails as u64,
                total_size: total_size as u64,
                purgeable_count: purgeable_count as u64,
                purgeable_size: purgeable_size as u64,
                protected_count: protected_count as u64,
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
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT 
                account_id, folder_path, uid, message_id, root_domain, full_domain,
                sender_address, subject, received_at, size_bytes, has_unsubscribe,
                has_attachment, attachment_names, is_flagged, classification
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
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
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

        let mut conn = self.conn.lock().unwrap();
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
}
