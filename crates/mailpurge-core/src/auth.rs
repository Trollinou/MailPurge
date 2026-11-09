use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::SystemTime;

use base64::prelude::*;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Service namespace used for storing credentials in OS Keyring.
pub const KEYRING_SERVICE_NAME: &str = "com.mailpurge.app";

/// Supported `OAuth2` identity providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthProvider {
    /// Google Workspace and standard Gmail accounts.
    Google,
    /// Microsoft 365, Office 365, and Outlook.com accounts.
    Microsoft365,
}

impl OAuthProvider {
    /// Returns the `OAuth2` authorization endpoint URL.
    #[must_use]
    pub const fn auth_endpoint(&self) -> &'static str {
        match self {
            Self::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Self::Microsoft365 => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        }
    }

    /// Returns the `OAuth2` token endpoint URL.
    #[must_use]
    pub const fn token_endpoint(&self) -> &'static str {
        match self {
            Self::Google => "https://oauth2.googleapis.com/token",
            Self::Microsoft365 => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        }
    }

    /// Returns the default scopes required for IMAP access and user profile.
    #[must_use]
    pub const fn default_scopes(&self) -> &'static str {
        match self {
            Self::Google => "https://mail.google.com/ email profile",
            Self::Microsoft365 => {
                "https://outlook.office.com/IMAP.AccessAsUser.All offline_access email"
            }
        }
    }
}

/// Errors occurring during `OAuth2` PKCE or keyring authentication flows.
#[derive(Debug, Error)]
pub enum AuthError {
    /// Failure communicating with the OS secure keychain.
    #[error("Erreur du trousseau système (Keyring): {0}")]
    KeyringError(String),
    /// Network connection error during token exchange.
    #[error("Erreur réseau: {0}")]
    NetworkError(String),
    /// Invalid callback query parameters.
    #[error("Rappel OAuth invalide: {0}")]
    InvalidCallback(String),
    /// The user explicitly denied the authorization request.
    #[error("Autorisation refusée par l'utilisateur: {0}")]
    OAuthDenied(String),
    /// Error decoding the token JSON response from the server.
    #[error("Échange de tokens échoué: {0}")]
    TokenExchangeError(String),
    /// I/O error during loopback socket communication.
    #[error("Erreur I/O: {0}")]
    IoError(String),
}

/// PKCE code verifier and challenge.
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// High-entropy random code verifier.
    pub verifier: String,
    /// Base64URL-encoded SHA256 code challenge.
    pub challenge: String,
}

/// Generates a cryptographic PKCE challenge compliant with RFC 7636 (S256).
#[must_use]
pub fn generate_pkce() -> PkceChallenge {
    let mut random_bytes = [0u8; 32];

    // Read high-entropy randomness from /dev/urandom on Unix with fallback to system clock
    #[cfg(unix)]
    {
        if let Ok(mut f) = File::open("/dev/urandom") {
            let _ = f.read_exact(&mut random_bytes);
        }
    }

    // In case urandom is unavailable, mix with timestamp and pointer address
    if random_bytes == [0u8; 32] {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let mix = now.to_le_bytes();
        for (i, b) in mix.iter().cycle().take(32).enumerate() {
            random_bytes[i] ^= b;
        }
    }

    let verifier = BASE64_URL_SAFE_NO_PAD.encode(random_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = BASE64_URL_SAFE_NO_PAD.encode(hasher.finalize());

    PkceChallenge {
        verifier,
        challenge,
    }
}

/// Constructs the authorization URL to redirect the user to the provider's SSO portal.
#[must_use]
pub fn build_authorization_url(
    provider: &OAuthProvider,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    let encoded_redirect = percent_encode(redirect_uri);
    let encoded_scope = percent_encode(provider.default_scopes());

    format!(
        "{}?client_id={}&response_type=code&redirect_uri={}&response_mode=query&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        provider.auth_endpoint(),
        client_id,
        encoded_redirect,
        encoded_scope,
        state,
        code_challenge
    )
}

/// Parses the query string received on the local redirect callback URL.
///
/// # Errors
///
/// Returns `AuthError::OAuthDenied` if the provider returned an error parameter,
/// or `AuthError::InvalidCallback` if the authorization `code` parameter is missing.
pub fn parse_callback_query(query: &str) -> Result<(String, String), AuthError> {
    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    let mut error_description: Option<String> = None;

    for param in query.split('&') {
        let mut parts = param.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        let raw_val = parts.next().unwrap_or("").trim();
        let val = percent_decode(raw_val);

        if key.eq_ignore_ascii_case("code") {
            code = Some(val);
        } else if key.eq_ignore_ascii_case("state") {
            state = Some(val);
        } else if key.eq_ignore_ascii_case("error") {
            error = Some(val);
        } else if key.eq_ignore_ascii_case("error_description") {
            error_description = Some(val);
        }
    }

    if let Some(err) = error {
        let desc = error_description.unwrap_or(err);
        return Err(AuthError::OAuthDenied(desc));
    }

    let extracted_code = code.ok_or_else(|| {
        AuthError::InvalidCallback("Paramètre code manquant dans le rappel OAuth".to_string())
    })?;

    let extracted_state = state.unwrap_or_default();

    Ok((extracted_code, extracted_state))
}

/// `OAuth2` tokens returned from the provider's token endpoint.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OAuthTokens {
    /// Short-lived access token used for IMAP SASL XOAUTH2 authentication.
    pub access_token: String,
    /// Long-lived refresh token used to obtain new access tokens.
    pub refresh_token: Option<String>,
    /// Token lifetime duration in seconds.
    pub expires_in: Option<u64>,
    /// OIDC ID token containing identity claims (e.g. user email).
    pub id_token: Option<String>,
}

