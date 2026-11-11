//! Tests for `OAuth2` PKCE and OS Keyring authentication.
#![allow(missing_docs)]

use base64::prelude::*;
use mailpurge_core::auth::{
    build_authorization_url, extract_email_from_id_token, generate_pkce, parse_callback_query,
    AuthError, OAuthProvider,
};
use sha2::{Digest, Sha256};

#[test]
fn test_generate_pkce_s256_validity() {
    let pkce = generate_pkce();

    // Verifier should be between 43 and 128 characters
    assert!(pkce.verifier.len() >= 43 && pkce.verifier.len() <= 128);

    // Verify S256 relationship: challenge = BASE64URL_NOPAD(SHA256(verifier))
    let mut hasher = Sha256::new();
    hasher.update(pkce.verifier.as_bytes());
    let hash = hasher.finalize();
    let expected_challenge = BASE64_URL_SAFE_NO_PAD.encode(hash);

    assert_eq!(pkce.challenge, expected_challenge);
}

#[test]
fn test_build_authorization_url_google() {
    let url = build_authorization_url(
        &OAuthProvider::Google,
        "dummy_client_id.apps.googleusercontent.com",
        "http://127.0.0.1:8080/callback",
        "xyzstate",
        "challenge123",
    );

    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(url.contains("client_id=dummy_client_id.apps.googleusercontent.com"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8080%2Fcallback"));
    assert!(url.contains("state=xyzstate"));
    assert!(url.contains("code_challenge=challenge123"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("scope="));
}

#[test]
fn test_build_authorization_url_microsoft() {
    let url = build_authorization_url(
        &OAuthProvider::Microsoft365,
        "ms-client-id-uuid",
        "http://127.0.0.1:9090/callback",
        "state_ms",
        "ms_challenge_456",
    );

    assert!(url.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"));
    assert!(url.contains("client_id=ms-client-id-uuid"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A9090%2Fcallback"));
    assert!(url.contains("state=state_ms"));
    assert!(url.contains("code_challenge=ms_challenge_456"));
    assert!(url.contains("code_challenge_method=S256"));
}

#[test]
fn test_parse_callback_query_success() {
    let query = "code=auth_code_12345&state=my_csrf_state&session_state=abc";
    let (code, state) = parse_callback_query(query).expect("Parsing should succeed");

    assert_eq!(code, "auth_code_12345");
    assert_eq!(state, "my_csrf_state");
}

#[test]
fn test_parse_callback_query_url_encoded() {
    let query = "code=code%2Fwith%2Bspecial%3D&state=state%201";
    let (code, state) = parse_callback_query(query).expect("Parsing should succeed");

    assert_eq!(code, "code/with+special=");
    assert_eq!(state, "state 1");
}

#[test]
fn test_parse_callback_query_error_response() {
    let query = "error=access_denied&error_description=User%20declined%20consent";
    let err = parse_callback_query(query).expect_err("Should return error");

    assert!(matches!(
        err,
        AuthError::OAuthDenied(desc) if desc.contains("User declined consent")
    ));
}

#[test]
fn test_parse_callback_query_missing_code() {
    let query = "state=just_state";
    let err = parse_callback_query(query).expect_err("Should return error");

    assert!(matches!(
        err,
        AuthError::InvalidCallback(msg) if msg.contains("code manquant")
    ));
}

#[test]
fn test_extract_email_from_id_token() {
    // Standard JWT: header.payload.signature
    // Payload contains {"email": "user@company.com", "sub": "12345"}
    // Base64URL for {"email":"user@company.com","sub":"12345"}
    // is: eyJlbWFpbCI6InVzZXJAY29tcGFueS5jb20iLCJzdWIiOiIxMjM0NSJ9
    let token =
        "eyJhbGciOiJSUzI1NiJ9.eyJlbWFpbCI6InVzZXJAY29tcGFueS5jb20iLCJzdWIiOiIxMjM0NSJ9.dummy_sig";

    let email = extract_email_from_id_token(token);
    assert_eq!(email.as_deref(), Some("user@company.com"));
}

#[test]
fn test_extract_email_from_malformed_id_token() {
    assert_eq!(extract_email_from_id_token("not-a-token"), None);
    assert_eq!(extract_email_from_id_token("part1.notbase64.part3"), None);
}

#[test]
fn test_keyring_store_mock_or_os() {
    use mailpurge_core::auth::KeyringStore;
    let email = "test_user_keyring_val@domain.com";
    let password = "super_secret_password_123";
    let save_res = KeyringStore::save_secret(email, password);
    if save_res.is_ok() {
        let get_res = KeyringStore::get_secret(email);
        assert_eq!(get_res.as_deref().unwrap(), password);
        let _ = KeyringStore::delete_secret(email);
    }
}
