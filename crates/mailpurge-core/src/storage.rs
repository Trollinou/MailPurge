use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::classifier::{ClassifierConfig, EmailClassification};
use crate::domain::community_rules::{
    default_disposable_domains, default_esp_whitelist, default_suspicious_tlds,
    CommunityRulesUpdateReport, EspWhitelistEntry, SuspiciousTldEntry,
};
use crate::domain::orphan_spam::{
    DnsStatus, OrphanCategory, OrphanCategoryGroup, OrphanSummaryReport, SharedDomainIntelligence,
};
use crate::domain::trusted_domains::is_trusted_domain;
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
    #[serde(default)]
    pub account_name: Option<String>,
    #[serde(default)]
    pub server_host: Option<String>,
    #[serde(default)]
    pub server_port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    #[serde(default)]
    pub return_path: Option<String>,
    #[serde(default)]
    pub auth_results: Option<String>,
    #[serde(default)]
    pub spam_headers: Option<String>,
    #[serde(default)]
    pub to_recipients: Option<String>,
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
    #[serde(default)]
    pub last_received_at: i64,
    #[serde(default)]
    pub list_count: u32,
    #[serde(default)]
    pub is_suspicious_spam: bool,
    #[serde(default)]
    pub orphan_category: Option<String>,
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
    #[serde(default)]
    pub last_received_at: i64,
}

