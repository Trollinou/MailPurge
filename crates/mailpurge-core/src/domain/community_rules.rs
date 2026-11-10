use serde::{Deserialize, Serialize};

/// An entry in the legitimate Email Service Provider (ESP) whitelist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EspWhitelistEntry {
    /// Root domain of the ESP (e.g. `sendgrid.net`).
    pub root_domain: String,
    /// Human-friendly provider name (e.g. `SendGrid`).
    pub provider_name: String,
    /// Whether this is a default built-in seed entry.
    pub is_builtin: bool,
    /// Whether this entry was added by the user.
    pub is_user_added: bool,
    /// Whether this whitelist rule is active.
    pub is_enabled: bool,
    /// Timestamp when this entry was created or updated.
    pub updated_at: i64,
}

/// An entry in the suspicious TLD list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuspiciousTldEntry {
    /// TLD string with leading dot (e.g. `.xyz`).
    pub tld: String,
    /// Risk level (`HIGH` or `MEDIUM`).
    pub risk_level: String,
    /// Whether this is a default built-in seed.
    pub is_builtin: bool,
    /// Whether this rule is currently enabled.
    pub is_enabled: bool,
    /// Timestamp when this rule was updated.
    pub updated_at: i64,
}

/// Report returned after on-demand or periodic synchronization of community rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityRulesUpdateReport {
    /// Number of ESP entries updated or added.
    pub esp_updated_count: usize,
    /// Number of TLD entries updated or added.
    pub tlds_updated_count: usize,
    /// Number of disposable domains updated.
    pub disposable_updated_count: usize,
    /// Total active rules count.
    pub total_rules_count: usize,
    /// Timestamp of last successful update.
    pub last_updated_at: i64,
    /// Summary status message.
    pub status_message: String,
}

/// Default built-in seeds for reputable Email Service Providers (ESP / Return-Path routers).
#[must_use]
pub fn default_esp_whitelist() -> Vec<EspWhitelistEntry> {
    let now = 1_700_000_000;
    let list = [
        ("sendgrid.net", "SendGrid (Twilio)"),
        ("sendgrid.info", "SendGrid"),
        ("sendgrid.biz", "SendGrid"),
        ("mcsv.net", "Mailchimp"),
        ("mailchimpapp.net", "Mailchimp"),
        ("mandrillapp.com", "Mandrill (Mailchimp)"),
        ("sendinblue.com", "Brevo / Sendinblue"),
        ("sib.com", "Brevo"),
        ("brevo.com", "Brevo"),
        ("amazonses.com", "Amazon SES"),
        ("aws-ses.com", "Amazon SES"),
        ("mailgun.org", "Mailgun"),
        ("mailgun.net", "Mailgun"),
        ("mailgun.com", "Mailgun"),
        ("postmarkapp.com", "Postmark (ActiveCampaign)"),
        ("wildbit.com", "Postmark"),
        ("sparkpostmail.com", "SparkPost (MessageBird)"),
        ("hubspotemail.net", "HubSpot"),
        ("hubspot.com", "HubSpot"),
        ("exacttarget.com", "Salesforce Marketing Cloud"),
        ("salesforce.com", "Salesforce"),
        ("outlook.com", "Microsoft 365 / Exchange"),
        ("microsoft.com", "Microsoft"),
        ("google.com", "Google Workspace"),
        ("gmail.com", "Google"),
        ("constantcontact.com", "Constant Contact"),
        ("rsys2.com", "Oracle Responsys"),
        ("rsys5.com", "Oracle Responsys"),
        ("klaviyo.com", "Klaviyo"),
        ("klaviyomail.com", "Klaviyo"),
        ("createsend.com", "Campaign Monitor"),
        ("cmail1.com", "Campaign Monitor"),
        ("cmail2.com", "Campaign Monitor"),
        ("infobip.com", "Infobip"),
        ("mailjet.com", "Mailjet (Sinch)"),
    ];

    list.iter()
        .map(|(domain, name)| EspWhitelistEntry {
            root_domain: (*domain).to_string(),
            provider_name: (*name).to_string(),
            is_builtin: true,
            is_user_added: false,
            is_enabled: true,
            updated_at: now,
        })
        .collect()
}

/// Default built-in seeds for high-abuse TLDs based on Spamhaus and SURBL reports.
#[must_use]
pub fn default_suspicious_tlds() -> Vec<SuspiciousTldEntry> {
    let now = 1_700_000_000;
    let list = [
        (".xyz", "HIGH"),
        (".top", "HIGH"),
        (".loan", "HIGH"),
        (".work", "HIGH"),
        (".click", "HIGH"),
        (".date", "HIGH"),
        (".racing", "HIGH"),
        (".win", "HIGH"),
        (".bid", "HIGH"),
        (".stream", "HIGH"),
        (".gq", "HIGH"),
        (".tk", "HIGH"),
        (".cf", "HIGH"),
        (".ml", "HIGH"),
        (".buzz", "HIGH"),
        (".icu", "HIGH"),
        (".fit", "MEDIUM"),
        (".rest", "MEDIUM"),
        (".surf", "MEDIUM"),
        (".monster", "HIGH"),
        (".sbs", "HIGH"),
        (".cfd", "HIGH"),
        (".bar", "MEDIUM"),
        (".wiki", "MEDIUM"),
        (".beauty", "HIGH"),
        (".skin", "HIGH"),
        (".hair", "HIGH"),
        (".quest", "HIGH"),
        (".link", "MEDIUM"),
        (".live", "MEDIUM"),
        (".guru", "MEDIUM"),
        (".shop", "MEDIUM"),
        (".online", "MEDIUM"),
        (".site", "MEDIUM"),
        (".support", "MEDIUM"),
    ];

    list.iter()
        .map(|(tld, risk)| SuspiciousTldEntry {
            tld: (*tld).to_string(),
            risk_level: (*risk).to_string(),
            is_builtin: true,
            is_enabled: true,
            updated_at: now,
        })
        .collect()
}

/// Default seed of well-known burner and disposable email domains.
#[must_use]
pub fn default_disposable_domains() -> Vec<String> {
    vec![
        "10minutemail.com".to_string(),
        "guerrillamail.com".to_string(),
        "mailinator.com".to_string(),
        "tempmail.com".to_string(),
        "trashmail.com".to_string(),
        "yopmail.com".to_string(),
        "dispostable.com".to_string(),
        "sharklasers.com".to_string(),
        "throwawaymail.com".to_string(),
        "fakemailgenerator.com".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_esp_whitelist_contains_major_providers() {
        let esps = default_esp_whitelist();
        assert!(esps.iter().any(|e| e.root_domain == "sendgrid.net"));
        assert!(esps.iter().any(|e| e.root_domain == "mcsv.net"));
        assert!(esps.iter().any(|e| e.root_domain == "amazonses.com"));
        assert!(esps.iter().any(|e| e.root_domain == "brevo.com"));
    }

    #[test]
    fn test_default_suspicious_tlds_contains_known_spamhaus_tlds() {
        let tlds = default_suspicious_tlds();
        assert!(tlds.iter().any(|t| t.tld == ".xyz"));
        assert!(tlds.iter().any(|t| t.tld == ".top"));
        assert!(tlds.iter().any(|t| t.tld == ".loan"));
        assert!(tlds.iter().any(|t| t.tld == ".click"));
    }
}
