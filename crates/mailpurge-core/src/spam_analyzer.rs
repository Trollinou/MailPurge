use std::collections::HashSet;

use crate::domain::orphan_spam::{DnsStatus, SpamVerdictDetail};
use crate::domain_extractor::extract_sender_domain;

/// Analyzes an email's headers, DNS status, and routing against community rules to detect spam or spoofing.
#[must_use]
pub fn analyze_email_spam_signals(
    root_domain: &str,
    sender_address: &str,
    return_path: Option<&str>,
    auth_results: Option<&str>,
    spam_headers: Option<&str>,
    esp_whitelist: &HashSet<String>,
    suspicious_tlds: &HashSet<String>,
    is_disposable_domain: bool,
    dns_status: Option<DnsStatus>,
) -> SpamVerdictDetail {
    let mut detail = SpamVerdictDetail {
        is_disposable_domain,
        ..Default::default()
    };

    // 1. DNS Dead Domain check
    if let Some(status) = dns_status {
        if status.is_dead() {
            detail.is_dead_domain = true;
        }
    }

    // 2. Authentication-Results parsing (SPF, DKIM, DMARC)
    if let Some(ar) = auth_results {
        let lower = ar.to_lowercase();
        if lower.contains("spf=fail") || lower.contains("spf=softfail") {
            detail.spf_fail = true;
        }
        if lower.contains("dkim=fail") {
            detail.dkim_fail = true;
        }
        if lower.contains("dmarc=fail") {
            detail.dmarc_fail = true;
        }
    }

    // 3. Spam verdict headers parsing (X-Spam-Status, X-Spam-Flag, X-Forefront-Antispam-Report)
    if let Some(sh) = spam_headers {
        let lower = sh.to_lowercase();

        if lower.contains("x-spam-flag: yes") || lower.contains("x-spam-status: yes") {
            detail.x_spam_flag = true;
        }

        // Parse score if present: e.g. "score=6.5"
        if let Some(pos) = lower.find("score=") {
            let after = &lower[pos + 6..];
            let num_str: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(score) = num_str.parse::<f32>() {
                detail.spam_score = Some(score);
                if score >= 4.0 {
                    detail.x_spam_flag = true;
                }
            }
        }

        // Parse Forefront SCL: e.g. "SCL:6"
        if let Some(pos) = lower.find("scl:") {
            let after = &lower[pos + 4..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(scl) = num_str.parse::<u8>() {
                detail.scl_score = Some(scl);
            }
        }
    }

    // 4. Return-Path dissonance (Bounce Address spoofing detection)
    if let Some(rp) = return_path {
        let sender_root = extract_sender_domain(sender_address)
            .map(|s| s.root_domain)
            .unwrap_or_default();
        let rp_root = extract_sender_domain(rp)
            .map(|s| s.root_domain)
            .unwrap_or_default();

        if !sender_root.is_empty()
            && !rp_root.is_empty()
            && sender_root != rp_root
            && !esp_whitelist.contains(&rp_root)
        {
            detail.return_path_dissonance = true;
        }
    }

    // 5. Suspicious TLD check
    let lower_domain = root_domain.to_lowercase();
    if suspicious_tlds
        .iter()
        .any(|tld| lower_domain.ends_with(tld))
    {
        detail.is_suspicious_tld = true;
    }

    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spf_and_dkim_failures_detected() {
        let esps = HashSet::new();
        let tlds = HashSet::new();

        let detail = analyze_email_spam_signals(
            "fake-bank.com",
            "security@fake-bank.com",
            Some("bounce@compromised.net"),
            Some("mx.google.com; spf=fail (IP not permitted); dkim=fail"),
            Some("X-Spam-Flag: YES; score=5.8"),
            &esps,
            &tlds,
            false,
            Some(DnsStatus::Alive),
        );

        assert!(detail.spf_fail);
        assert!(detail.dkim_fail);
        assert!(detail.x_spam_flag);
        assert_eq!(detail.spam_score, Some(5.8));
        assert!(detail.return_path_dissonance);
        assert!(detail.has_hard_spam_signals());
    }

    #[test]
    fn test_return_path_dissonance_whitelisted_for_legit_esp() {
        let mut esps = HashSet::new();
        esps.insert("sendgrid.net".to_string());
        let tlds = HashSet::new();

        let detail = analyze_email_spam_signals(
            "airbnb.com",
            "automated@airbnb.com",
            Some("bounces+1234@sendgrid.net"),
            Some("spf=pass dkim=pass"),
            None,
            &esps,
            &tlds,
            false,
            Some(DnsStatus::Alive),
        );

        assert!(!detail.return_path_dissonance);
        assert!(!detail.has_hard_spam_signals());
    }

    #[test]
    fn test_dead_domain_sets_hard_signal() {
        let esps = HashSet::new();
        let tlds = HashSet::new();

        let detail = analyze_email_spam_signals(
            "expired-store-2015.com",
            "contact@expired-store-2015.com",
            None,
            None,
            None,
            &esps,
            &tlds,
            false,
            Some(DnsStatus::DeadNxDomain),
        );

        assert!(detail.is_dead_domain);
        assert!(detail.has_hard_spam_signals());
    }
}
