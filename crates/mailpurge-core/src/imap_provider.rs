use std::sync::Mutex;

use imap_proto::types::{BodyStructure, NameAttribute};

use crate::folder_scanner::{FolderAttribute, FolderInfo};
use crate::imap_sequence::chunk_sequence_set;
use crate::provider_port::{MailProviderError, MailProviderPort, RawEmailEnvelope};

/// Authentication methods supported for IMAP connections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImapAuthMethod {
    Password(String),
    XOAuth2 { access_token: String },
}

/// Configuration required to connect to an IMAP mailbox.
#[derive(Debug, Clone)]
pub struct ImapConfig {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub auth: ImapAuthMethod,
}

/// Authenticator implementing SASL XOAUTH2 for Google Workspace and Microsoft 365.
pub struct XOAuth2Authenticator {
    pub username: String,
    pub access_token: String,
}

impl imap::Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&self, _data: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.username, self.access_token
        )
    }
}

/// Maps RFC 6154 and IMAP-proto `NameAttribute` to internal domain `FolderAttribute`.
#[must_use]
pub fn map_imap_attribute(attr: &NameAttribute) -> FolderAttribute {
    match attr {
        NameAttribute::Trash => FolderAttribute::Trash,
        NameAttribute::Sent => FolderAttribute::Sent,
        NameAttribute::Drafts => FolderAttribute::Drafts,
        NameAttribute::Junk => FolderAttribute::Junk,
        NameAttribute::All => FolderAttribute::All,
        NameAttribute::Archive => FolderAttribute::Archive,
        NameAttribute::NoSelect => FolderAttribute::NoSelect,
        NameAttribute::NoInferiors => FolderAttribute::Other("NoInferiors".to_string()),
        NameAttribute::Marked => FolderAttribute::Other("Marked".to_string()),
        NameAttribute::Unmarked => FolderAttribute::Other("Unmarked".to_string()),
        NameAttribute::Flagged => FolderAttribute::Other("Flagged".to_string()),
        NameAttribute::Extension(ext) => FolderAttribute::Other(ext.to_string()),
        _ => FolderAttribute::Other(format!("{attr:?}")),
    }
}

/// Recursively traverses an IMAP `BodyStructure` to extract all attachment names.
#[must_use]
pub fn extract_attachment_names(bs: &BodyStructure) -> Vec<String> {
    let mut names = Vec::new();
    collect_attachment_names(bs, &mut names);
    names
}

fn collect_attachment_names(bs: &BodyStructure, acc: &mut Vec<String>) {
    match bs {
        BodyStructure::Basic { common, .. } | BodyStructure::Text { common, .. } => {
            let mut found_name = false;

            // 1. Look in Content-Disposition parameters
            if let Some(disposition) = &common.disposition {
                if let Some(params) = &disposition.params {
                    for (k, v) in params {
                        if k.eq_ignore_ascii_case("filename") {
                            acc.push(v.to_string());
                            found_name = true;
                            break;
                        }
                    }
                }
            }

            // 2. Look in Content-Type parameters (name=...)
            if !found_name {
                if let Some(params) = &common.ty.params {
                    for (k, v) in params {
                        if k.eq_ignore_ascii_case("name") || k.eq_ignore_ascii_case("filename") {
                            acc.push(v.to_string());
                            found_name = true;
                            break;
                        }
                    }
                }
            }

            // 3. Fallback for PDF attachments without explicit filename
            if !found_name && common.ty.subtype.eq_ignore_ascii_case("pdf") {
                acc.push("document.pdf".to_string());
            }
        }
        BodyStructure::Message { body, .. } => {
            collect_attachment_names(body, acc);
        }
        BodyStructure::Multipart { bodies, .. } => {
            for child in bodies {
                collect_attachment_names(child, acc);
            }
        }
    }
}

