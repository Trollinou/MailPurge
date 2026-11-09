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

fn strip_ascii_prefix_ci<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    let head = input.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        input.get(prefix.len()..)
    } else {
        None
    }
}

fn strip_header_prefix(input: &str) -> &str {
    let trimmed = input.trim();
    for prefix in &["from:", "sender:", "reply-to:"] {
        if let Some(rest) = strip_ascii_prefix_ci(trimmed, prefix) {
            return rest.trim();
        }
    }
    trimmed
}

fn sanitize_candidate_addr(input: &str) -> String {
    let mut s = input.trim();
    if let Some(rest) = strip_ascii_prefix_ci(s, "mailto:") {
        s = rest.trim();
    }
    s.trim_matches(|c: char| {
        c == '<'
            || c == '>'
            || c == '"'
            || c == '\''
            || c == '['
            || c == ']'
            || c == '('
            || c == ')'
            || c.is_whitespace()
    })
    .trim_end_matches(|c: char| c == '.' || c == ',' || c == ';' || c == ':')
    .to_string()
}

fn try_build_extracted_sender(
    raw_from: &str,
    display_name: Option<String>,
    candidate_addr: &str,
) -> Option<ExtractedSender> {
    let clean_addr = sanitize_candidate_addr(candidate_addr);
    let at_idx = clean_addr.rfind('@')?;

    let user_part = clean_addr.get(..at_idx)?;
    let domain_part = clean_addr.get((at_idx + 1)..)?;

    if user_part.is_empty()
        || user_part.contains(char::is_whitespace)
        || user_part.contains('<')
        || user_part.contains('>')
    {
        return None;
    }

    let clean_domain = domain_part
        .trim_matches(|c: char| {
            c == '>'
                || c == ')'
                || c == ']'
                || c == '"'
                || c == '\''
                || c == ','
                || c == ';'
                || c == ':'
        })
        .trim_end_matches('.')
        .trim()
        .to_lowercase();

    if clean_domain.is_empty() || clean_domain.contains(char::is_whitespace) {
        return None;
    }

    if !clean_domain.chars().any(|c| c.is_alphanumeric()) {
        return None;
    }

    let root_domain = extract_root_domain(&clean_domain);
    let email_address = format!("{user_part}@{clean_domain}").to_lowercase();

    let display_name = display_name
        .map(|s| s.trim().trim_matches('"').trim().to_string())
        .filter(|s| !s.is_empty());

    Some(ExtractedSender {
        raw_from: raw_from.to_string(),
        display_name,
        email_address,
        full_domain: clean_domain,
        root_domain,
    })
}

