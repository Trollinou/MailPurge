use std::collections::HashMap;

use crate::folder_scanner::FolderInfo;
use crate::provider_port::{MailProviderError, MailProviderPort, RawEmailEnvelope};

#[derive(Debug, Default, Clone)]
pub struct MockMailProvider {
    folders: Vec<FolderInfo>,
    emails_by_folder: HashMap<String, Vec<RawEmailEnvelope>>,
    pub operations_log: Vec<String>,
}

impl MockMailProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_folder(&mut self, folder: FolderInfo) {
        if !self.folders.iter().any(|f| f.path == folder.path) {
            self.folders.push(folder);
        }
    }

    pub fn add_email(&mut self, folder_path: &str, envelope: RawEmailEnvelope) {
        self.emails_by_folder
            .entry(folder_path.to_string())
            .or_default()
            .push(envelope);
    }
}

impl MailProviderPort for MockMailProvider {
    fn list_folders(&self) -> Result<Vec<FolderInfo>, MailProviderError> {
        Ok(self.folders.clone())
    }

    fn get_folder_message_count(&self, folder_path: &str) -> Result<u32, MailProviderError> {
        match self.emails_by_folder.get(folder_path) {
            Some(emails) => Ok(emails.len() as u32),
            None => {
                if self.folders.iter().any(|f| f.path == folder_path) {
                    Ok(0)
                } else {
                    Err(MailProviderError::FolderNotFound(folder_path.to_string()))
                }
            }
        }
    }

    fn fetch_envelopes(
        &self,
        folder_path: &str,
    ) -> Result<Vec<RawEmailEnvelope>, MailProviderError> {
        match self.emails_by_folder.get(folder_path) {
            Some(emails) => Ok(emails.clone()),
            None => {
                // If folder exists in list but has no emails yet, return empty vec
                if self.folders.iter().any(|f| f.path == folder_path) {
                    Ok(Vec::new())
                } else {
                    Err(MailProviderError::FolderNotFound(folder_path.to_string()))
                }
            }
        }
    }

    fn fetch_envelopes_range(
        &self,
        folder_path: &str,
        start_seq: u32,
        end_seq: u32,
    ) -> Result<Vec<RawEmailEnvelope>, MailProviderError> {
        let emails = match self.emails_by_folder.get(folder_path) {
            Some(e) => e,
            None => {
                if self.folders.iter().any(|f| f.path == folder_path) {
                    return Ok(Vec::new());
                }
                return Err(MailProviderError::FolderNotFound(folder_path.to_string()));
            }
        };

        if start_seq == 0 || start_seq > end_seq {
            return Ok(Vec::new());
        }

        let start_idx = (start_seq.saturating_sub(1)) as usize;
        let end_idx = (end_seq as usize).min(emails.len());
        if start_idx >= emails.len() || start_idx >= end_idx {
            return Ok(Vec::new());
        }

        Ok(emails[start_idx..end_idx].to_vec())
    }

    fn move_to_trash(
        &mut self,
        folder_path: &str,
        uids: &[u32],
        trash_folder_path: &str,
    ) -> Result<usize, MailProviderError> {
        let source_emails = self
            .emails_by_folder
            .get_mut(folder_path)
            .ok_or_else(|| MailProviderError::FolderNotFound(folder_path.to_string()))?;

        let mut to_move = Vec::new();
        source_emails.retain(|email| {
            if uids.contains(&email.uid) {
                to_move.push(email.clone());
                false
            } else {
                true
            }
        });

        let moved_count = to_move.len();
        self.emails_by_folder
            .entry(trash_folder_path.to_string())
            .or_default()
            .extend(to_move);

        self.operations_log.push(format!(
            "move_to_trash from '{}' to '{}' ({} items)",
            folder_path, trash_folder_path, moved_count
        ));

        Ok(moved_count)
    }

    fn empty_trash(&mut self, trash_folder_path: &str) -> Result<usize, MailProviderError> {
        let trash_emails = self
            .emails_by_folder
            .get_mut(trash_folder_path)
            .ok_or_else(|| MailProviderError::FolderNotFound(trash_folder_path.to_string()))?;

        let count = trash_emails.len();
        trash_emails.clear();

        self.operations_log.push(format!(
            "empty_trash '{}' ({} items)",
            trash_folder_path, count
        ));

        Ok(count)
    }
}
