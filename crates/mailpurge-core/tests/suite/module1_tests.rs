#![allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]

use mailpurge_core::classifier::{
    classify_email, ClassifierConfig, EmailClassification, EmailMetadataInput,
};
use mailpurge_core::domain_extractor::{
    extract_domain_from_list_id, extract_sender_domain, DomainExtractError,
};
use mailpurge_core::imap_sequence::{chunk_sequence_set, format_sequence_set};

#[test]
fn test_1_extract_root_domain_complex_addresses() {
    // 1. Display name + multi-level subdomain
    let res = extract_sender_domain("\"Service Client\" <no-reply@mail.support.societe.fr>")
        .expect("Should parse complex address");
    assert_eq!(res.email_address, "no-reply@mail.support.societe.fr");
    assert_eq!(res.full_domain, "mail.support.societe.fr");
    assert_eq!(res.root_domain, "societe.fr");
    assert_eq!(res.display_name.as_deref(), Some("Service Client"));

    // 2. ccTLD / eTLD+1 (.co.uk)
    let res = extract_sender_domain("facture@service-client.amazon.co.uk")
        .expect("Should parse amazon.co.uk");
    assert_eq!(res.email_address, "facture@service-client.amazon.co.uk");
    assert_eq!(res.full_domain, "service-client.amazon.co.uk");
    assert_eq!(res.root_domain, "amazon.co.uk");

    // 3. Simple email with subdomain
    let res =
        extract_sender_domain("promo@newsletter.ldlc.com").expect("Should parse ldlc newsletter");
    assert_eq!(res.email_address, "promo@newsletter.ldlc.com");
    assert_eq!(res.full_domain, "newsletter.ldlc.com");
    assert_eq!(res.root_domain, "ldlc.com");

    // 4. Naked email
    let res = extract_sender_domain("contact@fnac.com").expect("Should parse fnac");
    assert_eq!(res.email_address, "contact@fnac.com");
    assert_eq!(res.full_domain, "fnac.com");
    assert_eq!(res.root_domain, "fnac.com");
}

#[test]
fn test_extract_root_domain_edge_cases() {
    // Empty header
    assert!(matches!(
        extract_sender_domain("   "),
        Err(DomainExtractError::InvalidHeader)
    ));

    // Invalid header without valid email syntax
    assert!(matches!(
        extract_sender_domain("invalid-address"),
        Err(DomainExtractError::InvalidHeader)
    ));

    // Fallback on localhost
    let res = extract_sender_domain("admin@localhost").expect("Should parse localhost");
    assert_eq!(res.root_domain, "localhost");
    assert_eq!(res.full_domain, "localhost");

    // Real world edge cases
    let res = extract_sender_domain("Jean Dupont, SARL <jean@dupont.fr>")
        .expect("Unquoted comma in display name");
    assert_eq!(res.root_domain, "dupont.fr");
    assert_eq!(res.email_address, "jean@dupont.fr");

    let res =
        extract_sender_domain("=?UTF-8?Q?Le_Monde_-_L'=C3=A9dition_du_soir?= <contact@lemonde.fr>")
            .expect("MIME encoded words");
    assert_eq!(res.root_domain, "lemonde.fr");
    assert_eq!(res.email_address, "contact@lemonde.fr");

    let res = extract_sender_domain("<contact@service.free.fr>").expect("Angle brackets only");
    assert_eq!(res.root_domain, "free.fr");

    let res = extract_sender_domain("contact@service.free.fr (Support Technique)")
        .expect("RFC 822 comment style");
    assert_eq!(res.root_domain, "free.fr");

    let res = extract_sender_domain("Support < contact@service.free.fr >")
        .expect("Spaces inside angle brackets");
    assert_eq!(res.root_domain, "free.fr");

    let res = extract_sender_domain("From: \"Newsletter\" <news@info.fnac.com>")
        .expect("Prefix From: included");
    assert_eq!(res.root_domain, "fnac.com");

    let res = extract_sender_domain("contact@badformat.com>").expect("Stray closing bracket");
    assert_eq!(res.root_domain, "badformat.com");

    // Multi-byte UTF-8 in display name (e.g. É, è, ç, etc. which panicked on byte boundary slicing)
    let res = extract_sender_domain("Équipe Game On Tabletop <contact@gameontabletop.com>")
        .expect("Multi-byte UTF-8 character at start of display name");
    assert_eq!(res.root_domain, "gameontabletop.com");
    assert_eq!(res.email_address, "contact@gameontabletop.com");

    let res = extract_sender_domain("L'Équipe <newsletter@lequipe.fr>")
        .expect("Multi-byte UTF-8 character in display name");
    assert_eq!(res.root_domain, "lequipe.fr");

    let res = extract_sender_domain("Société Générale <securite@societegenerale.fr>")
        .expect("Accented display name");
    assert_eq!(res.root_domain, "societegenerale.fr");

    let res = extract_sender_domain("北京 <info@beijing.cn>").expect("CJK multi-byte display name");
    assert_eq!(res.root_domain, "beijing.cn");

    // List-ID tests
    assert_eq!(
        extract_domain_from_list_id("<newsletter.fnac.com>"),
        Some("fnac.com".to_string())
    );
    assert_eq!(
        extract_domain_from_list_id("\"Offres Spéciales\" <promos.vente-privee.com>"),
        Some("vente-privee.com".to_string())
    );
    assert_eq!(extract_domain_from_list_id(""), None);
}