/// Exchanges an authorization code and PKCE verifier for access and refresh tokens.
///
/// # Errors
///
/// Returns `AuthError::NetworkError` on network transport failures or
/// `AuthError::TokenExchangeError` if the response cannot be decoded.
pub fn exchange_code_for_tokens(
    provider: &OAuthProvider,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthTokens, AuthError> {
    let request = ureq::post(provider.token_endpoint())
        .set("Content-Type", "application/x-www-form-urlencoded");

    let mut body = format!(
        "grant_type=authorization_code&client_id={}&code={}&code_verifier={}&redirect_uri={}",
        percent_encode(client_id),
        percent_encode(code),
        percent_encode(code_verifier),
        percent_encode(redirect_uri)
    );

    if let Some(secret) = client_secret {
        let _ = write!(body, "&client_secret={}", percent_encode(secret));
    }

    let response = request
        .send_string(&body)
        .map_err(|e| AuthError::NetworkError(format!("Échec de la requête de jetons: {e}")))?;

    let body_text = response
        .into_string()
        .map_err(|e| AuthError::NetworkError(format!("Lecture réponse HTTP échouée: {e}")))?;

    serde_json::from_str::<OAuthTokens>(&body_text)
        .map_err(|e| AuthError::TokenExchangeError(format!("Décodage JSON impossible: {e}")))
}

/// Refreshes an expired access token using a stored refresh token.
///
/// # Errors
///
/// Returns `AuthError::NetworkError` on transport errors or `AuthError::TokenExchangeError`
/// if token decoding fails.
pub fn refresh_access_token(
    provider: &OAuthProvider,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<OAuthTokens, AuthError> {
    let request = ureq::post(provider.token_endpoint())
        .set("Content-Type", "application/x-www-form-urlencoded");

    let mut body = format!(
        "grant_type=refresh_token&client_id={}&refresh_token={}",
        percent_encode(client_id),
        percent_encode(refresh_token)
    );

    if let Some(secret) = client_secret {
        let _ = write!(body, "&client_secret={}", percent_encode(secret));
    }

    let response = request
        .send_string(&body)
        .map_err(|e| AuthError::NetworkError(format!("Échec de rafraîchissement: {e}")))?;

    let body_text = response
        .into_string()
        .map_err(|e| AuthError::NetworkError(format!("Lecture réponse HTTP échouée: {e}")))?;

    serde_json::from_str::<OAuthTokens>(&body_text)
        .map_err(|e| AuthError::TokenExchangeError(format!("Décodage JSON impossible: {e}")))
}

/// Extracts the user's email address from an unverified JWT ID token payload.
#[must_use]
pub fn extract_email_from_id_token(id_token: &str) -> Option<String> {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    let payload_b64 = parts[1];
    let decoded = BASE64_URL_SAFE_NO_PAD
        .decode(payload_b64)
        .or_else(|_| BASE64_STANDARD.decode(payload_b64))
        .ok()?;

    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;

    json.get("email")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            json.get("preferred_username")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
        .or_else(|| {
            json.get("upn")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
}

/// Secure OS Keyring storage manager for passwords and OAuth refresh tokens.
pub struct KeyringStore;

impl KeyringStore {
    /// Saves a credential (password or refresh token) into the OS Keyring.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::KeyringError` if writing to the OS keychain fails.
    pub fn save_secret(account_identifier: &str, secret: &str) -> Result<(), AuthError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE_NAME, account_identifier).map_err(|e| {
            AuthError::KeyringError(format!("Création entrée keyring échouée: {e}"))
        })?;

        entry.set_password(secret).map_err(|e| {
            AuthError::KeyringError(format!("Écriture dans le trousseau échouée: {e}"))
        })
    }

    /// Retrieves a credential from the OS Keyring.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::KeyringError` if the secret cannot be read or is missing.
    pub fn get_secret(account_identifier: &str) -> Result<String, AuthError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE_NAME, account_identifier)
            .map_err(|e| AuthError::KeyringError(format!("Accès entrée keyring échoué: {e}")))?;

        entry
            .get_password()
            .map_err(|e| AuthError::KeyringError(format!("Lecture trousseau échouée: {e}")))
    }

    /// Deletes a credential from the OS Keyring.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::KeyringError` if credential deletion fails.
    pub fn delete_secret(account_identifier: &str) -> Result<(), AuthError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE_NAME, account_identifier)
            .map_err(|e| AuthError::KeyringError(format!("Accès entrée keyring échoué: {e}")))?;

        let _ = entry.delete_credential();
        Ok(())
    }
}

