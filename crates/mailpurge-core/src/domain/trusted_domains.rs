//! Whitelist and trusted domain verification engine.
//!
//! Provides institutional domain validation (.gouv.fr, .sante.fr, .edu, academies, universities)
//! and static embedded allowlists for major legitimate services (Google, public services, French banks).

use std::collections::HashSet;
use std::sync::LazyLock;

static TRUSTED_DOMAINS_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    include_str!("../../assets/trusted_domains.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
});

/// Returns true if the domain belongs to a recognized sovereign, institutional, health, or academic body.
#[must_use]
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub fn is_institutional_domain(domain: &str) -> bool {
    let lower = domain.trim().to_lowercase();
    if lower.ends_with(".gouv.fr")
        || lower.ends_with(".etat.fr")
        || lower.ends_with(".sante.fr")
        || lower.ends_with(".mssante.fr")
        || lower.ends_with(".edu")
    {
        return true;
    }

    // Academic and university domains in France (e.g., *.ac-paris.fr, *.univ-lyon1.fr)
    if lower.ends_with(".fr") {
        let parts: Vec<&str> = lower.split('.').collect();
        if parts.len() >= 2 {
            let sld = parts[parts.len() - 2];
            if sld.starts_with("ac-") || sld.starts_with("univ-") {
                return true;
            }
        }
    }

    false
}

/// Returns true if the root domain is listed in the static embedded trusted domains whitelist.
#[must_use]
pub fn is_embedded_trusted_domain(root_domain: &str) -> bool {
    let lower = root_domain.trim().to_lowercase();
    TRUSTED_DOMAINS_SET.contains(lower.as_str())
}

/// Checks if a domain is trusted, combining institutional rules and embedded allowlist.
#[must_use]
pub fn is_trusted_domain(root_domain: &str, full_domain: &str) -> bool {
    is_institutional_domain(full_domain)
        || is_institutional_domain(root_domain)
        || is_embedded_trusted_domain(root_domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_institutional_domains() {
        assert!(is_institutional_domain("impots.gouv.fr"));
        assert!(is_institutional_domain("ants.gouv.fr"));
        assert!(is_institutional_domain("prefecture.etat.fr"));
        assert!(is_institutional_domain("laboratoire.sante.fr"));
        assert!(is_institutional_domain("hopital.mssante.fr"));
        assert!(is_institutional_domain("stanford.edu"));
        assert!(is_institutional_domain("mail.ac-paris.fr"));
        assert!(is_institutional_domain("recherche.univ-lyon1.fr"));

        assert!(!is_institutional_domain("fake-impots-gouv.com"));
        assert!(!is_institutional_domain("spammer.xyz"));
    }

    #[test]
    fn test_embedded_trusted_domains() {
        assert!(is_embedded_trusted_domain("google.com"));
        assert!(is_embedded_trusted_domain("google.fr"));
        assert!(is_embedded_trusted_domain("ameli.fr"));
        assert!(is_embedded_trusted_domain("sncf.com"));
        assert!(is_embedded_trusted_domain("doctolib.fr"));
        assert!(is_embedded_trusted_domain("credit-agricole.fr"));
        assert!(is_embedded_trusted_domain("boursobank.com"));
        assert!(is_embedded_trusted_domain("amazon.fr"));

        assert!(!is_embedded_trusted_domain("burner-spammer.top"));
        assert!(!is_embedded_trusted_domain("infosia.fr"));
    }

    #[test]
    fn test_combined_is_trusted_domain() {
        assert!(is_trusted_domain("google.com", "mail.google.com"));
        assert!(is_trusted_domain("gouv.fr", "impots.gouv.fr"));
        assert!(!is_trusted_domain(
            "unknown-spammer.com",
            "unknown-spammer.com"
        ));
    }
}
