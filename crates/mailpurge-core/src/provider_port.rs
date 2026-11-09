use crate::folder_scanner::FolderInfo;
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
pub trait MailProviderPort {
    /// Returns the list of all available mail folders on the server.
    fn list_folders(&self) -> Result<Vec<FolderInfo>, MailProviderError>;

    /// Fetches all email envelopes / metadata from a specific folder.
    fn fetch_envelopes(&self, folder_path: &str) -> Result<Vec<RawEmailEnvelope>, MailProviderError>;

    /// Moves a list of UIDs from a folder to the trash folder. Returns count of moved items.
    fn move_to_trash(
        &mut self,
        folder_path: &str,
        uids: &[u32],
        trash_folder_path: &str,
    ) -> Result<usize, MailProviderError>;

    /// Empties all items from the specified trash folder. Returns count of deleted items.
    fn empty_trash(&mut self, trash_folder_path: &str) -> Result<usize, MailProviderError>;
}