#[test]
fn test_2_detect_transactional_keywords_case_and_accent_insensitive() {
    let config = ClassifierConfig::default();
    let ref_time = 1_700_000_000i64; // arbitrary reference timestamp
    let old_time = ref_time - (70 * 86400); // 70 days old (> 60 days)

    let test_subjects = vec![
        "Votre Facture N° 12345",
        "Confirmation de votre commande Fnac",
        "Votre REÇU de paiement",
        "Your order has been shipped",
        "Monthly invoice statement",
        "Votre e-billet de train",
        "Confirmation de réservation d'hôtel",
        "Certificat de garantie Darty",
        "Votre contrat d'assurance",
        "Avoir disponible sur votre compte",
    ];

    for subject in test_subjects {
        let meta = EmailMetadataInput {
            uid: 1,
            subject,
            received_at_timestamp: old_time,
            is_flagged: false,
            has_unsubscribe: false,
            attachment_names: &[],
            root_domain: None,
            spam_verdict: None,
        };
        let classification = classify_email(&meta, &config, ref_time);
        assert_eq!(
            classification,
            EmailClassification::ProtectedInvoice,
            "Subject '{}' should be classified as ProtectedInvoice",
            subject
        );
    }
}

#[test]
fn test_3_protect_email_with_pdf_and_no_unsubscribe_even_ambiguous_subject() {
    let config = ClassifierConfig::default();
    let ref_time = 1_700_000_000i64;
    let old_time = ref_time - (90 * 86400); // 90 days old

    // Subject has NO keyword, but has PDF attachment and NO List-Unsubscribe
    let meta = EmailMetadataInput {
        uid: 42,
        subject: "Documents importants pour votre dossier",
        received_at_timestamp: old_time,
        is_flagged: false,
        has_unsubscribe: false,
        attachment_names: &["document_annexe.PDF".to_string()],
        root_domain: None,
        spam_verdict: None,
    };
    let classification = classify_email(&meta, &config, ref_time);
    assert_eq!(
        classification,
        EmailClassification::ProtectedInvoice,
        "PDF without unsubscribe header must be protected"
    );

    // If it has List-Unsubscribe, it's a newsletter/marketing even with a PDF attached
    let newsletter_meta = EmailMetadataInput {
        uid: 43,
        subject: "Découvrez notre catalogue en PDF",
        received_at_timestamp: old_time,
        is_flagged: false,
        has_unsubscribe: true,
        attachment_names: &["catalogue.pdf".to_string()],
        root_domain: Some("fnac.com"),
        spam_verdict: None,
    };
    let newsletter_class = classify_email(&newsletter_meta, &config, ref_time);
    assert_eq!(
        newsletter_class,
        EmailClassification::Purgeable,
        "Newsletter with List-Unsubscribe should be Purgeable even with a PDF"
    );
}

