use deunicode::deunicode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FolderAttribute {
    Inbox,
    Trash,
    Sent,
    Drafts,
    Junk,
    All,
    Archive,
    NoSelect,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderInfo {
    pub path: String,
    pub delimiter: Option<char>,
    pub attributes: Vec<FolderAttribute>,
}

impl FolderInfo {
    pub fn new(path: impl Into<String>, attributes: Vec<FolderAttribute>) -> Self {
        Self {
            path: path.into(),
            delimiter: Some('/'),
            attributes,
        }
    }

    /// Determines if this folder represents the Trash / Corbeille folder.
    pub fn is_trash(&self) -> bool {
        // 1. Strict RFC 6154 check
        if self.attributes.contains(&FolderAttribute::Trash) {
            return true;
        }

        // 2. Multilingual heuristic fallback on folder name
        let normalized = deunicode(&self.path).to_lowercase();
        let last_segment = normalized.rsplit('/').next().unwrap_or(&normalized).trim();

        matches!(
            last_segment,
            "trash"
                | "corbeille"
                | "deleted items"
                | "elements supprimes"
                | "deleted messages"
                | "messages supprimes"
                | "supprimes"
                | "papierkorb"
                | "papelera"
                | "cestino"
                | "bin"
                | "deleted"
        ) || normalized.contains("[gmail]/corbeille")
            || normalized.contains("[gmail]/trash")
            || normalized.contains("[gmail]/bin")
            || normalized.contains("[gmail]/messages supprimes")
            || self.path.to_lowercase().contains("&aok-l&aok-ments")
            || (self.path.to_lowercase().contains("supprim")
                && (self.path.to_lowercase().contains("gmail")
                    || self.path.to_lowercase().contains("element")
                    || self.path.to_lowercase().contains("message")))
    }

    /// Determines if this folder can be scanned for purge candidates.
    /// Excludes Trash, Sent, Drafts, Junk, All-mail, and NoSelect folders.
    pub fn is_scannable(&self) -> bool {
        if self.attributes.iter().any(|attr| {
            matches!(
                attr,
                FolderAttribute::Trash
                    | FolderAttribute::Sent
                    | FolderAttribute::Drafts
                    | FolderAttribute::Junk
                    | FolderAttribute::All
                    | FolderAttribute::NoSelect
            )
        }) {
            return false;
        }

        if self.is_trash() {
            return false;
        }

        let normalized = deunicode(&self.path).to_lowercase();
        let last_segment = normalized.rsplit('/').next().unwrap_or(&normalized).trim();

        let excluded_names = [
            "sent",
            "sent items",
            "elements envoyes",
            "messages envoyes",
            "drafts",
            "brouillons",
            "junk",
            "spam",
            "pourriel",
            "all mail",
            "tous les messages",
        ];

        !excluded_names.contains(&last_segment)
    }

    /// Determines if this folder represents the Sent / Éléments envoyés folder.
    pub fn is_sent(&self) -> bool {
        if self.attributes.contains(&FolderAttribute::Sent) {
            return true;
        }
        let normalized = deunicode(&self.path).to_lowercase();
        let last_segment = normalized.rsplit('/').next().unwrap_or(&normalized).trim();
        matches!(
            last_segment,
            "sent"
                | "sent messages"
                | "sent items"
                | "elements envoyes"
                | "messages envoyes"
                | "envoyes"
                | "gesendete objekte"
                | "gesendet"
                | "inviati"
                | "enviados"
        ) || normalized.contains("[gmail]/sent")
            || normalized.contains("[gmail]/messages envoyes")
    }
}

/// Identifies the Trash folder from a list of folders.
pub fn find_trash_folder(folders: &[FolderInfo]) -> Option<&FolderInfo> {
    // 1. Look for explicit RFC 6154 \Trash attribute first
    if let Some(trash) = folders
        .iter()
        .find(|f| f.attributes.contains(&FolderAttribute::Trash))
    {
        return Some(trash);
    }

    // 2. Fallback to heuristic
    folders.iter().find(|f| f.is_trash())
}

/// Filters and returns only folders eligible for scanning.
pub fn filter_scannable_folders(folders: &[FolderInfo]) -> Vec<&FolderInfo> {
    folders.iter().filter(|f| f.is_scannable()).collect()
}
