use serde::{Deserialize, Serialize};

/// DNS resolution status for a root domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DnsStatus {
    /// Domain has active MX records or A/AAAA records.
    Alive,
    /// Domain does not exist in DNS (NXDOMAIN).
    DeadNxDomain,
    /// Domain has no MX and no A records.
    NoMailServer,
    /// DNS query timed out or failed with temporary network error.
    TimeoutOrError,
}

impl DnsStatus {
    /// Returns the string representation stored in SQLite.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Alive => "ALIVE",
            Self::DeadNxDomain => "DEAD_NXDOMAIN",
            Self::NoMailServer => "NO_MAIL_SERVER",
            Self::TimeoutOrError => "TIMEOUT_OR_ERROR",
        }
    }

    /// Parses a status from its database string representation.
    #[must_use]
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "ALIVE" => Self::Alive,
            "DEAD_NXDOMAIN" => Self::DeadNxDomain,
            "NO_MAIL_SERVER" => Self::NoMailServer,
            _ => Self::TimeoutOrError,
        }
    }

    /// Returns recommended adaptive TTL in seconds based on status volatility.
    #[must_use]
    pub const fn recommended_ttl_seconds(&self) -> i64 {
        match self {
            // Dead / Non-existent domains rarely resurrect quickly (30 days)
            Self::DeadNxDomain | Self::NoMailServer => 30 * 86_400,
            // Alive domains may change MX or expire (7 days)
            Self::Alive => 7 * 86_400,
            // Transient timeout or network error should be retested quickly (1 hour)
            Self::TimeoutOrError => 3_600,
        }
    }

    /// Returns true if the domain is considered dead / non-functional for mail.
    #[must_use]
    pub const fn is_dead(&self) -> bool {
        matches!(self, Self::DeadNxDomain | Self::NoMailServer)
    }
}

/// Technical spam and authentication verdict details parsed from mail headers.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SpamVerdictDetail {
    /// True if SPF check resulted in fail or softfail.
    pub spf_fail: bool,
    /// True if DKIM verification failed.
    pub dkim_fail: bool,
    /// True if DMARC alignment/policy check failed.
    pub dmarc_fail: bool,
    /// True if server marked `X-Spam-Flag: YES` or similar.
    pub x_spam_flag: bool,
    /// Numerical spam score from `SpamAssassin` or `Rspamd` if present.
    pub spam_score: Option<f32>,
    /// Microsoft 365 Spam Confidence Level (SCL) (0-9).
    pub scl_score: Option<u8>,
    /// True if From and Return-Path eTLD+1 mismatch without a recognized ESP.
    pub return_path_dissonance: bool,
    /// True if DNS lookup returned NXDOMAIN or no MX records.
    pub is_dead_domain: bool,
    /// True if root domain ends with a high-abuse TLD.
    pub is_suspicious_tld: bool,
    /// True if root domain matches a known disposable email provider.
    pub is_disposable_domain: bool,
}

impl SpamVerdictDetail {
    /// Returns true if there is at least one hard technical indication of spam or spoofing.
    #[must_use]
    pub const fn has_hard_spam_signals(&self) -> bool {
        self.is_dead_domain
            || self.spf_fail
            || self.dmarc_fail
            || self.x_spam_flag
            || match self.scl_score {
                Some(scl) => scl >= 5,
                None => false,
            }
            || match self.spam_score {
                Some(score) => score >= 4.0,
                None => false,
            }
            || self.return_path_dissonance
            || self.is_disposable_domain
    }
}

/// Category classification for one-hit wonder and long-tail spam domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrphanCategory {
    /// Domain confirmed dead or without MX records.
    DeadDomain,
    /// Explicit spam or spoofing verdict from email server (SPF/DKIM/DMARC fail, X-Spam, SCL:5+).
    ServerSpamVerdict,
    /// Structural anomalies (Return-Path dissonance vs ESP, high-risk TLD, missing To/Cc).
    StructuralAnomaly,
    /// Known disposable or burner domain from community lists.
    DisposableDomain,
}

impl OrphanCategory {
    /// Returns the database string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::DeadDomain => "DEAD_DOMAIN",
            Self::ServerSpamVerdict => "SERVER_SPAM",
            Self::StructuralAnomaly => "STRUCTURAL_ANOMALY",
            Self::DisposableDomain => "DISPOSABLE",
        }
    }

    /// Returns human-readable label for the UI.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::DeadDomain => "Domaines Morts (NXDOMAIN)",
            Self::ServerSpamVerdict => "Échecs Auth & Verdicts Serveurs",
            Self::StructuralAnomaly => "Anomalies Expéditeur & TLDs Suspects",
            Self::DisposableDomain => "Domaines Jetables (Burner)",
        }
    }

    /// Returns detailed explanation of why these emails/domains are categorized here.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::DeadDomain => {
                "Domaines n'existant plus au DNS ou sans serveur de réception d'emails."
            }
            Self::ServerSpamVerdict => {
                "Emails ayant échoué à l'authentification SPF/DKIM/DMARC ou classés spam par le serveur."
            }
            Self::StructuralAnomaly => {
                "Dissonance d'adresse de rebond (Return-Path suspect) ou extensions TLD à haut risque."
            }
            Self::DisposableDomain => {
                "Services de courriels temporaires répertoriés dans les listes de cyber-sécurité."
            }
        }
    }
}

