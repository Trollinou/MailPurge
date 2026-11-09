use crate::folder_scanner::FolderInfo;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEmailEnvelope {
    pub uid: u32,
    pub message_id: Option<String>,
    pub subject: String,
    pub from: String,
    pub date_timestamp: i64,
    pub size_bytes: u64,
    pub is_flagged: bool,
    pub has_unsubscribe: bool,
    pub attachment_names: Vec<String>,
    pub list_id: Option<String>,
    pub list_unsubscribe: Option<String>,
    pub list_unsubscribe_post: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MailProviderError {
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("Folder not found: {0}")]
    FolderNotFound(String),
    #[error("IMAP connection error: {0}")]
    ConnectionError(String),
    #[error("IMAP protocol error: {0}")]
    ProtocolError(String),
    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

/// Hexagonal port interface defining mail provider interactions (IMAP, XOAUTH2, Mock).
#[async_trait]
pub trait MailProviderPort: Send + Sync {
    /// Returns the list of all available mail folders on the server.
    async fn list_folders(&self) -> Result<Vec<FolderInfo>, MailProviderError>;

    /// Returns the total message count in the specified folder.
    async fn get_folder_message_count(&self, folder_path: &str) -> Result<u32, MailProviderError> {
        self.fetch_envelopes(folder_path)
            .await
            .map(|v| v.len() as u32)
    }

    /// Fetches all email envelopes / metadata from a specific folder.
    async fn fetch_envelopes(
        &self,
        folder_path: &str,
    ) -> Result<Vec<RawEmailEnvelope>, MailProviderError>;

    /// Fetches email envelopes within a 1-based sequence range `[start_seq, end_seq]`.
    async fn fetch_envelopes_range(
        &self,
        folder_path: &str,
        start_seq: u32,
        end_seq: u32,
    ) -> Result<Vec<RawEmailEnvelope>, MailProviderError> {
        let all = self.fetch_envelopes(folder_path).await?;
        let start_idx = (start_seq.saturating_sub(1)) as usize;
        let end_idx = (end_seq as usize).min(all.len());
        if start_idx >= all.len() || start_idx >= end_idx {
            return Ok(Vec::new());
        }
        Ok(all[start_idx..end_idx].to_vec())
    }

    /// Moves a list of UIDs from a folder to the trash folder. Returns count of moved items.
    async fn move_to_trash(
        &mut self,
        folder_path: &str,
        uids: &[u32],
        trash_folder_path: &str,
    ) -> Result<usize, MailProviderError>;

    /// Empties all items from the specified trash folder. Returns count of deleted items.
    async fn empty_trash(&mut self, trash_folder_path: &str) -> Result<usize, MailProviderError>;

    /// Fetches an on-demand body preview snippet for a specific email UID without marking it as read.
    async fn fetch_body_snippet(
        &mut self,
        folder_path: &str,
        uid: u32,
    ) -> Result<String, MailProviderError> {
        let _ = (folder_path, uid);
        Ok(String::new())
    }
}
