//! Tests for IMAP provider adapter.
#![allow(missing_docs)]

use async_imap::imap_proto::types::{
    BodyContentCommon, BodyContentSinglePart, BodyStructure, ContentDisposition, ContentEncoding,
    ContentType, NameAttribute,
};
use mailpurge_core::folder_scanner::FolderAttribute;
use mailpurge_core::imap_provider::{
    extract_attachment_names, map_imap_attribute, parse_raw_headers, ImapAuthMethod, ImapConfig,
    XOAuth2Authenticator,
};
use std::borrow::Cow;

#[test]
fn test_xoauth2_authenticator_format() {
    use async_imap::Authenticator;

    let mut auth = XOAuth2Authenticator {
        username: "test@domain.com".to_string(),
        access_token: "ya29.secret_token".to_string(),
    };

    let response = auth.process(b"");
    assert_eq!(
        response,
        "user=test@domain.com\x01auth=Bearer ya29.secret_token\x01\x01"
    );
}

#[test]
fn test_map_imap_attribute() {
    assert_eq!(
        map_imap_attribute(&NameAttribute::Trash),
        FolderAttribute::Trash
    );
    assert_eq!(
        map_imap_attribute(&NameAttribute::Sent),
        FolderAttribute::Sent
    );
    assert_eq!(
        map_imap_attribute(&NameAttribute::Drafts),
        FolderAttribute::Drafts
    );
    assert_eq!(
        map_imap_attribute(&NameAttribute::Junk),
        FolderAttribute::Junk
    );
    assert_eq!(
        map_imap_attribute(&NameAttribute::All),
        FolderAttribute::All
    );
    assert_eq!(
        map_imap_attribute(&NameAttribute::Archive),
        FolderAttribute::Archive
    );
    assert_eq!(
        map_imap_attribute(&NameAttribute::NoSelect),
        FolderAttribute::NoSelect
    );
    assert_eq!(
        map_imap_attribute(&NameAttribute::Extension(Cow::Borrowed("CustomTag"))),
        FolderAttribute::Other("CustomTag".to_string())
    );
}

#[test]
fn test_extract_attachment_names_from_bodystructure_single_pdf() {
    let bs = BodyStructure::Basic {
        common: BodyContentCommon {
            ty: ContentType {
                ty: Cow::Borrowed("application"),
                subtype: Cow::Borrowed("pdf"),
                params: None,
            },
            disposition: Some(ContentDisposition {
                ty: Cow::Borrowed("attachment"),
                params: Some(vec![(
                    Cow::Borrowed("filename"),
                    Cow::Borrowed("facture_edf.pdf"),
                )]),
            }),
            language: None,
            location: None,
        },
        other: BodyContentSinglePart {
            id: None,
            md5: None,
            octets: 120_000,
            description: None,
            transfer_encoding: ContentEncoding::Base64,
        },
        extension: None,
    };

    let attachments = extract_attachment_names(&bs);
    assert_eq!(attachments, vec!["facture_edf.pdf".to_string()]);
}

#[test]
fn test_extract_attachment_names_from_bodystructure_multipart() {
    let text_part = BodyStructure::Text {
        common: BodyContentCommon {
            ty: ContentType {
                ty: Cow::Borrowed("text"),
                subtype: Cow::Borrowed("plain"),
                params: None,
            },
            disposition: None,
            language: None,
            location: None,
        },
        other: BodyContentSinglePart {
            id: None,
            md5: None,
            octets: 500,
            description: None,
            transfer_encoding: ContentEncoding::SevenBit,
        },
        lines: 10,
        extension: None,
    };

    let pdf_part = BodyStructure::Basic {
        common: BodyContentCommon {
            ty: ContentType {
                ty: Cow::Borrowed("application"),
                subtype: Cow::Borrowed("pdf"),
                params: Some(vec![(
                    Cow::Borrowed("name"),
                    Cow::Borrowed("recu_fiscal.pdf"),
                )]),
            },
            disposition: Some(ContentDisposition {
                ty: Cow::Borrowed("attachment"),
                params: None,
            }),
            language: None,
            location: None,
        },
        other: BodyContentSinglePart {
            id: None,
            md5: None,
            octets: 45_000,
            description: None,
            transfer_encoding: ContentEncoding::Base64,
        },
        extension: None,
    };

    let multipart = BodyStructure::Multipart {
        common: BodyContentCommon {
            ty: ContentType {
                ty: Cow::Borrowed("multipart"),
                subtype: Cow::Borrowed("mixed"),
                params: None,
            },
            disposition: None,
            language: None,
            location: None,
        },
        bodies: vec![text_part, pdf_part],
        extension: None,
    };

    let attachments = extract_attachment_names(&multipart);
    assert_eq!(attachments, vec!["recu_fiscal.pdf".to_string()]);
}

#[test]
fn test_parse_raw_headers() {
    let raw = b"From: =?utf-8?B?QWNwbWUgQ29ycA==?= <billing@acme.com>\r\n\
Subject: =?utf-8?Q?Facture_N=C2=B0_42001?=\r\n\
Message-ID: <msg-9999@acme.com>\r\n\
List-Unsubscribe: <https://acme.com/unsub>, <mailto:unsub@acme.com>\r\n\
Date: Mon, 9 Nov 2026 10:00:00 +0100\r\n\
\r\n";

    let parsed = parse_raw_headers(raw);
    assert_eq!(parsed.from, "Acpme Corp <billing@acme.com>");
    assert_eq!(parsed.subject, "Facture N° 42001");
    assert_eq!(parsed.message_id.as_deref(), Some("<msg-9999@acme.com>"));
    assert!(parsed.has_unsubscribe);
}

