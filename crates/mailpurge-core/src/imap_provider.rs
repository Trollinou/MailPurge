use async_imap::imap_proto::types::{BodyStructure, NameAttribute};
use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::Mutex;

use crate::folder_scanner::{FolderAttribute, FolderInfo};
use crate::imap_sequence::chunk_sequence_set;
use crate::provider_port::{MailProviderError, MailProviderPort, RawEmailEnvelope};

/// Authentication methods supported for IMAP connections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImapAuthMethod {
    Password(String),
    XOAuth2 { access_token: String },
}

/// Installs the `ring` crypto provider as the default process-level provider for Rustls.
///
/// This avoids runtime panics when multiple providers (`ring` and `aws-lc-rs`)
/// coexist in the dependency tree.
pub fn install_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
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

impl async_imap::Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&mut self, _data: &[u8]) -> Self::Response {
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
    pub return_path: Option<String>,
    pub authentication_results: Option<String>,
    pub spam_verdict_header: Option<String>,
    pub to_header: Option<String>,
}

/// Parses raw header bytes into `ParsedHeaders` using RFC 2047 decoding.
#[must_use]
pub fn parse_raw_headers(header_bytes: &[u8]) -> ParsedHeaders {
    let mut result = ParsedHeaders::default();

    if let Some(msg) = mail_parser::MessageParser::new().parse_headers(header_bytes) {
        if let Some(subject) = msg.subject() {
            result.subject = subject.to_string();
        }

        if let Some(from) = msg.from() {
            if let Some(first) = from.first() {
                match (&first.name, &first.address) {
                    (Some(name), Some(addr)) => {
                        result.from = format!("{name} <{addr}>");
                    }
                    (None, Some(addr)) => {
                        result.from = addr.to_string();
                    }
                    (Some(name), None) => {
                        result.from = name.to_string();
                    }
                    (None, None) => {}
                }
            }
        } else if let Some(sender) = msg.sender().and_then(|s| s.first()) {
            match (&sender.name, &sender.address) {
                (Some(name), Some(addr)) => {
                    result.from = format!("{name} <{addr}>");
                }
                (None, Some(addr)) => {
                    result.from = addr.to_string();
                }
                (Some(name), None) => {
                    result.from = name.to_string();
                }
                (None, None) => {}
            }
        } else if let Some(reply_to) = msg.reply_to().and_then(|r| r.first()) {
            match (&reply_to.name, &reply_to.address) {
                (Some(name), Some(addr)) => {
                    result.from = format!("{name} <{addr}>");
                }
                (None, Some(addr)) => {
                    result.from = addr.to_string();
                }
                (Some(name), None) => {
                    result.from = name.to_string();
                }
                (None, None) => {}
            }
        }

        if let Some(msg_id) = msg.message_id() {
            let trimmed = msg_id.trim();
            if !trimmed.is_empty() {
                if trimmed.starts_with('<') && trimmed.ends_with('>') {
                    result.message_id = Some(trimmed.to_string());
                } else {
                    result.message_id = Some(format!("<{trimmed}>"));
                }
            }
        } else if let Some(raw_id) = msg
            .header_raw("Message-ID")
            .or_else(|| msg.header_raw("Message-Id"))
        {
            let trimmed = raw_id.trim();
            if !trimmed.is_empty() {
                result.message_id = Some(trimmed.to_string());
            }
        }

        let list_id = msg
            .header_raw("List-ID")
            .or_else(|| msg.header_raw("List-Id"))
            .or_else(|| msg.list_id().as_text());
        if let Some(lid) = list_id {
            let trimmed = lid.trim();
            if !trimmed.is_empty() {
                result.list_id = Some(trimmed.to_string());
            }
        }

        let unsub = msg
            .header_raw("List-Unsubscribe")
            .or_else(|| msg.header_raw("list-unsubscribe"))
            .or_else(|| msg.list_unsubscribe().as_text());
        if let Some(u) = unsub {
            let trimmed = u.trim();
            if !trimmed.is_empty() {
                result.has_unsubscribe = true;
                result.list_unsubscribe = Some(trimmed.to_string());
            }
        }

        let unsub_post = msg
            .header_raw("List-Unsubscribe-Post")
            .or_else(|| msg.header_raw("list-unsubscribe-post"));
        if let Some(up) = unsub_post {
            let trimmed = up.trim();
            if !trimmed.is_empty() {
                result.has_unsubscribe = true;
                result.list_unsubscribe_post = Some(trimmed.to_string());
            }
        }

        // Return-Path (Bounce address)
        if let Some(rp) = msg
            .header_raw("Return-Path")
            .or_else(|| msg.header_raw("return-path"))
        {
            let trimmed = rp.trim();
            if !trimmed.is_empty() {
                result.return_path = Some(trimmed.to_string());
            }
        }

        // To recipients
        if let Some(to) = msg.to() {
            let addrs: Vec<String> = to
                .iter()
                .filter_map(|addr| addr.address.as_ref().map(|s| s.to_string()))
                .collect();
            if !addrs.is_empty() {
                result.to_header = Some(addrs.join(", "));
            }
        } else if let Some(to_raw) = msg.header_raw("To").or_else(|| msg.header_raw("to")) {
            let trimmed = to_raw.trim();
            if !trimmed.is_empty() {
                result.to_header = Some(trimmed.to_string());
            }
        }

        // Authentication-Results
        if let Some(ar) = msg
            .header_raw("Authentication-Results")
            .or_else(|| msg.header_raw("authentication-results"))
        {
            let trimmed = ar.trim();
            if !trimmed.is_empty() {
                result.authentication_results = Some(trimmed.to_string());
            }
        }

        // Spam verdict headers (X-Spam-Status, X-Spam-Flag, X-Forefront-Antispam-Report, X-Rspamd-Result)
        let mut spam_parts = Vec::new();
        if let Some(val) = msg.header_raw("X-Spam-Status") {
            spam_parts.push(format!("X-Spam-Status: {}", val.trim()));
        }
        if let Some(val) = msg.header_raw("X-Spam-Flag") {
            spam_parts.push(format!("X-Spam-Flag: {}", val.trim()));
        }
        if let Some(val) = msg.header_raw("X-Forefront-Antispam-Report") {
            spam_parts.push(format!("X-Forefront-Antispam-Report: {}", val.trim()));
        }
        if let Some(val) = msg.header_raw("X-Rspamd-Result") {
            spam_parts.push(format!("X-Rspamd-Result: {}", val.trim()));
        }
        if !spam_parts.is_empty() {
            result.spam_verdict_header = Some(spam_parts.join("; "));
        }
    }

    result
}

