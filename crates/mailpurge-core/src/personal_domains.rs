/// Known personal email providers, French ISPs, and global webmails.
pub static DEFAULT_PERSONAL_DOMAINS: &[&str] = &[
    // French ISPs & Webmails
    "orange.fr",
    "wanadoo.fr",
    "free.fr",
    "sfr.fr",
    "neuf.fr",
    "laposte.net",
    "numericable.fr",
    "bouyguestelecom.fr",
    "bbox.fr",
    "club-internet.fr",
    "cegetel.net",
    "aliceadsl.fr",
    "noos.fr",
    // International Webmails
    "gmail.com",
    "googlemail.com",
    "outlook.com",
    "hotmail.com",
    "hotmail.fr",
    "live.com",
    "live.fr",
    "msn.com",
    "yahoo.com",
    "yahoo.fr",
    "ymail.com",
    "icloud.com",
    "me.com",
    "mac.com",
    "proton.me",
    "protonmail.com",
    "gmx.com",
    "gmx.fr",
    "zoho.com",
    "mail.com",
    "aol.com",
    "aol.fr",
    "fastmail.com",
    "tutanota.com",
    "tutamail.com",
];

/// Returns true if the given root domain corresponds to a known personal/freemail/ISP provider.
pub fn is_personal_email_provider(domain: &str) -> bool {
    let lower = domain.trim().to_lowercase();
    DEFAULT_PERSONAL_DOMAINS.iter().any(|&d| d == lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_french_isps() {
        assert!(is_personal_email_provider("orange.fr"));
        assert!(is_personal_email_provider("wanadoo.fr"));
        assert!(is_personal_email_provider("free.fr"));
        assert!(is_personal_email_provider("sfr.fr"));
        assert!(is_personal_email_provider("laposte.net"));
    }

    #[test]
    fn test_detects_global_webmails() {
        assert!(is_personal_email_provider("gmail.com"));
        assert!(is_personal_email_provider("outlook.com"));
        assert!(is_personal_email_provider("yahoo.com"));
        assert!(is_personal_email_provider("icloud.com"));
        assert!(is_personal_email_provider("proton.me"));
    }

    #[test]
    fn test_rejects_commercial_domains() {
        assert!(!is_personal_email_provider("amazon.co.uk"));
        assert!(!is_personal_email_provider("fnac.com"));
        assert!(!is_personal_email_provider("ldlc.com"));
        assert!(!is_personal_email_provider("boulanger.fr"));
        assert!(!is_personal_email_provider("linkedin.com"));
    }
}
