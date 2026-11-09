use psl::Psl;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedSender {
    pub raw_from: String,
    pub display_name: Option<String>,
    pub email_address: String,
    pub full_domain: String,
    pub root_domain: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainExtractError {
    #[error("Empty or invalid From header")]
    InvalidHeader,
    #[error("Could not find any email address in From header")]
    NoAddressFound,
    #[error("Email address has no domain: {0}")]
    MissingDomain(String),
}

/// Extracts sender information and root domain (eTLD+1) from a `From` or `Sender` header.
pub fn extract_sender_domain(from_header: &str) -> Result<ExtractedSender, DomainExtractError> {
    let trimmed = from_header.trim();
    if trimmed.is_empty() {
        return Err(DomainExtractError::InvalidHeader);
    }

    let parsed_addrs = mailparse::addrparse(trimmed).map_err(|_| DomainExtractError::InvalidHeader)?;

    let (display_name, raw_addr) = parsed_addrs
        .iter()
        .find_map(|item| match item {
            mailparse::MailAddr::Single(info) => {
                if info.addr.is_empty() && info.display_name.is_none() {
                    None
                } else {
                    Some((info.display_name.clone(), info.addr.clone()))
                }
            }
            mailparse::MailAddr::Group(group) => {
                group.addrs.first().map(|info| (info.display_name.clone(), info.addr.clone()))
            }
        })
        .ok_or(DomainExtractError::NoAddressFound)?;

    let mut email_address = raw_addr.trim().to_lowercase();
    let display_name = display_name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    // If addr is empty but display_name contains something without @, or if addr has no @
    if email_address.is_empty() {
        if let Some(ref disp) = display_name {
            if disp.contains('@') {
                email_address = disp.to_lowercase();
            } else {
                return Err(DomainExtractError::MissingDomain(disp.clone()));
            }
        } else {
            return Err(DomainExtractError::NoAddressFound);
        }
    }

    let at_idx = email_address
        .rfind('@')
        .ok_or_else(|| DomainExtractError::MissingDomain(email_address.clone()))?;

    let full_domain = email_address[(at_idx + 1)..].trim().to_lowercase();
    if full_domain.is_empty() {
        return Err(DomainExtractError::MissingDomain(email_address));
    }

    let root_domain = extract_root_domain(&full_domain);

    Ok(ExtractedSender {
        raw_from: trimmed.to_string(),
        display_name,
        email_address,
        full_domain,
        root_domain,
    })
}

/// Resolves the registrable domain (eTLD+1) using the Public Suffix List.
pub fn extract_root_domain(full_domain: &str) -> String {
    let lower_domain = full_domain.to_lowercase();
    if let Some(domain) = psl::List.domain(lower_domain.as_bytes()) {
        if let Ok(root_str) = std::str::from_utf8(domain.as_bytes()) {
            return root_str.to_string();
        }
    }
    lower_domain
}