#[test]
fn test_protect_flagged_email() {
    let config = ClassifierConfig::default();
    let ref_time = 1_700_000_000i64;
    let old_time = ref_time - (100 * 86400);

    let meta = EmailMetadataInput {
        uid: 99,
        subject: "Promo flash -50% aujourd'hui seulement",
        received_at_timestamp: old_time,
        is_flagged: true,
        has_unsubscribe: true,
        attachment_names: &[],
        root_domain: None,
        spam_verdict: None,
    };
    let classification = classify_email(&meta, &config, ref_time);
    assert_eq!(
        classification,
        EmailClassification::ProtectedFlagged,
        "Flagged emails must always be ProtectedFlagged"
    );
}

#[test]
fn test_classification_serde_screaming_snake_case() {
    assert_eq!(
        serde_json::to_string(&EmailClassification::Purgeable).unwrap(),
        "\"PURGEABLE\""
    );
    assert_eq!(
        serde_json::to_string(&EmailClassification::ProtectedInvoice).unwrap(),
        "\"PROTECTED_INVOICE\""
    );
    assert_eq!(
        serde_json::to_string(&EmailClassification::ProtectedFlagged).unwrap(),
        "\"PROTECTED_FLAGGED\""
    );
    assert_eq!(
        serde_json::to_string(&EmailClassification::ProtectedPersonal).unwrap(),
        "\"PROTECTED_PERSONAL\""
    );
    assert_eq!(
        serde_json::to_string(&EmailClassification::Recent).unwrap(),
        "\"RECENT\""
    );
}

#[test]
fn test_recent_email_classification() {
    let config = ClassifierConfig::default(); // 60 days
    let ref_time = 1_700_000_000i64;
    let recent_time = ref_time - (10 * 86400); // 10 days old (< 60 days)

    let meta = EmailMetadataInput {
        uid: 101,
        subject: "Newsletter hebdomadaire",
        received_at_timestamp: recent_time,
        is_flagged: false,
        has_unsubscribe: true,
        attachment_names: &[],
        root_domain: None,
        spam_verdict: None,
    };
    let classification = classify_email(&meta, &config, ref_time);
    assert_eq!(
        classification,
        EmailClassification::Recent,
        "Email under retention threshold must be Recent"
    );
}

#[test]
fn test_personal_domain_protection() {
    let config = ClassifierConfig::default();
    let ref_time = 1_700_000_000i64;
    let old_time = ref_time - (120 * 86400); // 120 days old (> 60 days)

    // Old email from orange.fr with normal casual conversation subject
    let orange_meta = EmailMetadataInput {
        uid: 105,
        subject: "Photos du barbecue de dimanche",
        received_at_timestamp: old_time,
        is_flagged: false,
        has_unsubscribe: false,
        attachment_names: &[],
        root_domain: Some("orange.fr"),
        spam_verdict: None,
    };
    assert_eq!(
        classify_email(&orange_meta, &config, ref_time),
        EmailClassification::ProtectedPersonal,
        "Emails from orange.fr must be protected as ProtectedPersonal"
    );

    // Old email from gmail.com
    let gmail_meta = EmailMetadataInput {
        uid: 106,
        subject: "Salut, comment vas-tu depuis le temps ?",
        received_at_timestamp: old_time,
        is_flagged: false,
        has_unsubscribe: false,
        attachment_names: &[],
        root_domain: Some("gmail.com"),
        spam_verdict: None,
    };
    assert_eq!(
        classify_email(&gmail_meta, &config, ref_time),
        EmailClassification::ProtectedPersonal,
        "Emails from gmail.com must be protected as ProtectedPersonal"
    );
}

#[test]
fn test_custom_classifier_config() {
    let custom_config = ClassifierConfig {
        retention_days: 30, // 30 days retention
        transactional_keywords: vec!["licence".to_string()],
        protected_extensions: vec![".zip".to_string()],
    };
    let ref_time = 1_700_000_000i64;
    let email_time = ref_time - (35 * 86400); // 35 days old (> 30 days)

    // Matches custom keyword
    let meta_kw = EmailMetadataInput {
        uid: 102,
        subject: "Votre nouvelle licence logicielle",
        received_at_timestamp: email_time,
        is_flagged: false,
        has_unsubscribe: false,
        attachment_names: &[],
        root_domain: None,
        spam_verdict: None,
    };
    assert_eq!(
        classify_email(&meta_kw, &custom_config, ref_time),
        EmailClassification::ProtectedInvoice
    );

    // Matches custom attachment
    let meta_att = EmailMetadataInput {
        uid: 103,
        subject: "Archives",
        received_at_timestamp: email_time,
        is_flagged: false,
        has_unsubscribe: false,
        attachment_names: &["sources.zip".to_string()],
        root_domain: None,
        spam_verdict: None,
    };
    assert_eq!(
        classify_email(&meta_att, &custom_config, ref_time),
        EmailClassification::ProtectedInvoice
    );
}