/// Parsed metadata extracted from email headers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedHeaders {
    pub subject: String,
    pub from: String,
    pub message_id: Option<String>,
    pub has_unsubscribe: bool,
    pub list_id: Option<String>,
    pub list_unsubscribe: Option<String>,
    pub list_unsubscribe_post: Option<String>,
}

/// Parses raw header bytes into `ParsedHeaders` using RFC 2047 decoding.
#[must_use]
pub fn parse_raw_headers(header_bytes: &[u8]) -> ParsedHeaders {
    let mut result = ParsedHeaders::default();

    if let Ok((headers, _)) = mailparse::parse_headers(header_bytes) {
        for header in headers {
            let key = header.get_key();
            let value = header.get_value();

            if key.eq_ignore_ascii_case("subject") {
                result.subject = value;
            } else if key.eq_ignore_ascii_case("from") {
                result.from = value;
            } else if key.eq_ignore_ascii_case("sender") {
                if result.from.trim().is_empty() {
                    result.from = value;
                }
            } else if key.eq_ignore_ascii_case("reply-to") {
                if result.from.trim().is_empty() {
                    result.from = value;
                }
            } else if key.eq_ignore_ascii_case("message-id") {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    result.message_id = Some(trimmed.to_string());
                }
            } else if key.eq_ignore_ascii_case("list-id") {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    result.list_id = Some(trimmed.to_string());
                }
            } else if key.eq_ignore_ascii_case("list-unsubscribe") {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    result.has_unsubscribe = true;
                    result.list_unsubscribe = Some(trimmed.to_string());
                }
            } else if key.eq_ignore_ascii_case("list-unsubscribe-post") {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    result.has_unsubscribe = true;
                    result.list_unsubscribe_post = Some(trimmed.to_string());
                }
            }
        }
    }

    result
}

/// Converts an IMAP `Fetch` response item into a `RawEmailEnvelope`.
#[must_use]
pub fn parse_fetch_to_envelope(fetch: &imap::types::Fetch) -> Option<RawEmailEnvelope> {
    let uid = fetch.uid?;
    let size_bytes = u64::from(fetch.size.unwrap_or(0));
    let is_flagged = fetch
        .flags()
        .iter()
        .any(|f| matches!(f, imap::types::Flag::Flagged));
    let date_timestamp = fetch.internal_date().map_or(0, |dt| dt.timestamp());

    let parsed_headers = match fetch.header() {
        Some(bytes) => parse_raw_headers(bytes),
        None => ParsedHeaders::default(),
    };

    let attachment_names = match fetch.bodystructure() {
        Some(bs) => extract_attachment_names(bs),
        None => Vec::new(),
    };

    Some(RawEmailEnvelope {
        uid,
        message_id: parsed_headers.message_id,
        subject: parsed_headers.subject,
        from: parsed_headers.from,
        date_timestamp,
        size_bytes,
        is_flagged,
        has_unsubscribe: parsed_headers.has_unsubscribe,
        attachment_names,
        list_id: parsed_headers.list_id,
        list_unsubscribe: parsed_headers.list_unsubscribe,
        list_unsubscribe_post: parsed_headers.list_unsubscribe_post,
    })
}

fn map_imap_error(err: imap::Error) -> MailProviderError {
    match err {
        imap::Error::Io(e) => MailProviderError::ConnectionError(e.to_string()),
        imap::Error::RustlsHandshake(e) => MailProviderError::ConnectionError(e.to_string()),
        imap::Error::Bad(e) => MailProviderError::ProtocolError(e.to_string()),
        imap::Error::No(e) => MailProviderError::OperationFailed(e.to_string()),
        imap::Error::ConnectionLost => {
            MailProviderError::ConnectionError("Connexion IMAP interrompue".to_string())
        }
        imap::Error::Parse(e) => {
            MailProviderError::ProtocolError(format!("Erreur de parsing: {e}"))
        }
        imap::Error::Validate(e) => {
            MailProviderError::ProtocolError(format!("Validation invalide: {e}"))
        }
        imap::Error::Append => {
            MailProviderError::OperationFailed("Échec de la commande APPEND".to_string())
        }
        other => MailProviderError::OperationFailed(other.to_string()),
    }
}