/// Converts an IMAP `Fetch` response item into a `RawEmailEnvelope`.
#[must_use]
pub fn parse_fetch_to_envelope(fetch: &async_imap::types::Fetch) -> Option<RawEmailEnvelope> {
    let uid = fetch.uid?;
    let size_bytes = u64::from(fetch.size.unwrap_or(0));
    let is_flagged = fetch
        .flags()
        .any(|f| matches!(f, async_imap::types::Flag::Flagged));
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
        return_path: parsed_headers.return_path,
        authentication_results: parsed_headers.authentication_results,
        spam_verdict_header: parsed_headers.spam_verdict_header,
        to_header: parsed_headers.to_header,
    })
}

fn map_async_imap_error(err: async_imap::error::Error) -> MailProviderError {
    match err {
        async_imap::error::Error::Io(e) => MailProviderError::ConnectionError(e.to_string()),
        async_imap::error::Error::Bad(e) => MailProviderError::ProtocolError(e),
        async_imap::error::Error::No(e) => MailProviderError::OperationFailed(e),
        async_imap::error::Error::ConnectionLost => {
            MailProviderError::ConnectionError("Connexion IMAP interrompue".to_string())
        }
        async_imap::error::Error::Parse(e) => {
            MailProviderError::ProtocolError(format!("Erreur de parsing: {e}"))
        }
        async_imap::error::Error::Validate(e) => {
            MailProviderError::ProtocolError(format!("Validation invalide: {e}"))
        }
        async_imap::error::Error::Append => {
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

            // Skip comments <!-- ... -->
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
                        } else if entity.starts_with("&#x") || entity.starts_with("&#X") {
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

/// Extracts a clean, human-readable body snippet from raw email bytes (handling MIME multipart, Base64/Quoted-Printable, and HTML).
#[must_use]
pub fn extract_body_preview(raw_bytes: &[u8]) -> String {
    if let Some(msg) = mail_parser::MessageParser::new().parse(raw_bytes) {
        // 1. Prefer explicit text/plain part if present
        for part in &msg.parts {
            if let mail_parser::PartType::Text(text) = &part.body {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return truncate_preview(trimmed, 1500);
                }
            }
        }

        // 2. Try explicit text/html part with clean tag & CSS & script stripping
        for part in &msg.parts {
            if let mail_parser::PartType::Html(html) = &part.body {
                let stripped = strip_html_tags(html);
                let trimmed = stripped.trim();
                if !trimmed.is_empty() {
                    return truncate_preview(trimmed, 1500);
                }
            }
        }
    }

    let s = String::from_utf8_lossy(raw_bytes);
    truncate_preview(&strip_html_tags(s.trim()), 1500)
}

pub type ImapSessionStream = tokio_rustls::client::TlsStream<tokio::net::TcpStream>;
pub type ImapSession = async_imap::Session<ImapSessionStream>;

/// Production IMAP mail provider adapter communicating over TLS port 993 asynchronously.
pub struct ImapProvider {
    config: ImapConfig,
    session: Mutex<Option<ImapSession>>,
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
    pub async fn connect_session(&self) -> Result<ImapSession, MailProviderError> {
        let server = self.config.server.clone();
        let port = self.config.port;
        let auth = self.config.auth.clone();
        let username = self.config.username.clone();

        tracing::info!(
            server = %server,
            port,
            username = %username,
            "Tentative de connexion TLS asynchrone IMAP..."
        );

        let connect_fut = async {
            let mut root_store = tokio_rustls::rustls::RootCertStore::empty();
            let native = rustls_native_certs::load_native_certs();
            for cert in native.certs {
                let _ = root_store.add(cert);
            }
            if root_store.is_empty() {
                root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            }

            let tls_config = tokio_rustls::rustls::ClientConfig::builder_with_provider(
                std::sync::Arc::new(tokio_rustls::rustls::crypto::ring::default_provider()),
            )
            .with_safe_default_protocol_versions()
            .map_err(|e| MailProviderError::ConnectionError(format!("Erreur versions TLS: {e}")))?
            .with_root_certificates(root_store)
            .with_no_client_auth();
            let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(tls_config));

            let server_name = rustls_pki_types::ServerName::try_from(server.as_str())
                .map_err(|e| {
                    MailProviderError::ConnectionError(format!("Nom de serveur TLS invalide: {e}"))
                })?
                .to_owned();

            let tcp = tokio::net::TcpStream::connect((server.as_str(), port))
                .await
                .map_err(|e| {
                    MailProviderError::ConnectionError(format!("Échec connexion TCP: {e}"))
                })?;

            let tls_stream = connector.connect(server_name, tcp).await.map_err(|e| {
                MailProviderError::ConnectionError(format!("Échec négociation TLS: {e}"))
            })?;

            let mut client = async_imap::Client::new(tls_stream);
            let _greeting = client.read_response().await;

            let session = match &auth {
                ImapAuthMethod::Password(password) => client
                    .login(&username, password)
                    .await
                    .map_err(|(err, _)| map_async_imap_error(err))?,
                ImapAuthMethod::XOAuth2 { access_token } => {
                    let authenticator = XOAuth2Authenticator {
                        username,
                        access_token: access_token.clone(),
                    };
                    client
                        .authenticate("XOAUTH2", authenticator)
                        .await
                        .map_err(|(err, _)| map_async_imap_error(err))?
                }
            };

            tracing::info!(server = %server, port, "Connexion et authentification réussies");
            Ok(session)
        };

        tokio::time::timeout(std::time::Duration::from_secs(12), connect_fut)
            .await
            .map_err(|_| {
                let msg = format!(
                    "Délai d'attente dépassé (timeout 12s) vers {server}:{port} — Le serveur ne répond pas (port bloqué ou restriction réseau fournisseur)"
                );
                tracing::error!(server = %server, port, "{msg}");
                MailProviderError::ConnectionError(msg)
            })?
    }

    async fn get_session(
        &self,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<ImapSession>>, MailProviderError> {
        let mut guard = self.session.lock().await;
        if guard.is_none() {
            *guard = Some(self.connect_session().await?);
        }
        Ok(guard)
    }

    async fn invalidate_session(&self) {
        let mut guard = self.session.lock().await;
        *guard = None;
    }
}

#[async_trait]
impl MailProviderPort for ImapProvider {
    async fn list_folders(&self) -> Result<Vec<FolderInfo>, MailProviderError> {
        let res = async {
            let mut guard = self.get_session().await?;
            let session = guard.as_mut().ok_or_else(|| {
                MailProviderError::ConnectionError("Session non initialisée".to_string())
            })?;
            let stream = session
                .list(Some(""), Some("*"))
                .await
                .map_err(map_async_imap_error)?;
            tokio::pin!(stream);

            let mut folders = Vec::new();
            while let Some(res) = stream.next().await {
                let name = res.map_err(map_async_imap_error)?;
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
        }
        .await;

        if res.is_err() {
            self.invalidate_session().await;
        }
        res
    }

    async fn get_folder_message_count(&self, folder_path: &str) -> Result<u32, MailProviderError> {
        let res = async {
            let mut guard = self.get_session().await?;
            let session = guard.as_mut().ok_or_else(|| {
                MailProviderError::ConnectionError("Session non initialisée".to_string())
            })?;
            let mailbox = session
                .select(folder_path)
                .await
                .map_err(map_async_imap_error)?;
            Ok(mailbox.exists)
        }
        .await;

        if let Err(MailProviderError::ConnectionError(_)) = &res {
            self.invalidate_session().await;
        }
        res
    }

    async fn fetch_envelopes(
        &self,
        folder_path: &str,
    ) -> Result<Vec<RawEmailEnvelope>, MailProviderError> {
        let res = async {
            let mut guard = self.get_session().await?;
            let session = guard.as_mut().ok_or_else(|| {
                MailProviderError::ConnectionError("Session non initialisée".to_string())
            })?;

            let mailbox = session
                .select(folder_path)
                .await
                .map_err(map_async_imap_error)?;
            if mailbox.exists == 0 {
                return Ok(Vec::new());
            }

            let stream = session
                .uid_fetch(
                    "1:*",
                    "(UID RFC822.SIZE FLAGS INTERNALDATE BODYSTRUCTURE BODY.PEEK[HEADER.FIELDS (Subject From To Sender Reply-To Return-Path Date Message-ID List-ID List-Unsubscribe List-Unsubscribe-Post Authentication-Results X-Spam-Status X-Spam-Flag X-Forefront-Antispam-Report X-Rspamd-Result X-OVH-Tracer-Id)])",
                )
                .await
                .map_err(map_async_imap_error)?;
            tokio::pin!(stream);

            let mut envelopes = Vec::new();
            while let Some(fetch_res) = stream.next().await {
                let fetch = fetch_res.map_err(map_async_imap_error)?;
                if let Some(env) = parse_fetch_to_envelope(&fetch) {
                    envelopes.push(env);
                }
            }

            Ok(envelopes)
        }
        .await;

        if let Err(MailProviderError::ConnectionError(_)) = &res {
            self.invalidate_session().await;
        }
        res
    }

    async fn fetch_envelopes_range(
        &self,
        folder_path: &str,
        start_seq: u32,
        end_seq: u32,
    ) -> Result<Vec<RawEmailEnvelope>, MailProviderError> {
        if start_seq == 0 || start_seq > end_seq {
            return Ok(Vec::new());
        }

        let res = async {
            let mut guard = self.get_session().await?;
            let session = guard.as_mut().ok_or_else(|| {
                MailProviderError::ConnectionError("Session non initialisée".to_string())
            })?;

            let mailbox = session
                .select(folder_path)
                .await
                .map_err(map_async_imap_error)?;
            if mailbox.exists == 0 || start_seq > mailbox.exists {
                return Ok(Vec::new());
            }

            let bounded_end = end_seq.min(mailbox.exists);
            let seq_range = format!("{start_seq}:{bounded_end}");

            let stream = session
                .fetch(
                    &seq_range,
                    "(UID RFC822.SIZE FLAGS INTERNALDATE BODYSTRUCTURE BODY.PEEK[HEADER.FIELDS (Subject From To Sender Reply-To Return-Path Date Message-ID List-ID List-Unsubscribe List-Unsubscribe-Post Authentication-Results X-Spam-Status X-Spam-Flag X-Forefront-Antispam-Report X-Rspamd-Result X-OVH-Tracer-Id)])",
                )
                .await
                .map_err(map_async_imap_error)?;
            tokio::pin!(stream);

            let mut envelopes = Vec::new();
            while let Some(fetch_res) = stream.next().await {
                let fetch = fetch_res.map_err(map_async_imap_error)?;
                if let Some(env) = parse_fetch_to_envelope(&fetch) {
                    envelopes.push(env);
                }
            }

            Ok(envelopes)
        }
        .await;

        if let Err(MailProviderError::ConnectionError(_)) = &res {
            self.invalidate_session().await;
        }
        res
    }

    async fn move_to_trash(
        &mut self,
        folder_path: &str,
        uids: &[u32],
        trash_folder_path: &str,
    ) -> Result<usize, MailProviderError> {
        if uids.is_empty() {
            return Ok(0);
        }

        let quoted_trash_for_copy = quote_mailbox_name(trash_folder_path);

        let res = async {
            let mut guard = self.get_session().await?;
            let session = guard.as_mut().ok_or_else(|| {
                MailProviderError::ConnectionError("Session non initialisée".to_string())
            })?;

            session
                .select(folder_path)
                .await
                .map_err(map_async_imap_error)?;

            let (has_move, has_uidplus) = match session.capabilities().await {
                Ok(caps) => (caps.has_str("MOVE"), caps.has_str("UIDPLUS")),
                Err(_) => (false, false),
            };

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
                if has_move && session.uid_mv(&seq, trash_folder_path).await.is_ok() {
                    moved = true;
                }

                if !moved {
                    session
                        .uid_copy(&seq, &quoted_trash_for_copy)
                        .await
                        .map_err(map_async_imap_error)?;

                    {
                        let store_stream = session
                            .uid_store(&seq, r"+FLAGS (\Deleted)")
                            .await
                            .map_err(map_async_imap_error)?;
                        tokio::pin!(store_stream);
                        while let Some(res) = store_stream.next().await {
                            let _ = res.map_err(map_async_imap_error)?;
                        }
                    }

                    if has_uidplus {
                        let _ = session.uid_expunge(&seq).await;
                    } else {
                        let _ = session.expunge().await;
                    }
                }

                moved_count += chunk_uids.len();
            }

            Ok(moved_count)
        }
        .await;

        if let Err(MailProviderError::ConnectionError(_)) = &res {
            self.invalidate_session().await;
        }
        res
    }

    async fn empty_trash(&mut self, trash_folder_path: &str) -> Result<usize, MailProviderError> {
        let res = async {
            let mut guard = self.get_session().await?;
            let session = guard.as_mut().ok_or_else(|| {
                MailProviderError::ConnectionError("Session non initialisée".to_string())
            })?;

            let mailbox = session
                .select(trash_folder_path)
                .await
                .map_err(map_async_imap_error)?;
            if mailbox.exists == 0 {
                return Ok(0);
            }

            {
                let store_stream = session
                    .store("1:*", r"+FLAGS (\Deleted)")
                    .await
                    .map_err(map_async_imap_error)?;
                tokio::pin!(store_stream);
                while let Some(res) = store_stream.next().await {
                    let _ = res.map_err(map_async_imap_error)?;
                }
            }

            // Also execute UID STORE for servers (such as Gmail) that prefer UID-based flags in virtual trash
            if let Ok(uid_store_stream) = session.uid_store("1:*", r"+FLAGS (\Deleted)").await {
                tokio::pin!(uid_store_stream);
                while let Some(res) = uid_store_stream.next().await {
                    let _ = res.map_err(map_async_imap_error);
                }
            }

            let expunge_stream = session.expunge().await.map_err(map_async_imap_error)?;
            tokio::pin!(expunge_stream);

            let mut deleted = 0;
            while let Some(res) = expunge_stream.next().await {
                let _ = res.map_err(map_async_imap_error)?;
                deleted += 1;
            }

            Ok(deleted.max(mailbox.exists as usize))
        }
        .await;

        if let Err(MailProviderError::ConnectionError(_)) = &res {
            self.invalidate_session().await;
        }
        res
    }

    async fn fetch_body_snippet(
        &mut self,
        folder_path: &str,
        uid: u32,
    ) -> Result<String, MailProviderError> {
        let uid_str = uid.to_string();

        let fetch_fut = async {
            let mut guard = self.get_session().await?;
            let session = guard.as_mut().ok_or_else(|| {
                MailProviderError::ConnectionError("Session non initialisée".to_string())
            })?;

            session
                .select(folder_path)
                .await
                .map_err(map_async_imap_error)?;

            // 1. Fetch email up to 64KB via BODY.PEEK[] without marking as read
            let mut extracted = None;
            if let Ok(stream) = session.uid_fetch(&uid_str, "(BODY.PEEK[]<0.65536>)").await {
                tokio::pin!(stream);
                while let Some(res) = stream.next().await {
                    if let Ok(fetch) = res {
                        if extracted.is_none() {
                            if let Some(bytes) = fetch.body().or_else(|| fetch.text()) {
                                let preview = extract_body_preview(bytes);
                                if !preview.trim().is_empty() {
                                    extracted = Some(preview);
                                }
                            }
                        }
                    }
                }
            }

            if let Some(preview) = extracted {
                return Ok(preview);
            }

            // 2. Fallback to full BODY.PEEK[]
            let stream = session
                .uid_fetch(&uid_str, "(BODY.PEEK[])")
                .await
                .map_err(map_async_imap_error)?;
            tokio::pin!(stream);

            let mut full_extracted = None;
            while let Some(res) = stream.next().await {
                let fetch = res.map_err(map_async_imap_error)?;
                if full_extracted.is_none() {
                    if let Some(bytes) = fetch.body().or_else(|| fetch.text()) {
                        let preview = extract_body_preview(bytes);
                        if !preview.trim().is_empty() {
                            full_extracted = Some(preview);
                        }
                    }
                }
            }

            Ok(full_extracted.unwrap_or_default())
        };

        let res = match tokio::time::timeout(std::time::Duration::from_secs(10), fetch_fut).await {
            Ok(inner_res) => inner_res,
            Err(_) => {
                tracing::warn!(uid, %folder_path, "Délai dépassé (timeout 10s) pour l'aperçu de l'email");
                Err(MailProviderError::ConnectionError(
                    "Délai d'attente dépassé (timeout 10s) lors de la récupération de l'aperçu depuis le serveur IMAP".to_string(),
                ))
            }
        };

        if res.is_err() {
            self.invalidate_session().await;
        }
        res
    }
}