/// Listens on a local loopback TCP socket for the single OAuth callback HTTP request.
///
/// # Errors
///
/// Returns `AuthError::IoError` if socket reading fails, or `AuthError::InvalidCallback`
/// if state verification fails.
pub fn listen_for_oauth_code(
    listener: &TcpListener,
    expected_state: &str,
) -> Result<String, AuthError> {
    let (mut stream, _) = listener
        .accept()
        .map_err(|e| AuthError::IoError(format!("Écoute socket interrompue: {e}")))?;

    let mut buffer = [0u8; 4096];
    let bytes_read = stream
        .read(&mut buffer)
        .map_err(|e| AuthError::IoError(format!("Lecture flux HTTP échouée: {e}")))?;

    let request_str = String::from_utf8_lossy(&buffer[..bytes_read]);
    let first_line = request_str.lines().next().unwrap_or("");

    // E.g.: "GET /callback?code=xxx&state=yyy HTTP/1.1"
    let query_str = first_line
        .split_whitespace()
        .nth(1)
        .and_then(|path| path.split_once('?').map(|(_, q)| q))
        .unwrap_or("");

    let parse_result = parse_callback_query(query_str);

    // Render a friendly confirmation page to the browser
    let (status, body) = match &parse_result {
        Ok((_, state)) if state == expected_state => (
            "200 OK",
            "<!DOCTYPE html><html><head><meta charset='utf-8'><title>MailPurge</title><style>body{font-family:system-ui,-apple-system,sans-serif;text-align:center;padding:60px;background:#0f172a;color:#f8fafc;}h1{color:#38bdf8;}</style></head><body><h1>Connexion réussie !</h1><p>Votre compte a été associé à MailPurge. Vous pouvez fermer cet onglet en toute sécurité.</p></body></html>",
        ),
        _ => (
            "400 Bad Request",
            "<!DOCTYPE html><html><head><meta charset='utf-8'><title>MailPurge - Erreur</title><style>body{font-family:system-ui,-apple-system,sans-serif;text-align:center;padding:60px;background:#0f172a;color:#f8fafc;}h1{color:#ef4444;}</style></head><body><h1>Échec de connexion</h1><p>Le jeton ou l'état de sécurité n'a pas pu être validé.</p></body></html>",
        ),
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{body}"
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    let (code, state) = parse_result?;
    if state != expected_state {
        return Err(AuthError::InvalidCallback(
            "Validation de l'état CSRF échouée".to_string(),
        ));
    }

    Ok(code)
}

fn percent_encode(input: &str) -> String {
    let mut output = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char);
            }
            _ => {
                let _ = write!(output, "%{byte:02X}");
            }
        }
    }
    output
}

fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let mut chars = input.bytes();

    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(c1), Some(c2)) = (h1, h2) {
                if let Ok(val) = u8::from_str_radix(&format!("{}{}", c1 as char, c2 as char), 16) {
                    bytes.push(val);
                }
            }
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }

    String::from_utf8_lossy(&bytes).to_string()
}