#[test]
fn test_4_imap_sequence_formatting_and_chunking() {
    // Formatting sequences: [1, 2, 3, 7, 9, 10] -> "1:3,7,9:10"
    let uids = vec![1, 2, 3, 7, 9, 10];
    let formatted = format_sequence_set(&uids);
    assert_eq!(formatted, "1:3,7,9:10");

    // Unsorted and duplicates handling
    let unsorted = vec![10, 2, 1, 3, 9, 7, 2, 1];
    let formatted_unsorted = format_sequence_set(&unsorted);
    assert_eq!(formatted_unsorted, "1:3,7,9:10");

    // Single elements and empty
    assert_eq!(format_sequence_set(&[]), "");
    assert_eq!(format_sequence_set(&[42]), "42");
    assert_eq!(format_sequence_set(&[1, 2]), "1:2");

    // Chunking: chunk by 3 elements
    let chunks = chunk_sequence_set(&uids, 3);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0], "1:3");
    assert_eq!(chunks[1], "7,9:10");

    // Chunk size larger than total elements
    let single_chunk = chunk_sequence_set(&uids, 50);
    assert_eq!(single_chunk.len(), 1);
    assert_eq!(single_chunk[0], "1:3,7,9:10");

    // Chunk size 1
    let unit_chunks = chunk_sequence_set(&[5, 6, 7], 1);
    assert_eq!(unit_chunks, vec!["5", "6", "7"]);
}

#[test]
fn test_fake_invoice_with_hard_spam_signals_revokes_protection() {
    use mailpurge_core::domain::orphan_spam::SpamVerdictDetail;

    let config = ClassifierConfig::default();
    let ref_time = 1_700_000_000i64;
    let old_time = ref_time - (90 * 86400);

    // 1. Fake invoice on dead domain -> Purgeable
    let dead_verdict = SpamVerdictDetail {
        is_dead_domain: true,
        ..Default::default()
    };
    let meta_dead = EmailMetadataInput {
        uid: 501,
        subject: "Votre facture impayée",
        received_at_timestamp: old_time,
        is_flagged: false,
        has_unsubscribe: false,
        attachment_names: &["facture.pdf".to_string()],
        root_domain: Some("dead-spammer.com"),
        spam_verdict: Some(&dead_verdict),
    };
    assert_eq!(
        classify_email(&meta_dead, &config, ref_time),
        EmailClassification::Purgeable,
        "Dead domain with invoice keyword or PDF must NOT be protected"
    );

    // 2. Fake invoice with SPF failure -> Purgeable
    let spf_fail_verdict = SpamVerdictDetail {
        spf_fail: true,
        ..Default::default()
    };
    let meta_spf = EmailMetadataInput {
        uid: 502,
        subject: "Reçu de votre commande #8921",
        received_at_timestamp: old_time,
        is_flagged: false,
        has_unsubscribe: false,
        attachment_names: &[],
        root_domain: Some("phishing-store.com"),
        spam_verdict: Some(&spf_fail_verdict),
    };
    assert_eq!(
        classify_email(&meta_spf, &config, ref_time),
        EmailClassification::Purgeable,
        "SPF fail with invoice keyword must NOT be protected"
    );

    // 3. Genuine invoice without spam signals -> ProtectedInvoice
    let legit_verdict = SpamVerdictDetail::default();
    let meta_legit = EmailMetadataInput {
        uid: 503,
        subject: "Votre facture #4892",
        received_at_timestamp: old_time,
        is_flagged: false,
        has_unsubscribe: false,
        attachment_names: &["facture.pdf".to_string()],
        root_domain: Some("amazon.fr"),
        spam_verdict: Some(&legit_verdict),
    };
    assert_eq!(
        classify_email(&meta_legit, &config, ref_time),
        EmailClassification::ProtectedInvoice,
        "Legitimate invoice without spam signals must remain ProtectedInvoice"
    );
}
