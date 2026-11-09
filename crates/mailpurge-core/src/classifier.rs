use deunicode::deunicode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmailClassification {
    Purgeable,
    ProtectedInvoice,
    ProtectedFlagged,
    Recent,
}

impl EmailClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Purgeable => "PURGEABLE",
            Self::ProtectedInvoice => "PROTECTED_INVOICE",
            Self::ProtectedFlagged => "PROTECTED_FLAGGED",
            Self::Recent => "RECENT",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClassifierConfig {
    pub retention_days: u32,
    pub transactional_keywords: Vec<String>,
    pub protected_extensions: Vec<String>,
}

impl Default for ClassifierConfig {
    fn default() -> Self {
        let keywords = vec![
            // French
            "facture",
            "commande",
            "recu",
            "billet",
            "reservation",
            "garantie",
            "contrat",
            "avoir",
            // English
            "invoice",
            "order",
            "receipt",
            "ticket",
            "booking",
            "reservation",
            "warranty",
            "contract",
            "credit note",
        ];

        Self {
            retention_days: 60,
            transactional_keywords: keywords.into_iter().map(String::from).collect(),
            protected_extensions: vec![".pdf".to_string()],
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmailMetadataInput<'a> {
    pub uid: u32,
    pub subject: &'a str,
    pub received_at_timestamp: i64,
    pub is_flagged: bool,
    pub has_unsubscribe: bool,
    pub attachment_names: &'a [String],
}

/// Classifies an email according to retention, flags, transactional content, and attachments.
pub fn classify_email(
    metadata: &EmailMetadataInput,
    config: &ClassifierConfig,
    reference_timestamp: i64,
) -> EmailClassification {
    // 1. Starred / Flagged emails are permanently protected
    if metadata.is_flagged {
        return EmailClassification::ProtectedFlagged;
    }

    // 2. Retention window check (< retention_days is Recent)
    let retention_seconds = (config.retention_days as i64) * 86_400;
    let age_seconds = reference_timestamp - metadata.received_at_timestamp;
    if age_seconds < retention_seconds {
        return EmailClassification::Recent;
    }

    // 3. Transactional keywords check in subject (case and accent insensitive)
    let normalized_subject = deunicode(metadata.subject).to_lowercase();
    let has_transactional_keyword = config.transactional_keywords.iter().any(|keyword| {
        let normalized_kw = deunicode(keyword).to_lowercase();
        normalized_subject.contains(&normalized_kw)
    });

    if has_transactional_keyword {
        return EmailClassification::ProtectedInvoice;
    }

    // 4. Protected attachments (e.g. PDF) without newsletter/unsubscribe header
    let has_protected_attachment = metadata.attachment_names.iter().any(|name| {
        let lower_name = name.to_lowercase();
        config
            .protected_extensions
            .iter()
            .any(|ext| lower_name.ends_with(&ext.to_lowercase()))
    });

    if has_protected_attachment && !metadata.has_unsubscribe {
        return EmailClassification::ProtectedInvoice;
    }

    // 5. Default: Purgeable
    EmailClassification::Purgeable
}