/// Evaluates whether a domain exhibits characteristics of untrusted or spam mail.
#[must_use]
pub fn is_suspicious_domain(root_domain: &str, total_emails: u64, has_unsubscribe: bool) -> bool {
    const SUSPICIOUS_TLDS: &[&str] = &[
        ".xyz", ".top", ".buzz", ".icu", ".work", ".click", ".fit", ".rest", ".surf", ".monster",
        ".sbs", ".cfd", ".bar", ".wiki", ".beauty", ".skin", ".hair", ".quest", ".link", ".live",
        ".guru", ".shop", ".online", ".site", ".support",
    ];
    let lower = root_domain.to_lowercase();
    for tld in SUSPICIOUS_TLDS {
        if lower.ends_with(tld) {
            return true;
        }
    }
    // High digit count in domain name with very low email volume and no unsubscribe header
    if total_emails <= 2 && !has_unsubscribe {
        let digit_count = lower.chars().filter(|c| c.is_ascii_digit()).count();
        if digit_count >= 3 {
            return true;
        }
    }
    false
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

            CREATE TABLE IF NOT EXISTS shared_domain_intelligence (
                root_domain TEXT PRIMARY KEY,
                dns_status TEXT,
                dns_checked_at INTEGER,
                dns_ttl_seconds INTEGER DEFAULT 604800,
                is_disposable BOOLEAN DEFAULT 0,
                is_known_esp BOOLEAN DEFAULT 0,
                is_suspicious_tld BOOLEAN DEFAULT 0,
                dnsbl_status TEXT,
                dnsbl_checked_at INTEGER,
                dnsbl_ttl_seconds INTEGER DEFAULT 86400,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS esp_whitelist (
                root_domain TEXT PRIMARY KEY,
                provider_name TEXT NOT NULL,
                is_builtin BOOLEAN DEFAULT 1,
                is_user_added BOOLEAN DEFAULT 0,
                is_enabled BOOLEAN DEFAULT 1,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS suspicious_tlds (
                tld TEXT PRIMARY KEY,
                risk_level TEXT NOT NULL,
                is_builtin BOOLEAN DEFAULT 1,
                is_enabled BOOLEAN DEFAULT 1,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS disposable_domains (
                root_domain TEXT PRIMARY KEY,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS user_trusted_domains (
                account_id INTEGER NOT NULL,
                root_domain TEXT NOT NULL,
                source TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (account_id, root_domain)
            );

            CREATE TABLE IF NOT EXISTS tagged_orphan_domains (
                account_id INTEGER NOT NULL,
                root_domain TEXT NOT NULL,
                orphan_category TEXT NOT NULL,
                tagged_at INTEGER NOT NULL,
                PRIMARY KEY (account_id, root_domain)
            );
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
        let _ = conn.execute("ALTER TABLE emails ADD COLUMN return_path TEXT;", []);
        let _ = conn.execute("ALTER TABLE emails ADD COLUMN auth_results TEXT;", []);
        let _ = conn.execute("ALTER TABLE emails ADD COLUMN spam_headers TEXT;", []);
        let _ = conn.execute("ALTER TABLE emails ADD COLUMN to_recipients TEXT;", []);
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN account_name TEXT;", []);
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN server_host TEXT;", []);
        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN server_port INTEGER;", []);

        // Migrate existing accounts lacking server_host
        if let Ok(mut mig_stmt) =
            conn.prepare("SELECT id, display_name FROM accounts WHERE server_host IS NULL")
        {
            let mig_rows = mig_stmt.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            });
            if let Ok(rows) = mig_rows {
                let mut updates = Vec::new();
                for r in rows.flatten() {
                    let (acc_id, disp) = r;
                    if let Some(start) = disp.find('(') {
                        if let Some(end) = disp.rfind(')') {
                            let inside = &disp[start + 1..end];
                            if let Some((h, p)) = inside.split_once(':') {
                                if let Ok(port) = p.trim().parse::<u16>() {
                                    updates.push((acc_id, h.trim().to_string(), port));
                                }
                            }
                        }
                    }
                }
                for (acc_id, host, port) in updates {
                    let _ = conn.execute(
                        "UPDATE accounts SET server_host = ?1, server_port = ?2 WHERE id = ?3",
                        params![host, port, acc_id],
                    );
                }
            }
        }

        // Invalidate any legacy previews that contained raw HTML / CSS
        let _ = conn.execute(
            "UPDATE emails SET body_snippet = NULL WHERE body_snippet LIKE '<!DOCTYPE%' OR body_snippet LIKE '<html%' OR body_snippet LIKE '%@media%' OR body_snippet LIKE '%!important%' OR body_snippet LIKE '%{font-family%';",
            [],
        );

        // Seed default ESP whitelist if empty
        let esp_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM esp_whitelist", [], |r| r.get(0))
            .unwrap_or(0);
        if esp_count == 0 {
            for entry in default_esp_whitelist() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO esp_whitelist (root_domain, provider_name, is_builtin, is_user_added, is_enabled, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![entry.root_domain, entry.provider_name, entry.is_builtin, entry.is_user_added, entry.is_enabled, entry.updated_at],
                );
            }
        }

        // Seed default suspicious TLDs if empty
        let tld_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM suspicious_tlds", [], |r| r.get(0))
            .unwrap_or(0);
        if tld_count == 0 {
            for entry in default_suspicious_tlds() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO suspicious_tlds (tld, risk_level, is_builtin, is_enabled, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![entry.tld, entry.risk_level, entry.is_builtin, entry.is_enabled, entry.updated_at],
                );
            }
        }

        // Seed default disposable domains if empty
        let disp_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM disposable_domains", [], |r| r.get(0))
            .unwrap_or(0);
        if disp_count == 0 {
            let now = chrono::Utc::now().timestamp();
            for domain in default_disposable_domains() {
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO disposable_domains (root_domain, updated_at) VALUES (?1, ?2)",
                    params![domain, now],
                );
            }
        }

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
        self.create_account_with_details(email, provider_type, display_name, None, None, None)
    }

    pub fn create_account_with_details(
        &self,
        email: &str,
        provider_type: &str,
        display_name: &str,
        account_name: Option<&str>,
        server_host: Option<&str>,
        server_port: Option<u16>,
    ) -> Result<AccountRecord, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute(
            r#"
            INSERT INTO accounts (email, provider_type, display_name, account_name, server_host, server_port)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(email) DO UPDATE SET
                provider_type = excluded.provider_type,
                display_name = excluded.display_name,
                account_name = excluded.account_name,
                server_host = excluded.server_host,
                server_port = excluded.server_port
            "#,
            params![email, provider_type, display_name, account_name, server_host, server_port],
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, email, provider_type, display_name, account_name, server_host, server_port FROM accounts WHERE email = ?1",
        )?;
        let account = stmt.query_row(params![email], |row| {
            Ok(AccountRecord {
                id: row.get(0)?,
                email: row.get(1)?,
                provider_type: row.get(2)?,
                display_name: row.get(3)?,
                account_name: row.get(4)?,
                server_host: row.get(5)?,
                server_port: row.get(6)?,
            })
        })?;
        Ok(account)
    }

    pub fn get_account(&self, id: i64) -> Result<Option<AccountRecord>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn
            .prepare("SELECT id, email, provider_type, display_name, account_name, server_host, server_port FROM accounts WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(AccountRecord {
                id: row.get(0)?,
                email: row.get(1)?,
                provider_type: row.get(2)?,
                display_name: row.get(3)?,
                account_name: row.get(4)?,
                server_host: row.get(5)?,
                server_port: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn save_account(&self, account: &AccountRecord) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute(
            r#"
            INSERT INTO accounts (id, email, provider_type, display_name, account_name, server_host, server_port)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
                email = excluded.email,
                provider_type = excluded.provider_type,
                display_name = excluded.display_name,
                account_name = excluded.account_name,
                server_host = excluded.server_host,
                server_port = excluded.server_port
            "#,
            params![
                account.id,
                account.email,
                account.provider_type,
                account.display_name,
                account.account_name,
                account.server_host,
                account.server_port
            ],
        )?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountRecord>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT id, email, provider_type, display_name, account_name, server_host, server_port FROM accounts ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AccountRecord {
                id: row.get(0)?,
                email: row.get(1)?,
                provider_type: row.get(2)?,
                display_name: row.get(3)?,
                account_name: row.get(4)?,
                server_host: row.get(5)?,
                server_port: row.get(6)?,
            })
        })?;

        let mut accounts = Vec::new();
        for acc in rows {
            accounts.push(acc?);
        }
        Ok(accounts)
    }

    pub fn update_account_name(&self, id: i64, name: Option<&str>) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute(
            "UPDATE accounts SET account_name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    pub fn vacuum(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute("VACUUM", [])?;
        Ok(())
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
                    list_id, list_unsubscribe, list_unsubscribe_post,
                    return_path, auth_results, spam_headers, to_recipients
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
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
                    list_unsubscribe_post = excluded.list_unsubscribe_post,
                    return_path = excluded.return_path,
                    auth_results = excluded.auth_results,
                    spam_headers = excluded.spam_headers,
                    to_recipients = excluded.to_recipients
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
                    email.return_path,
                    email.auth_results,
                    email.spam_headers,
                    email.to_recipients,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_domain_stats(&self, account_id: i64) -> Result<Vec<DomainStats>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;

        // Query tagged orphan categories for this account
        let mut tagged_map = std::collections::HashMap::new();
        if let Ok(mut tag_stmt) = conn.prepare(
            "SELECT root_domain, orphan_category FROM tagged_orphan_domains WHERE account_id = ?1",
        ) {
            if let Ok(tag_rows) = tag_stmt.query_map(params![account_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            }) {
                for item in tag_rows.flatten() {
                    tagged_map.insert(item.0, item.1);
                }
            }
        }

        let mut stmt = conn.prepare(
            r#"
            SELECT 
                root_domain,
                COUNT(*) as total_emails,
                COALESCE(SUM(size_bytes), 0) as total_size,
                COALESCE(SUM(CASE WHEN classification = 'PURGEABLE' THEN 1 ELSE 0 END), 0) as purgeable_count,
                COALESCE(SUM(CASE WHEN classification = 'PURGEABLE' THEN size_bytes ELSE 0 END), 0) as purgeable_size,
                COALESCE(SUM(CASE WHEN classification != 'PURGEABLE' THEN 1 ELSE 0 END), 0) as protected_count,
                COALESCE(MAX(received_at), 0) as last_received_at,
                COUNT(DISTINCT CASE WHEN (has_unsubscribe = 1 OR list_id IS NOT NULL OR list_unsubscribe IS NOT NULL) THEN COALESCE(list_id, sender_address) END) as list_count,
                COALESCE(MAX(CASE WHEN has_unsubscribe = 1 THEN 1 ELSE 0 END), 0) as has_any_unsub
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
            let last_received_at: i64 = row.get(6)?;
            let list_count: i64 = row.get(7)?;
            let has_any_unsub: i64 = row.get(8)?;
            let is_personal_domain =
                crate::personal_domains::is_personal_email_provider(&root_domain);
            let is_suspicious_spam =
                is_suspicious_domain(&root_domain, total_emails as u64, has_any_unsub > 0);

            Ok(DomainStats {
                root_domain,
                total_emails: total_emails as u64,
                total_size: total_size as u64,
                purgeable_count: purgeable_count as u64,
                purgeable_size: purgeable_size as u64,
                protected_count: protected_count as u64,
                is_personal_domain,
                last_received_at,
                list_count: list_count as u32,
                is_suspicious_spam,
                orphan_category: None,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            let mut stat = r?;
            stat.orphan_category = tagged_map.remove(&stat.root_domain);
            results.push(stat);
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
                list_id, list_unsubscribe, list_unsubscribe_post,
                return_path, auth_results, spam_headers, to_recipients
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
                return_path: row.get(19).ok(),
                auth_results: row.get(20).ok(),
                spam_headers: row.get(21).ok(),
                to_recipients: row.get(22).ok(),
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
                list_id, list_unsubscribe, list_unsubscribe_post, received_at
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
            last_received_at: i64,
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
            let received_at: i64 = row.get(7)?;

            Ok((
                uid as u32,
                sender_address,
                size_bytes as u64,
                list_id,
                list_unsubscribe,
                list_unsubscribe_post,
                received_at,
            ))
        })?;

        for item in rows {
            let (uid, sender, size, list_id_opt, unsub_opt, post_opt, received_at) = item?;
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
                    last_received_at: received_at,
                }
            });

            entry.email_count += 1;
            entry.total_size += size;
            entry.uids.push(uid);
            entry.last_received_at = entry.last_received_at.max(received_at);
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
                last_received_at: acc.last_received_at,
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

    /// Fetches diagnostic info (attachment names, auth results, spam headers) for an email.
    pub fn get_email_diagnostic_info(
        &self,
        account_id: i64,
        folder_path: &str,
        uid: u32,
    ) -> Result<Option<(Vec<String>, Option<String>, Option<String>)>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT attachment_names, auth_results, spam_headers FROM emails WHERE account_id = ?1 AND folder_path = ?2 AND uid = ?3 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![account_id, folder_path, uid as i64])?;
        if let Some(row) = rows.next()? {
            let att_json: Option<String> = row.get(0)?;
            let atts: Vec<String> = att_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default();
            let auth: Option<String> = row.get(1)?;
            let spam: Option<String> = row.get(2)?;
            Ok(Some((atts, auth, spam)))
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

    pub fn unprotect_domain_emails(
        &self,
        account_id: i64,
        root_domain: &str,
    ) -> Result<usize, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let count = conn.execute(
            "UPDATE emails SET classification = 'PURGEABLE' WHERE account_id = ?1 AND root_domain = ?2 AND classification != 'PURGEABLE'",
            params![account_id, root_domain],
        )?;
        Ok(count)
    }

    pub fn reclassify_domain_emails(
        &self,
        account_id: i64,
        root_domain: &str,
        classifier_config: &ClassifierConfig,
        now: i64,
    ) -> Result<usize, StorageError> {
        let emails = self.get_emails_by_domain(account_id, root_domain)?;
        let mut updated = 0;
        let mut conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE emails SET classification = ?1 WHERE account_id = ?2 AND folder_path = ?3 AND uid = ?4",
            )?;
            for email in emails {
                let new_class = crate::classifier::classify_email(
                    &crate::classifier::EmailMetadataInput {
                        uid: email.uid,
                        subject: &email.subject,
                        received_at_timestamp: email.received_at,
                        is_flagged: email.is_flagged,
                        has_unsubscribe: email.has_unsubscribe,
                        attachment_names: &email.attachment_names,
                        root_domain: Some(&email.root_domain),
                        spam_verdict: None,
                    },
                    classifier_config,
                    now,
                );
                if new_class != email.classification {
                    stmt.execute(params![
                        new_class.as_str(),
                        account_id,
                        email.folder_path,
                        email.uid
                    ])?;
                    updated += 1;
                }
            }
        }
        tx.commit()?;
        Ok(updated)
    }

    /// Retrieves cached domain intelligence for the requested domains.
    pub fn get_shared_domain_intelligence(
        &self,
        domains: &[String],
    ) -> Result<HashMap<String, SharedDomainIntelligence>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        Self::query_shared_intelligence(&conn, domains)
    }

    fn query_shared_intelligence(
        conn: &Connection,
        domains: &[String],
    ) -> Result<HashMap<String, SharedDomainIntelligence>, StorageError> {
        let mut results = HashMap::new();
        if domains.is_empty() {
            return Ok(results);
        }

        let mut stmt = conn.prepare(
            "SELECT root_domain, dns_status, dns_checked_at, dns_ttl_seconds, is_disposable, is_known_esp, is_suspicious_tld, dnsbl_status, dnsbl_checked_at, updated_at FROM shared_domain_intelligence WHERE root_domain = ?1",
        )?;

        for domain in domains {
            let row_res = stmt.query_row(params![domain], |row| {
                let status_str: Option<String> = row.get(1)?;
                let dns_status = status_str.map(|s| DnsStatus::from_db_str(&s));
                Ok(SharedDomainIntelligence {
                    root_domain: row.get(0)?,
                    dns_status,
                    dns_checked_at: row.get(2)?,
                    dns_ttl_seconds: row.get(3)?,
                    is_disposable: row.get(4)?,
                    is_known_esp: row.get(5)?,
                    is_suspicious_tld: row.get(6)?,
                    dnsbl_status: row.get(7)?,
                    dnsbl_checked_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            });

            if let Ok(intel) = row_res {
                results.insert(domain.clone(), intel);
            }
        }

        Ok(results)
    }

    /// Upserts a batch of resolved DNS statuses into the global domain intelligence table.
    pub fn upsert_shared_domain_dns_batch(
        &self,
        statuses: &[(String, DnsStatus, i64)],
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let now = chrono::Utc::now().timestamp();
        let tx = conn.transaction()?;

        {
            let mut stmt = tx.prepare(
                r#"
                INSERT INTO shared_domain_intelligence (
                    root_domain, dns_status, dns_checked_at, dns_ttl_seconds, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(root_domain) DO UPDATE SET
                    dns_status = excluded.dns_status,
                    dns_checked_at = excluded.dns_checked_at,
                    dns_ttl_seconds = excluded.dns_ttl_seconds,
                    updated_at = excluded.updated_at
                "#,
            )?;

            for (domain, status, ttl) in statuses {
                stmt.execute(params![domain, status.as_str(), now, ttl, now])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Returns all configured ESP whitelist entries.
    pub fn get_esp_whitelist(&self) -> Result<Vec<EspWhitelistEntry>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT root_domain, provider_name, is_builtin, is_user_added, is_enabled, updated_at FROM esp_whitelist ORDER BY root_domain ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(EspWhitelistEntry {
                root_domain: row.get(0)?,
                provider_name: row.get(1)?,
                is_builtin: row.get(2)?,
                is_user_added: row.get(3)?,
                is_enabled: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// Enables or disables an ESP whitelist entry.
    pub fn set_esp_enabled(&self, root_domain: &str, is_enabled: bool) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE esp_whitelist SET is_enabled = ?1, updated_at = ?2 WHERE root_domain = ?3",
            params![is_enabled, now, root_domain],
        )?;
        Ok(())
    }

    /// Inserts or updates an ESP whitelist entry.
    pub fn upsert_esp_whitelist_entry(
        &self,
        entry: &EspWhitelistEntry,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute(
            r#"
            INSERT INTO esp_whitelist (root_domain, provider_name, is_builtin, is_user_added, is_enabled, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(root_domain) DO UPDATE SET
                provider_name = excluded.provider_name,
                is_enabled = excluded.is_enabled,
                updated_at = excluded.updated_at
            "#,
            params![
                entry.root_domain,
                entry.provider_name,
                entry.is_builtin,
                entry.is_user_added,
                entry.is_enabled,
                entry.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Returns all suspicious TLD rules.
    pub fn get_suspicious_tlds(&self) -> Result<Vec<SuspiciousTldEntry>, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT tld, risk_level, is_builtin, is_enabled, updated_at FROM suspicious_tlds ORDER BY tld ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(SuspiciousTldEntry {
                tld: row.get(0)?,
                risk_level: row.get(1)?,
                is_builtin: row.get(2)?,
                is_enabled: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// Enables or disables a suspicious TLD rule.
    pub fn set_tld_enabled(&self, tld: &str, is_enabled: bool) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE suspicious_tlds SET is_enabled = ?1, updated_at = ?2 WHERE tld = ?3",
            params![is_enabled, now, tld],
        )?;
        Ok(())
    }

    /// Inserts or updates a suspicious TLD entry.
    pub fn upsert_tld_entry(&self, entry: &SuspiciousTldEntry) -> Result<(), StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        conn.execute(
            r#"
            INSERT INTO suspicious_tlds (tld, risk_level, is_builtin, is_enabled, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(tld) DO UPDATE SET
                risk_level = excluded.risk_level,
                is_enabled = excluded.is_enabled,
                updated_at = excluded.updated_at
            "#,
            params![
                entry.tld,
                entry.risk_level,
                entry.is_builtin,
                entry.is_enabled,
                entry.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Batch upserts disposable email domain names.
    pub fn upsert_disposable_domains(&self, domains: &[String]) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let now = chrono::Utc::now().timestamp();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO disposable_domains (root_domain, updated_at) VALUES (?1, ?2) ON CONFLICT(root_domain) DO UPDATE SET updated_at = excluded.updated_at",
            )?;
            for d in domains {
                stmt.execute(params![d, now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Returns true if domain is present in disposable domains blacklist.
    pub fn is_disposable_domain(&self, root_domain: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM disposable_domains WHERE root_domain = ?1",
                params![root_domain],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok(count > 0)
    }

    /// Synchronizes community rules into SQLite without overwriting user custom entries or toggles.
    pub fn sync_community_rules(
        &self,
        esps: &[EspWhitelistEntry],
        tlds: &[SuspiciousTldEntry],
        disposable: &[String],
    ) -> Result<CommunityRulesUpdateReport, StorageError> {
        let mut conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let now = chrono::Utc::now().timestamp();
        let tx = conn.transaction()?;

        let mut esp_updated = 0;
        {
            let mut stmt = tx.prepare(
                r#"
                INSERT INTO esp_whitelist (root_domain, provider_name, is_builtin, is_user_added, is_enabled, updated_at)
                VALUES (?1, ?2, 1, 0, 1, ?3)
                ON CONFLICT(root_domain) DO UPDATE SET
                    provider_name = excluded.provider_name,
                    updated_at = excluded.updated_at
                WHERE is_user_added = 0
                "#,
            )?;
            for e in esps {
                stmt.execute(params![e.root_domain, e.provider_name, now])?;
                esp_updated += 1;
            }
        }

        let mut tlds_updated = 0;
        {
            let mut stmt = tx.prepare(
                r#"
                INSERT INTO suspicious_tlds (tld, risk_level, is_builtin, is_enabled, updated_at)
                VALUES (?1, ?2, 1, 1, ?3)
                ON CONFLICT(tld) DO UPDATE SET
                    risk_level = excluded.risk_level,
                    updated_at = excluded.updated_at
                "#,
            )?;
            for t in tlds {
                stmt.execute(params![t.tld, t.risk_level, now])?;
                tlds_updated += 1;
            }
        }

        let mut disp_updated = 0;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO disposable_domains (root_domain, updated_at) VALUES (?1, ?2) ON CONFLICT(root_domain) DO UPDATE SET updated_at = excluded.updated_at",
            )?;
            for d in disposable {
                stmt.execute(params![d, now])?;
                disp_updated += 1;
            }
        }

        tx.commit()?;

        let (total_esp, total_tlds, total_disp) = {
            let e: usize = conn
                .query_row("SELECT COUNT(*) FROM esp_whitelist", [], |r| r.get(0))
                .unwrap_or(0);
            let t: usize = conn
                .query_row("SELECT COUNT(*) FROM suspicious_tlds", [], |r| r.get(0))
                .unwrap_or(0);
            let d: usize = conn
                .query_row("SELECT COUNT(*) FROM disposable_domains", [], |r| r.get(0))
                .unwrap_or(0);
            (e, t, d)
        };

        Ok(CommunityRulesUpdateReport {
            esp_updated_count: esp_updated,
            tlds_updated_count: tlds_updated,
            disposable_updated_count: disp_updated,
            total_rules_count: total_esp + total_tlds + total_disp,
            last_updated_at: now,
            status_message: "Règles communautaires synchronisées avec succès".to_string(),
        })
    }

    /// Returns the current count and status of all community rules.
    pub fn get_community_rules_status(&self) -> Result<CommunityRulesUpdateReport, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let e: usize = conn
            .query_row("SELECT COUNT(*) FROM esp_whitelist", [], |r| r.get(0))
            .unwrap_or(0);
        let t: usize = conn
            .query_row("SELECT COUNT(*) FROM suspicious_tlds", [], |r| r.get(0))
            .unwrap_or(0);
        let d: usize = conn
            .query_row("SELECT COUNT(*) FROM disposable_domains", [], |r| r.get(0))
            .unwrap_or(0);
        let max_updated: Option<i64> = conn
            .query_row(
                r#"
                SELECT MAX(m) FROM (
                    SELECT MAX(updated_at) as m FROM esp_whitelist
                    UNION ALL
                    SELECT MAX(updated_at) as m FROM suspicious_tlds
                    UNION ALL
                    SELECT MAX(updated_at) as m FROM disposable_domains
                )
                "#,
                [],
                |r| r.get(0),
            )
            .unwrap_or(None);

        let last_updated = max_updated.unwrap_or_else(|| chrono::Utc::now().timestamp());

        Ok(CommunityRulesUpdateReport {
            esp_updated_count: e,
            tlds_updated_count: t,
            disposable_updated_count: d,
            total_rules_count: e + t + d,
            last_updated_at: last_updated,
            status_message: "Règles prêtes et actives".to_string(),
        })
    }

    /// Returns summary of orphan domains and categorized groups for a single account or all accounts.
    pub fn get_orphan_summary(
        &self,
        account_id: Option<i64>,
        threshold: u64,
    ) -> Result<OrphanSummaryReport, StorageError> {
        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let threshold = threshold.clamp(1, 10);
        let now = chrono::Utc::now().timestamp();

        let sql = r#"
            SELECT 
                root_domain,
                COUNT(*) as email_count,
                COALESCE(SUM(size_bytes), 0) as total_size,
                COALESCE(SUM(CASE WHEN classification = 'PURGEABLE' THEN 1 ELSE 0 END), 0) as purgeable_count,
                COALESCE(SUM(CASE WHEN classification != 'PURGEABLE' THEN 1 ELSE 0 END), 0) as protected_count
            FROM emails
            WHERE (?1 IS NULL OR account_id = ?1)
            GROUP BY root_domain
            HAVING email_count <= ?2
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![account_id, threshold], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, i64>(2)? as u64,
                row.get::<_, i64>(3)? as u64,
                row.get::<_, i64>(4)? as u64,
            ))
        })?;

        let mut candidate_domains = Vec::new();
        for r in rows {
            candidate_domains.push(r?);
        }

        // Fetch enabled ESPs and suspicious TLDs
        let mut esp_set = std::collections::HashSet::new();
        if let Ok(mut esp_stmt) =
            conn.prepare("SELECT root_domain FROM esp_whitelist WHERE is_enabled = 1")
        {
            if let Ok(esp_rows) = esp_stmt.query_map([], |r| r.get::<_, String>(0)) {
                for esp in esp_rows.flatten() {
                    esp_set.insert(esp);
                }
            }
        }

        let mut tld_set = std::collections::HashSet::new();
        if let Ok(mut tld_stmt) =
            conn.prepare("SELECT tld FROM suspicious_tlds WHERE is_enabled = 1")
        {
            if let Ok(tld_rows) = tld_stmt.query_map([], |r| r.get::<_, String>(0)) {
                for tld in tld_rows.flatten() {
                    tld_set.insert(tld);
                }
            }
        }

        let mut disposable_set = std::collections::HashSet::new();
        if let Ok(mut disp_stmt) = conn.prepare("SELECT root_domain FROM disposable_domains") {
            if let Ok(disp_rows) = disp_stmt.query_map([], |r| r.get::<_, String>(0)) {
                for disp in disp_rows.flatten() {
                    disposable_set.insert(disp);
                }
            }
        }

        let mut user_trusted_set = std::collections::HashSet::new();
        if let Some(id) = account_id {
            if let Ok(mut ut_stmt) =
                conn.prepare("SELECT root_domain FROM user_trusted_domains WHERE account_id = ?1")
            {
                if let Ok(ut_rows) = ut_stmt.query_map(params![id], |r| r.get::<_, String>(0)) {
                    for r in ut_rows.flatten() {
                        user_trusted_set.insert(r);
                    }
                }
            }
        } else if let Ok(mut ut_stmt) = conn.prepare("SELECT root_domain FROM user_trusted_domains")
        {
            if let Ok(ut_rows) = ut_stmt.query_map([], |r| r.get::<_, String>(0)) {
                for r in ut_rows.flatten() {
                    user_trusted_set.insert(r);
                }
            }
        }

        let domain_names: Vec<String> = candidate_domains.iter().map(|(d, ..)| d.clone()).collect();
        let intel_map = Self::query_shared_intelligence(&conn, &domain_names)?;

        let mut dead_domains = Vec::new();
        let mut server_spam_domains = Vec::new();
        let mut structural_domains = Vec::new();
        let mut disposable_candidates = Vec::new();

        let mut total_domains = 0usize;
        let mut total_emails = 0u64;
        let mut total_size = 0u64;
        let mut dns_analyzed_count = 0usize;
        let mut dns_pending_count = 0usize;

        let mut email_headers_stmt = conn.prepare(
            "SELECT return_path, auth_results, spam_headers, sender_address, classification FROM emails WHERE (?1 IS NULL OR account_id = ?1) AND root_domain = ?2",
        )?;

        for (domain, email_count, size, _purgeable_count, protected_count) in candidate_domains {
            let intel = intel_map.get(&domain);
            let is_dns_fresh = intel.map_or(false, |i| i.is_dns_fresh(now));
            if is_dns_fresh {
                dns_analyzed_count += 1;
            } else {
                dns_pending_count += 1;
            }

            let email_headers = email_headers_stmt.query_map(params![account_id, domain], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?;

            let mut has_server_spam_flag = false;
            let mut has_return_path_dissonance = false;

            for eh in email_headers.flatten() {
                let (ret_path, auth_res, spam_head, sender, _) = eh;

                if let Some(ar) = auth_res {
                    let lower = ar.to_lowercase();
                    if lower.contains("spf=fail")
                        || lower.contains("spf=softfail")
                        || lower.contains("dkim=fail")
                        || lower.contains("dmarc=fail")
                    {
                        has_server_spam_flag = true;
                    }
                }
                if let Some(sh) = spam_head {
                    let lower = sh.to_lowercase();
                    if lower.contains("yes")
                        || lower.contains("scl:5")
                        || lower.contains("scl:6")
                        || lower.contains("scl:7")
                        || lower.contains("scl:8")
                        || lower.contains("scl:9")
                        || lower.contains("cat:spm")
                    {
                        has_server_spam_flag = true;
                    }
                }

                if let Some(rp) = ret_path {
                    let sender_root = crate::domain_extractor::extract_sender_domain(&sender)
                        .map(|s| s.root_domain)
                        .unwrap_or_default();
                    let rp_root = crate::domain_extractor::extract_sender_domain(&rp)
                        .map(|s| s.root_domain)
                        .unwrap_or_default();
                    if !sender_root.is_empty()
                        && !rp_root.is_empty()
                        && sender_root != rp_root
                        && !esp_set.contains(&rp_root)
                    {
                        has_return_path_dissonance = true;
                    }
                }
            }

            // Check if domain is trusted / institutional / personal provider
            let is_personal = crate::personal_domains::is_personal_email_provider(&domain);
            let is_trusted = is_personal
                || is_trusted_domain(&domain, &domain)
                || user_trusted_set.contains(&domain);

            // Anti-phishing safeguard: only if server anti-spam detected spoofing/fail do we keep a trusted domain
            if is_trusted && !has_server_spam_flag {
                continue;
            }

            let is_dead = intel
                .and_then(|i| i.dns_status)
                .map_or(false, |s| s.is_dead());
            let is_disp = disposable_set.contains(&domain);
            let is_susp_tld = tld_set
                .iter()
                .any(|tld| domain.to_lowercase().ends_with(tld));

            let is_hard_spam =
                is_dead || has_server_spam_flag || has_return_path_dissonance || is_disp;

            if protected_count > 0 && !is_hard_spam {
                continue;
            }

            if is_dead {
                dead_domains.push((domain, email_count, size));
            } else if has_server_spam_flag {
                server_spam_domains.push((domain, email_count, size));
            } else if is_disp {
                disposable_candidates.push((domain, email_count, size));
            } else if has_return_path_dissonance || is_susp_tld {
                structural_domains.push((domain, email_count, size));
            } else {
                continue;
            }

            total_domains += 1;
            total_emails += email_count;
            total_size += size;
        }

        let make_group = |category: OrphanCategory, items: Vec<(String, u64, u64)>| {
            let domain_count = items.len();
            let email_count = items.iter().map(|(_, c, _)| *c).sum();
            let total_size = items.iter().map(|(_, _, s)| *s).sum();
            let domains = items.into_iter().map(|(d, _, _)| d).collect();
            OrphanCategoryGroup {
                category,
                label: category.label().to_string(),
                description: category.description().to_string(),
                domain_count,
                email_count,
                total_size,
                domains,
            }
        };

        let groups = vec![
            make_group(OrphanCategory::DeadDomain, dead_domains),
            make_group(OrphanCategory::ServerSpamVerdict, server_spam_domains),
            make_group(OrphanCategory::StructuralAnomaly, structural_domains),
            make_group(OrphanCategory::DisposableDomain, disposable_candidates),
        ];

        Ok(OrphanSummaryReport {
            threshold,
            account_id,
            total_domains,
            total_emails,
            total_size,
            dns_analyzed_count,
            dns_pending_count,
            groups,
        })
    }

    /// Returns list of (folder_path, uid, size_bytes) for orphan domains eligible for purge.
    pub fn get_orphan_candidates_for_purge(
        &self,
        account_id: i64,
        category: Option<OrphanCategory>,
        threshold: u64,
    ) -> Result<Vec<(String, u32, u64)>, StorageError> {
        let summary = self.get_orphan_summary(Some(account_id), threshold)?;
        let domains: Vec<String> = if let Some(cat) = category {
            summary
                .groups
                .into_iter()
                .find(|g| g.category == cat)
                .map(|g| g.domains)
                .unwrap_or_default()
        } else {
            let mut all_domains = Vec::new();
            for g in summary.groups {
                all_domains.extend(g.domains);
            }
            all_domains.sort();
            all_domains.dedup();
            all_domains
        };

        if domains.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut candidates = Vec::new();
        let mut stmt = conn.prepare(
            "SELECT folder_path, uid, size_bytes, classification FROM emails WHERE account_id = ?1 AND root_domain = ?2",
        )?;

        for domain in domains {
            let rows = stmt.query_map(params![account_id, domain], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, u64>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;

            for item in rows.flatten() {
                let (folder, uid, size_bytes, classification) = item;
                if classification == "PURGEABLE"
                    || (category == Some(OrphanCategory::DeadDomain)
                        && classification == "PROTECTED_INVOICE")
                {
                    candidates.push((folder, uid, size_bytes));
                }
            }
        }

        Ok(candidates)
    }

    /// Tags orphan domains in `tagged_orphan_domains` table for review.
    /// Returns the number of domains tagged.
    pub fn tag_orphan_domains(
        &self,
        account_id: Option<i64>,
        category: Option<OrphanCategory>,
        threshold: u64,
    ) -> Result<usize, StorageError> {
        let summary = self.get_orphan_summary(account_id, threshold)?;
        let mut to_tag: Vec<(String, String)> = Vec::new();

        for group in summary.groups {
            if let Some(target_cat) = category {
                if group.category != target_cat {
                    continue;
                }
            }
            let cat_str = group.category.to_string();
            for domain in group.domains {
                to_tag.push((domain, cat_str.clone()));
            }
        }

        if to_tag.is_empty() {
            return Ok(0);
        }

        let now = chrono::Utc::now().timestamp();
        let mut conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let tx = conn.transaction()?;

        let mut tagged_count = 0usize;
        {
            let mut insert_stmt = tx.prepare(
                "INSERT INTO tagged_orphan_domains (account_id, root_domain, orphan_category, tagged_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(account_id, root_domain) DO UPDATE SET orphan_category = ?3, tagged_at = ?4",
            )?;

            if let Some(acc_id) = account_id {
                for (domain, cat_str) in &to_tag {
                    insert_stmt.execute(params![acc_id, domain, cat_str, now])?;
                    tagged_count += 1;
                }
            } else {
                let mut acc_stmt =
                    tx.prepare("SELECT DISTINCT account_id FROM emails WHERE root_domain = ?1")?;
                for (domain, cat_str) in &to_tag {
                    let mut rows = acc_stmt.query(params![domain])?;
                    let mut acc_ids = Vec::new();
                    while let Some(row) = rows.next()? {
                        acc_ids.push(row.get::<_, i64>(0)?);
                    }
                    for acc_id in acc_ids {
                        insert_stmt.execute(params![acc_id, domain, cat_str, now])?;
                        tagged_count += 1;
                    }
                }
            }
        }

        tx.commit()?;
        Ok(tagged_count)
    }

    /// Removes an orphan tag from a domain, optionally adding it to user_trusted_domains.
    pub fn unmark_orphan_domain(
        &self,
        account_id: i64,
        root_domain: &str,
        add_to_whitelist: bool,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let tx = conn.transaction()?;

        tx.execute(
            "DELETE FROM tagged_orphan_domains WHERE account_id = ?1 AND root_domain = ?2",
            params![account_id, root_domain],
        )?;

        if add_to_whitelist {
            let now = chrono::Utc::now().timestamp();
            tx.execute(
                "INSERT INTO user_trusted_domains (account_id, root_domain, source, created_at)
                 VALUES (?1, ?2, 'user_manual', ?3)
                 ON CONFLICT(account_id, root_domain) DO NOTHING",
                params![account_id, root_domain, now],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Adds domains to user_trusted_domains (e.g. from Sent folder or manual user action).
    pub fn add_user_trusted_domains(
        &self,
        account_id: i64,
        domains: &[String],
        source: &str,
    ) -> Result<usize, StorageError> {
        if domains.is_empty() {
            return Ok(0);
        }
        let now = chrono::Utc::now().timestamp();
        let mut conn = self.conn.lock().map_err(|_| StorageError::LockPoisoned)?;
        let tx = conn.transaction()?;
        let mut count = 0;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO user_trusted_domains (account_id, root_domain, source, created_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(account_id, root_domain) DO NOTHING",
            )?;
            for d in domains {
                let trimmed = d.trim();
                if !trimmed.is_empty() {
                    count += stmt.execute(params![account_id, trimmed, source, now])?;
                }
            }
        }
        tx.commit()?;
        Ok(count)
    }
}
