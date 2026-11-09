use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::classifier::EmailClassification;
use crate::provider_port::{MailProviderError, MailProviderPort};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeCandidate {
    pub folder_path: String,
    pub uid: u32,
    pub size_bytes: u64,
    pub classification: EmailClassification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeConfig {
    pub batch_size: usize,
    pub dry_run: bool,
}

impl Default for PurgeConfig {
    fn default() -> Self {
        Self {
            batch_size: 500,
            dry_run: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeReport {
    pub total_candidates: usize,
    pub purged_count: usize,
    pub purged_bytes: u64,
    pub skipped_protected_count: usize,
    pub batches_executed: usize,
    pub is_dry_run: bool,
    pub details_by_folder: HashMap<String, Vec<u32>>,
}

pub struct PurgeExecutor;

impl PurgeExecutor {
    /// Executes a purge operation safely:
    /// - Strictly filters out any non-purgeable email (protected invoices, flagged, recent).
    /// - Groups purgeable emails by folder.
    /// - Chunks operations into batches of `batch_size` (max 500 by default).
    /// - If `dry_run` is true, simulates execution without calling the provider.
    pub fn execute<P: MailProviderPort + ?Sized>(
        provider: &mut P,
        candidates: &[PurgeCandidate],
        trash_folder_path: &str,
        config: &PurgeConfig,
    ) -> Result<PurgeReport, MailProviderError> {
        let total_candidates = candidates.len();

        let mut purgeable_by_folder: HashMap<String, Vec<&PurgeCandidate>> = HashMap::new();
        let mut skipped_protected_count = 0;
        let mut purged_bytes = 0;
        let mut purged_count = 0;

        for candidate in candidates {
            if candidate.classification == EmailClassification::Purgeable {
                purgeable_by_folder
                    .entry(candidate.folder_path.clone())
                    .or_default()
                    .push(candidate);
                purged_bytes += candidate.size_bytes;
                purged_count += 1;
            } else {
                skipped_protected_count += 1;
            }
        }

        let mut batches_executed = 0;
        let mut details_by_folder = HashMap::new();

        let batch_size = if config.batch_size == 0 {
            500
        } else {
            config.batch_size
        };

        for (folder, items) in purgeable_by_folder {
            let uids: Vec<u32> = items.iter().map(|item| item.uid).collect();
            details_by_folder.insert(folder.clone(), uids.clone());

            let chunks: Vec<&[u32]> = uids.chunks(batch_size).collect();
            batches_executed += chunks.len();

            if !config.dry_run {
                for chunk in chunks {
                    provider.move_to_trash(&folder, chunk, trash_folder_path)?;
                }
            }
        }

        Ok(PurgeReport {
            total_candidates,
            purged_count,
            purged_bytes,
            skipped_protected_count,
            batches_executed,
            is_dry_run: config.dry_run,
            details_by_folder,
        })
    }
}