/// Quotes an IMAP mailbox name according to RFC 3501 grammar if not already quoted.
#[must_use]
pub fn quote_mailbox_name(name: &str) -> String {
    if name.starts_with('"') && name.ends_with('"') && name.len() >= 2 {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn strip_html_tags(html: &str) -> String {
    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let len = chars.len();

    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if chars[i] == '<' {
            let rem_lower: String = lower_chars[i..].iter().take(10).collect();

            // HTML comments <!-- ... -->
            if rem_lower.starts_with("<!--") {
                i += 4;
                while i + 2 < len {
                    if chars[i] == '-' && chars[i + 1] == '-' && chars[i + 2] == '>' {
                        i += 3;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            // Skip tags and their entire body: <style, <script, <head
            let skip_end_tag = if rem_lower.starts_with("<style") {
                Some("</style>")
            } else if rem_lower.starts_with("<script") {
                Some("</script>")
            } else if rem_lower.starts_with("<head") {
                Some("</head>")
            } else {
                None
            };

            if let Some(end_tag) = skip_end_tag {
                let end_chars: Vec<char> = end_tag.chars().collect();
                let end_len = end_chars.len();
                i += 1;
                while i + end_len <= len {
                    if lower_chars[i..i + end_len] == end_chars[..] {
                        i += end_len;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            // Block elements insert newlines
            let block_tag = rem_lower.starts_with("<br")
                || rem_lower.starts_with("</p")
                || rem_lower.starts_with("</div")
                || rem_lower.starts_with("</tr")
                || rem_lower.starts_with("</li")
                || rem_lower.starts_with("</h1")
                || rem_lower.starts_with("</h2")
                || rem_lower.starts_with("</h3")
                || rem_lower.starts_with("<p")
                || rem_lower.starts_with("<tr")
                || rem_lower.starts_with("<li");

            if block_tag && !out.ends_with('\n') {
                out.push('\n');
            }

            // Skip tag definition until '>'
            while i < len && chars[i] != '>' {
                i += 1;
            }
            if i < len && chars[i] == '>' {
                i += 1;
            }
        } else if chars[i] == '&' {
            let rem: String = chars[i..].iter().take(12).collect();
            if let Some(semi_pos) = rem.find(';') {
                let entity = &rem[..=semi_pos];
                match entity.to_lowercase().as_str() {
                    "&nbsp;" => out.push(' '),
                    "&amp;" => out.push('&'),
                    "&lt;" => out.push('<'),
                    "&gt;" => out.push('>'),
                    "&quot;" => out.push('"'),
                    "&#39;" | "&apos;" => out.push('\''),
                    "&euro;" => out.push('€'),
                    _ => {
                        if entity.starts_with("&#") && !entity.starts_with("&#x") {
                            let num_str = &entity[2..entity.len() - 1];
                            if let Ok(code) = num_str.parse::<u32>() {
                                if let Some(ch) = char::from_u32(code) {
                                    out.push(ch);
                                }
                            }
                        } else if entity.to_lowercase().starts_with("&#x") {
                            let hex_str = &entity[3..entity.len() - 1];
                            if let Ok(code) = u32::from_str_radix(hex_str, 16) {
                                if let Some(ch) = char::from_u32(code) {
                                    out.push(ch);
                                }
                            }
                        }
                    }
                }
                i += semi_pos + 1;
            } else {
                out.push('&');
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }

    let mut lines = Vec::new();
    let mut prev_empty = false;
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_empty && !lines.is_empty() {
                lines.push("");
                prev_empty = true;
            }
        } else {
            lines.push(trimmed);
            prev_empty = false;
        }
    }
    lines.join("\n")
}

fn truncate_preview(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

fn collect_leaf_parts<'a>(
    part: &'a mailparse::ParsedMail<'a>,
    acc: &mut Vec<&'a mailparse::ParsedMail<'a>>,
) {
    if part.subparts.is_empty() {
        acc.push(part);
    } else {
        for subpart in &part.subparts {
            collect_leaf_parts(subpart, acc);
        }
    }
}

/// Extracts a clean, human-readable body snippet from raw email bytes (handling MIME multipart, Base64/Quoted-Printable, and HTML).
#[must_use]
pub fn extract_body_preview(raw_bytes: &[u8]) -> String {
    if let Ok(mail) = mailparse::parse_mail(raw_bytes) {
        let mut leaf_parts = Vec::new();
        collect_leaf_parts(&mail, &mut leaf_parts);

        // 1. Prefer text/plain across all leaf parts
        for part in &leaf_parts {
            if part.ctype.mimetype.eq_ignore_ascii_case("text/plain") {
                if let Ok(body) = part.get_body() {
                    let trimmed = body.trim();
                    if !trimmed.is_empty() {
                        return truncate_preview(trimmed, 1500);
                    }
                }
            }
        }

        // 2. Try text/html across all leaf parts with clean tag & CSS stripping
        for part in &leaf_parts {
            if part.ctype.mimetype.eq_ignore_ascii_case("text/html") {
                if let Ok(body) = part.get_body() {
                    let stripped = strip_html_tags(&body);
                    let trimmed = stripped.trim();
                    if !trimmed.is_empty() {
                        return truncate_preview(trimmed, 1500);
                    }
                }
            }
        }

        // 3. Fallback on top-level body
        if let Ok(body) = mail.get_body() {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                if mail.ctype.mimetype.to_lowercase().contains("html") {
                    return truncate_preview(&strip_html_tags(trimmed), 1500);
                }
                return truncate_preview(trimmed, 1500);
            }
        }
    }

    let s = String::from_utf8_lossy(raw_bytes);
    truncate_preview(&strip_html_tags(s.trim()), 1500)
}

/// Production IMAP mail provider adapter communicating over TLS port 993.
pub struct ImapProvider {
    config: ImapConfig,
    session: Mutex<Option<imap::Session<imap::Connection>>>,
}

impl ImapProvider {
    /// Creates a new `ImapProvider` from the given configuration.
    #[must_use]
    pub fn new(config: ImapConfig) -> Self {
        Self {
            config,
            session: Mutex::new(None),
        }
    }

    /// Connects a new IMAP session over TLS with a 12-second timeout.
    pub fn connect_session(&self) -> Result<imap::Session<imap::Connection>, MailProviderError> {
        let server = self.config.server.clone();
        let port = self.config.port;
        let auth = self.config.auth.clone();
        let username = self.config.username.clone();

        eprintln!("[IMAP] Tentative de connexion TLS vers {server}:{port} (login: {username})...");

        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let res = (|| -> Result<imap::Session<imap::Connection>, MailProviderError> {
                let client = imap::ClientBuilder::new(&server, port)
                    .connect()
                    .map_err(map_imap_error)?;

                match &auth {
                    ImapAuthMethod::Password(password) => client
                        .login(&username, password)
                        .map_err(|(err, _)| map_imap_error(err)),
                    ImapAuthMethod::XOAuth2 { access_token } => {
                        let authenticator = XOAuth2Authenticator {
                            username,
                            access_token: access_token.clone(),
                        };
                        client
                            .authenticate("XOAUTH2", &authenticator)
                            .map_err(|(err, _)| map_imap_error(err))
                    }
                }
            })();
            let _ = tx.send(res);
        });

        match rx.recv_timeout(std::time::Duration::from_secs(12)) {
            Ok(Ok(session)) => {
                eprintln!(
                    "[IMAP] Connexion et authentification réussies vers {}:{}",
                    self.config.server, self.config.port
                );
                Ok(session)
            }
            Ok(Err(e)) => {
                eprintln!(
                    "[IMAP] Échec de connexion/authentification vers {}:{} - {e}",
                    self.config.server, self.config.port
                );
                Err(e)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let msg = format!(
                    "Délai d'attente dépassé (timeout 12s) vers {}:{} — Le serveur ne répond pas (port bloqué ou restriction réseau fournisseur)",
                    self.config.server, self.config.port
                );
                eprintln!("[IMAP] {msg}");
                Err(MailProviderError::ConnectionError(msg))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err(MailProviderError::ConnectionError(
                    "Le thread de connexion IMAP s'est arrêté inopinément".to_string(),
                ))
            }
        }
    }

    fn execute_with_session<R>(
        &self,
        action: impl FnOnce(&mut imap::Session<imap::Connection>) -> Result<R, MailProviderError>,
    ) -> Result<R, MailProviderError> {
        let mut session_guard = self.session.lock().map_err(|_| {
            MailProviderError::OperationFailed("Verrouillage de session échoué".to_string())
        })?;

        if session_guard.is_none() {
            *session_guard = Some(self.connect_session()?);
        }

        let session = match session_guard.as_mut() {
            Some(s) => s,
            None => {
                return Err(MailProviderError::ConnectionError(
                    "Session IMAP non initialisée".to_string(),
                ))
            }
        };

        match action(session) {
            Ok(val) => Ok(val),
            Err(e) => {
                // Invalidate session on error so reconnection can happen
                *session_guard = None;
                Err(e)
            }
        }
    }
}

impl MailProviderPort for ImapProvider {
    fn list_folders(&self) -> Result<Vec<FolderInfo>, MailProviderError> {
        self.execute_with_session(|session| {
            let names = session.list(Some(""), Some("*")).map_err(map_imap_error)?;

            let mut folders = Vec::new();
            for name in names.iter() {
                let path = name.name().to_string();
                if path.is_empty() {
                    continue;
                }
                let delimiter = name.delimiter().and_then(|d| d.chars().next());
                let attributes = name.attributes().iter().map(map_imap_attribute).collect();

                folders.push(FolderInfo {
                    path,
                    delimiter,
                    attributes,
                });
            }
            Ok(folders)
        })
    }

    fn get_folder_message_count(&self, folder_path: &str) -> Result<u32, MailProviderError> {
        self.execute_with_session(|session| {
            let mailbox = session.select(folder_path).map_err(map_imap_error)?;
            Ok(mailbox.exists)
        })
    }

    fn fetch_envelopes(
        &self,
        folder_path: &str,
    ) -> Result<Vec<RawEmailEnvelope>, MailProviderError> {
        self.execute_with_session(|session| {
            let mailbox = session.select(folder_path).map_err(map_imap_error)?;
            if mailbox.exists == 0 {
                return Ok(Vec::new());
            }

            let fetches = session
                .uid_fetch(
                    "1:*",
                    "(UID RFC822.SIZE FLAGS INTERNALDATE BODYSTRUCTURE BODY.PEEK[HEADER.FIELDS (Subject From Sender Reply-To Date Message-ID List-ID List-Unsubscribe List-Unsubscribe-Post)])",
                )
                .map_err(map_imap_error)?;

            let mut envelopes = Vec::with_capacity(fetches.len());
            for fetch in fetches.iter() {
                if let Some(env) = parse_fetch_to_envelope(fetch) {
                    envelopes.push(env);
                }
            }

            Ok(envelopes)
        })
    }

    fn fetch_envelopes_range(
        &self,
        folder_path: &str,
        start_seq: u32,
        end_seq: u32,
    ) -> Result<Vec<RawEmailEnvelope>, MailProviderError> {
        if start_seq == 0 || start_seq > end_seq {
            return Ok(Vec::new());
        }

        self.execute_with_session(|session| {
            let mailbox = session.select(folder_path).map_err(map_imap_error)?;
            if mailbox.exists == 0 || start_seq > mailbox.exists {
                return Ok(Vec::new());
            }

            let bounded_end = end_seq.min(mailbox.exists);
            let seq_range = format!("{start_seq}:{bounded_end}");

            let fetches = session
                .fetch(
                    &seq_range,
                    "(UID RFC822.SIZE FLAGS INTERNALDATE BODYSTRUCTURE BODY.PEEK[HEADER.FIELDS (Subject From Sender Reply-To Date Message-ID List-ID List-Unsubscribe List-Unsubscribe-Post)])",
                )
                .map_err(map_imap_error)?;

            let mut envelopes = Vec::with_capacity(fetches.len());
            for fetch in fetches.iter() {
                if let Some(env) = parse_fetch_to_envelope(fetch) {
                    envelopes.push(env);
                }
            }

            Ok(envelopes)
        })
    }

    fn move_to_trash(
        &mut self,
        folder_path: &str,
        uids: &[u32],
        trash_folder_path: &str,
    ) -> Result<usize, MailProviderError> {
        if uids.is_empty() {
            return Ok(0);
        }

        self.execute_with_session(|session| {
            session.select(folder_path).map_err(map_imap_error)?;

            let (has_move, has_uidplus) = match session.capabilities() {
                Ok(caps) => (caps.has_str("MOVE"), caps.has_str("UIDPLUS")),
                Err(_) => (false, false),
            };

            let quoted_trash = quote_mailbox_name(trash_folder_path);
            let mut moved_count = 0;

            for chunk_uids in uids.chunks(500) {
                let seq = chunk_sequence_set(chunk_uids, 500)
                    .into_iter()
                    .next()
                    .unwrap_or_default();

                if seq.is_empty() {
                    continue;
                }

                let mut moved = false;
                if has_move && session.uid_mv(&seq, trash_folder_path).is_ok() {
                    moved = true;
                }

                if !moved {
                    // Fallback RFC 3501: UID COPY + UID STORE +FLAGS (\Deleted) + EXPUNGE
                    session
                        .uid_copy(&seq, &quoted_trash)
                        .map_err(map_imap_error)?;
                    session
                        .uid_store(&seq, "+FLAGS (\\Deleted)")
                        .map_err(map_imap_error)?;

                    if has_uidplus {
                        let _ = session.uid_expunge(&seq).or_else(|_| session.expunge());
                    } else {
                        let _ = session.expunge();
                    }
                }

                moved_count += chunk_uids.len();
            }

            Ok(moved_count)
        })
    }

    fn empty_trash(&mut self, trash_folder_path: &str) -> Result<usize, MailProviderError> {
        self.execute_with_session(|session| {
            let mailbox = session.select(trash_folder_path).map_err(map_imap_error)?;

            if mailbox.exists == 0 {
                return Ok(0);
            }

            session
                .store("1:*", "+FLAGS (\\Deleted)")
                .map_err(map_imap_error)?;
            session.expunge().map_err(map_imap_error)?;

            Ok(mailbox.exists as usize)
        })
    }

    fn fetch_body_snippet(
        &mut self,
        folder_path: &str,
        uid: u32,
    ) -> Result<String, MailProviderError> {
        self.execute_with_session(|session| {
            session.select(folder_path).map_err(map_imap_error)?;

            // 1. Fetch email up to 64KB via BODY.PEEK[] without marking as read
            if let Ok(fetches) = session.uid_fetch(uid.to_string(), "(BODY.PEEK[]<0.65536>)") {
                if let Some(fetch) = fetches.iter().next() {
                    if let Some(bytes) = fetch.body() {
                        let preview = extract_body_preview(bytes);
                        if !preview.trim().is_empty() {
                            return Ok(preview);
                        }
                    }
                }
            }

            // 2. Fallback to full BODY.PEEK[]
            if let Ok(fetches_full) = session.uid_fetch(uid.to_string(), "(BODY.PEEK[])") {
                if let Some(fetch) = fetches_full.iter().next() {
                    if let Some(bytes) = fetch.body() {
                        let preview = extract_body_preview(bytes);
                        if !preview.trim().is_empty() {
                            return Ok(preview);
                        }
                    }
                }
            }

            Ok(String::new())
        })
    }
}