impl std::fmt::Display for OrphanCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Summary aggregation for an orphan category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrphanCategoryGroup {
    /// Category key.
    pub category: OrphanCategory,
    /// Human readable category title.
    pub label: String,
    /// Detailed description for user guidance.
    pub description: String,
    /// Number of distinct root domains in this category.
    pub domain_count: usize,
    /// Total purgeable emails across these domains.
    pub email_count: u64,
    /// Total size in bytes for these emails.
    pub total_size: u64,
    /// List of root domains belonging to this category.
    pub domains: Vec<String>,
}

/// Global report returned to the UI for the orphan hunter feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrphanSummaryReport {
    /// Threshold max emails limit applied.
    pub threshold: u64,
    /// Account ID or None for consolidated all-accounts view.
    pub account_id: Option<i64>,
    /// Total orphan domains detected.
    pub total_domains: usize,
    /// Total purgeable emails found.
    pub total_emails: u64,
    /// Total size in bytes.
    pub total_size: u64,
    /// Number of domains with fresh cached DNS results.
    pub dns_analyzed_count: usize,
    /// Number of domains needing DNS lookup.
    pub dns_pending_count: usize,
    /// Aggregated category groups.
    pub groups: Vec<OrphanCategoryGroup>,
}

/// Unified domain intelligence record cached across all accounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedDomainIntelligence {
    /// Root domain (eTLD+1).
    pub root_domain: String,
    /// DNS status if resolved.
    pub dns_status: Option<DnsStatus>,
    /// Timestamp when DNS was checked.
    pub dns_checked_at: Option<i64>,
    /// Adaptive TTL in seconds for DNS validity.
    pub dns_ttl_seconds: i64,
    /// Whether domain is listed as disposable/burner.
    pub is_disposable: bool,
    /// Whether domain is a recognized legitimate ESP router.
    pub is_known_esp: bool,
    /// Whether domain uses a high-abuse TLD.
    pub is_suspicious_tld: bool,
    /// DNSBL status string if queried.
    pub dnsbl_status: Option<String>,
    /// Timestamp when DNSBL was queried.
    pub dnsbl_checked_at: Option<i64>,
    /// Timestamp when this record was updated.
    pub updated_at: i64,
}

impl SharedDomainIntelligence {
    /// Returns true if the DNS resolution status is still fresh according to its adaptive TTL.
    #[must_use]
    pub const fn is_dns_fresh(&self, current_timestamp: i64) -> bool {
        if let Some(checked_at) = self.dns_checked_at {
            current_timestamp < checked_at.saturating_add(self.dns_ttl_seconds)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dns_status_ttls() {
        assert_eq!(
            DnsStatus::DeadNxDomain.recommended_ttl_seconds(),
            30 * 86_400
        );
        assert_eq!(
            DnsStatus::NoMailServer.recommended_ttl_seconds(),
            30 * 86_400
        );
        assert_eq!(DnsStatus::Alive.recommended_ttl_seconds(), 7 * 86_400);
        assert_eq!(DnsStatus::TimeoutOrError.recommended_ttl_seconds(), 3_600);
    }

    #[test]
    fn test_dns_status_is_dead() {
        assert!(DnsStatus::DeadNxDomain.is_dead());
        assert!(DnsStatus::NoMailServer.is_dead());
        assert!(!DnsStatus::Alive.is_dead());
        assert!(!DnsStatus::TimeoutOrError.is_dead());
    }

    #[test]
    fn test_shared_domain_intelligence_freshness() {
        let now = 1_700_000_000;
        let intel = SharedDomainIntelligence {
            root_domain: "example.com".to_string(),
            dns_status: Some(DnsStatus::Alive),
            dns_checked_at: Some(now - 1_000),
            dns_ttl_seconds: 7 * 86_400,
            is_disposable: false,
            is_known_esp: false,
            is_suspicious_tld: false,
            dnsbl_status: None,
            dnsbl_checked_at: None,
            updated_at: now,
        };
        assert!(intel.is_dns_fresh(now));

        let expired = SharedDomainIntelligence {
            dns_checked_at: Some(now - 800_000),
            dns_ttl_seconds: 604_800,
            ..intel
        };
        assert!(!expired.is_dns_fresh(now));
    }

    #[test]
    fn test_spam_verdict_hard_signals() {
        let mut detail = SpamVerdictDetail::default();
        assert!(!detail.has_hard_spam_signals());

        detail.spf_fail = true;
        assert!(detail.has_hard_spam_signals());

        detail.spf_fail = false;
        detail.is_dead_domain = true;
        assert!(detail.has_hard_spam_signals());

        detail.is_dead_domain = false;
        detail.scl_score = Some(6);
        assert!(detail.has_hard_spam_signals());
    }
}