/// Extracts sender information and root domain (eTLD+1) from a `From` or `Sender` header.
/// Highly resilient against unquoted commas, malformed brackets, and complex display names.
pub fn extract_sender_domain(from_header: &str) -> Result<ExtractedSender, DomainExtractError> {
    let cleaned = strip_header_prefix(from_header);
    if cleaned.is_empty() || !cleaned.contains('@') {
        return Err(DomainExtractError::InvalidHeader);
    }

    // Strategy 1: mailparse::addrparse
    if let Ok(parsed_addrs) = mailparse::addrparse(cleaned) {
        for item in parsed_addrs.iter() {
            match item {
                mailparse::MailAddr::Single(info) => {
                    if let Some(res) =
                        try_build_extracted_sender(cleaned, info.display_name.clone(), &info.addr)
                    {
                        return Ok(res);
                    }
                    if let Some(ref disp) = info.display_name {
                        if let Some(res) = try_build_extracted_sender(cleaned, None, disp) {
                            return Ok(res);
                        }
                    }
                }
                mailparse::MailAddr::Group(group) => {
                    for info in &group.addrs {
                        if let Some(res) = try_build_extracted_sender(
                            cleaned,
                            info.display_name.clone(),
                            &info.addr,
                        ) {
                            return Ok(res);
                        }
                    }
                }
            }
        }
    }

    // Strategy 2: Angle bracket extraction `<addr@domain.com>`
    if let Some(open_idx) = cleaned.rfind('<') {
        let inside = if let Some(after_open) = cleaned.get(open_idx + 1..) {
            if let Some(close_idx) = after_open.find('>') {
                after_open.get(..close_idx).unwrap_or(after_open)
            } else {
                after_open
            }
        } else {
            ""
        };
        let disp = cleaned.get(..open_idx).unwrap_or("").trim();
        let display_name = if disp.is_empty() {
            None
        } else {
            Some(disp.to_string())
        };
        if let Some(res) = try_build_extracted_sender(cleaned, display_name, inside) {
            return Ok(res);
        }
    }

    // Strategy 3: Parenthesized display name `addr@domain.com (Display Name)`
    if let Some(paren_idx) = cleaned.find('(') {
        let before_paren = cleaned.get(..paren_idx).unwrap_or("");
        let comment = cleaned
            .get(paren_idx + 1..)
            .unwrap_or("")
            .trim_end_matches(')')
            .trim();
        let display_name = if comment.is_empty() {
            None
        } else {
            Some(comment.to_string())
        };
        if let Some(res) = try_build_extracted_sender(cleaned, display_name, before_paren) {
            return Ok(res);
        }
    }

    // Strategy 4: Square brackets `Display Name [addr@domain.com]`
    if let Some(open_idx) = cleaned.rfind('[') {
        if let Some(after_open) = cleaned.get(open_idx + 1..) {
            if let Some(close_idx) = after_open.find(']') {
                let inside = after_open.get(..close_idx).unwrap_or(after_open);
                let disp = cleaned.get(..open_idx).unwrap_or("").trim();
                let display_name = if disp.is_empty() {
                    None
                } else {
                    Some(disp.to_string())
                };
                if let Some(res) = try_build_extracted_sender(cleaned, display_name, inside) {
                    return Ok(res);
                }
            }
        }
    }

    // Strategy 5: Token scan across whitespace and comma/semicolon/quotes
    for token in cleaned.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '"') {
        let token = token.trim();
        if token.contains('@') {
            if let Some(res) = try_build_extracted_sender(cleaned, None, token) {
                return Ok(res);
            }
        }
    }

    Err(DomainExtractError::NoAddressFound)
}

/// Resolves the registrable domain (eTLD+1) using the Public Suffix List.
#[must_use]
pub fn extract_root_domain(full_domain: &str) -> String {
    let lower_domain = full_domain.to_lowercase();
    let clean_domain = lower_domain.trim_matches(|c: char| {
        c == '.'
            || c == '>'
            || c == ')'
            || c == ']'
            || c == '"'
            || c == '\''
            || c == ','
            || c == ';'
    });
    if clean_domain.is_empty() {
        return "unknown".to_string();
    }
    if let Some(domain) = psl::List.domain(clean_domain.as_bytes()) {
        if let Ok(root_str) = std::str::from_utf8(domain.as_bytes()) {
            return root_str.to_string();
        }
    }
    clean_domain.to_string()
}

/// Extracts a root domain from an RFC 2919 `List-ID` header value.
/// E.g. `"Deals <promotions.fnac.com>"` -> `"fnac.com"`.
#[must_use]
pub fn extract_domain_from_list_id(list_id: &str) -> Option<String> {
    let trimmed = list_id.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Extract identifier between < and > if present
    let candidate = if let Some(open) = trimmed.rfind('<') {
        if let Some(after_open) = trimmed.get(open + 1..) {
            if let Some(close) = after_open.find('>') {
                after_open.get(..close).unwrap_or(after_open)
            } else {
                after_open
            }
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    let clean = candidate.trim().trim_matches(|c: char| {
        c == '"' || c == '\'' || c == '[' || c == ']' || c == '<' || c == '>' || c.is_whitespace()
    });

    // If it contains a dot and letters
    if clean.contains('.') && clean.chars().any(|c| c.is_alphabetic()) {
        // Strip any user/sub-identifier before '@' if it looks like an email
        let domain_part = if let Some(at) = clean.rfind('@') {
            clean.get(at + 1..).unwrap_or(clean)
        } else {
            clean
        };
        let root = extract_root_domain(domain_part);
        if root != "unknown" && root.contains('.') {
            return Some(root);
        }
    }

    None
}
