//! Community Rules Updater Adapter.
//!
//! Synchronizes community anti-spam intelligence (ESP whitelist, suspicious TLDs,
//! and disposable email domains) into local SQLite storage.

use crate::domain::community_rules::{
    default_disposable_domains, default_esp_whitelist, default_suspicious_tlds,
    CommunityRulesUpdateReport,
};
use crate::storage::{MailStorageRepository, StorageError};

/// Default URL for community disposable domains blocklist.
pub const DEFAULT_DISPOSABLE_DOMAINS_URL: &str =
    "https://raw.githubusercontent.com/disposable-email-domains/disposable-email-domains/master/disposable_email_blocklist.conf";

/// Updater service for community intelligence rules.
pub struct CommunityRulesUpdater;

impl CommunityRulesUpdater {
    /// Synchronizes all community rules into the local SQLite database.
    ///
    /// If `remote_disposable_url` is provided, attempts to fetch updated disposable domains.
    /// In case of network errors, timeouts, or offline mode, falls back gracefully to built-in seeds
    /// without failing the update process.
    ///
    /// # Errors
    ///
    /// Returns `StorageError` if persisting synchronized rules to SQLite storage fails.
    pub fn update_rules(
        storage: &MailStorageRepository,
        remote_disposable_url: Option<&str>,
    ) -> Result<CommunityRulesUpdateReport, StorageError> {
        let mut disposable_domains = default_disposable_domains();
        let mut fetched_remote = false;

        if let Some(url) = remote_disposable_url {
            tracing::info!("Fetching remote disposable domains from: {url}");
            match ureq::get(url)
                .timeout(std::time::Duration::from_secs(4))
                .call()
            {
                Ok(resp) => match resp.into_string() {
                    Ok(body) => {
                        let lines: Vec<String> = body
                            .lines()
                            .map(|l| l.trim().to_lowercase())
                            .filter(|l| !l.is_empty() && !l.starts_with('#') && l.contains('.'))
                            .collect();
                        if !lines.is_empty() {
                            tracing::info!(
                                "Successfully fetched {} disposable domains from remote stream",
                                lines.len()
                            );
                            disposable_domains.extend(lines);
                            disposable_domains.sort();
                            disposable_domains.dedup();
                            fetched_remote = true;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read remote body as text: {e}");
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "Offline or unreachable remote URL ({e}), falling back to built-in seeds"
                    );
                }
            }
        }

        let esps = default_esp_whitelist();
        let tlds = default_suspicious_tlds();

        let mut report = storage.sync_community_rules(&esps, &tlds, &disposable_domains)?;

        if fetched_remote {
            report.status_message = format!(
                "Règles communautaires synchronisées avec succès (flux distant : {} domaines jetables)",
                disposable_domains.len()
            );
        } else {
            report.status_message =
                "Règles communautaires synchronisées avec succès (graines locales)".to_string();
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_community_rules_updater_offline_fallback() {
        let storage = MailStorageRepository::open_in_memory().expect("in-memory db failed");
        storage.init_schema().expect("init schema");
        let report =
            CommunityRulesUpdater::update_rules(&storage, None).expect("update_rules failed");

        assert!(report.esp_updated_count > 0);
        assert!(report.tlds_updated_count > 0);
        assert!(report.disposable_updated_count > 0);
        assert!(
            report.total_rules_count
                >= report.esp_updated_count
                    + report.tlds_updated_count
                    + report.disposable_updated_count
        );
        assert!(report.status_message.contains("graines locales"));
    }

    #[test]
    fn test_community_rules_updater_unreachable_url_falls_back() {
        let storage = MailStorageRepository::open_in_memory().expect("in-memory db failed");
        storage.init_schema().expect("init schema");
        // An unroutable / invalid domain to trigger immediate network fallback
        let report = CommunityRulesUpdater::update_rules(
            &storage,
            Some("http://invalid-url.local.test/domains.txt"),
        )
        .expect("should not fail even if network fails");

        assert!(report.disposable_updated_count > 0);
        assert!(report.status_message.contains("graines locales"));
    }
}