#[test]
fn test_parse_raw_headers_no_unsubscribe() {
    let raw = b"From: Alice <alice@example.org>\r\n\
Subject: Hello there\r\n\
Message-ID: <msg-001@example.org>\r\n\
\r\n";

    let parsed = parse_raw_headers(raw);
    assert_eq!(parsed.from, "Alice <alice@example.org>");
    assert_eq!(parsed.subject, "Hello there");
    assert_eq!(parsed.message_id.as_deref(), Some("<msg-001@example.org>"));
    assert!(!parsed.has_unsubscribe);
}

#[test]
fn test_imap_config_construction() {
    let config = ImapConfig {
        server: "imap.mail.me.com".to_string(),
        port: 993,
        username: "user@icloud.com".to_string(),
        auth: ImapAuthMethod::Password("app-spec-password".to_string()),
    };

    assert_eq!(config.server, "imap.mail.me.com");
    assert_eq!(config.port, 993);
    assert_eq!(config.username, "user@icloud.com");
}

#[test]
fn test_quote_mailbox_name() {
    use mailpurge_core::imap_provider::quote_mailbox_name;

    assert_eq!(quote_mailbox_name("Trash"), "\"Trash\"");
    assert_eq!(
        quote_mailbox_name("Éléments supprimés"),
        "\"Éléments supprimés\""
    );
    assert_eq!(
        quote_mailbox_name("\"Already Quoted\""),
        "\"Already Quoted\""
    );
    assert_eq!(quote_mailbox_name("Folder\\Sub"), "\"Folder\\\\Sub\"");
}

#[test]
fn test_extract_body_preview_plain_text() {
    use mailpurge_core::imap_provider::extract_body_preview;

    let raw = b"Content-Type: text/plain; charset=utf-8\r\n\r\nBonjour ceci est un apercu de test.";
    assert_eq!(
        extract_body_preview(raw),
        "Bonjour ceci est un apercu de test."
    );
}

#[test]
fn test_extract_body_preview_html() {
    use mailpurge_core::imap_provider::extract_body_preview;

    let raw = b"Content-Type: text/html; charset=utf-8\r\n\r\n<html><body><p>Offre <b>speciale</b> pour vous !</p></body></html>";
    assert_eq!(extract_body_preview(raw), "Offre speciale pour vous !");

    // HTML with style tags, head, scripts and entities (like Spotify receipts)
    let raw_spotify = b"Content-Type: text/html; charset=utf-8\r\n\r\n<html><head><style type=\"text/css\">\r\n@media only screen and (min-device-width: 481px) {\r\ndiv[id=\"main\"] { width: 480px !important; }\r\n}\r\nbody { font-family: Helvetica; }\r\n</style></head><body><p>Votre re\xc3\xa7u Spotify : <b>9,99 &euro;</b></p><script>console.log('test');</script></body></html>";
    let preview = extract_body_preview(raw_spotify);
    assert!(!preview.contains("@media"));
    assert!(!preview.contains("font-family"));
    assert!(!preview.contains("console.log"));
    assert!(preview.contains("Votre reçu Spotify : 9,99 €"));

    // Raw body without headers
    let raw_qp_body = b"<!DOCTYPE html>\r\n<html xmlns=3D\"http://www.w3.org/1999/xhtml\" style=3D\"margin: 0; padding: 0=\r\n\">\r\n<head>\r\n<title></title>\r\n<style type=3D\"text/css\">\r\n@media only screen and (min-device-width: 481px) {\r\n}\r\n</style>\r\n</head>\r\n<body><p>Test QP</p></body></html>";
    let preview_qp = extract_body_preview(raw_qp_body);
    assert!(!preview_qp.contains("<!DOCTYPE"));
    assert!(!preview_qp.contains("<html"));
}

#[test]
fn test_parse_raw_headers_extended_spam_and_auth() {
    let raw = b"From: spoofed@brand.com\r\n\
                To: victim@domain.com\r\n\
                Return-Path: <bounce@compromised-server.net>\r\n\
                Authentication-Results: mx.google.com; spf=fail (google.com: domain does not designate 1.2.3.4 as permitted sender)\r\n\
                X-Spam-Flag: YES\r\n\
                X-Spam-Status: Yes, score=6.5\r\n\
                X-Forefront-Antispam-Report: SCL:9;CAT:SPM\r\n\
                Subject: Important invoice\r\n\r\n";

    let headers = parse_raw_headers(raw);
    assert_eq!(headers.from, "spoofed@brand.com");
    assert_eq!(headers.to_header, Some("victim@domain.com".to_string()));
    assert_eq!(
        headers.return_path,
        Some("<bounce@compromised-server.net>".to_string())
    );
    assert!(headers
        .authentication_results
        .as_ref()
        .unwrap()
        .contains("spf=fail"));
    let spam = headers.spam_verdict_header.unwrap();
    assert!(spam.contains("X-Spam-Flag: YES"));
    assert!(spam.contains("X-Spam-Status: Yes, score=6.5"));
    assert!(spam.contains("X-Forefront-Antispam-Report: SCL:9;CAT:SPM"));
}
