#![allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]

use mailpurge_core::folder_scanner::{
    filter_scannable_folders, find_trash_folder, FolderAttribute, FolderInfo,
};
use mailpurge_core::mock_provider::MockMailProvider;
use mailpurge_core::provider_port::{MailProviderPort, RawEmailEnvelope};

#[test]
fn test_5_filter_folders_exclude_special_keep_custom() {
    let folders = vec![
        FolderInfo::new("INBOX", vec![FolderAttribute::Inbox]),
        FolderInfo::new(
            "INBOX/Achats",
            vec![], // custom user folder
        ),
        FolderInfo::new(
            "Archives/2024",
            vec![FolderAttribute::Archive], // custom archive folder, scannable
        ),
        FolderInfo::new(
            "[Gmail]/Tous les messages",
            vec![FolderAttribute::All], // Gmail All mail -> must be excluded from scan
        ),
        FolderInfo::new(
            "[Gmail]/Corbeille",
            vec![FolderAttribute::Trash], // Trash -> must be excluded from scan
        ),
        FolderInfo::new(
            "[Gmail]/Messages envoyés",
            vec![FolderAttribute::Sent], // Sent -> must be excluded from scan
        ),
        FolderInfo::new(
            "[Gmail]/Brouillons",
            vec![FolderAttribute::Drafts], // Drafts -> must be excluded from scan
        ),
        FolderInfo::new(
            "[Gmail]/Spam",
            vec![FolderAttribute::Junk], // Junk -> must be excluded from scan
        ),
        FolderInfo::new(
            "[Gmail]",
            vec![FolderAttribute::NoSelect], // NoSelect parent -> cannot scan
        ),
    ];

    let scannable = filter_scannable_folders(&folders);
    let scannable_names: Vec<&str> = scannable.iter().map(|f| f.path.as_str()).collect();

    assert_eq!(
        scannable_names,
        vec!["INBOX", "INBOX/Achats", "Archives/2024"]
    );
}

#[test]
fn test_6_identify_trash_folder_rfc6154_and_heuristics() {
    // 1. Gmail with RFC 6154 attribute
    let gmail_folders = vec![
        FolderInfo::new("INBOX", vec![FolderAttribute::Inbox]),
        FolderInfo::new("[Gmail]/Corbeille", vec![FolderAttribute::Trash]),
    ];
    let trash = find_trash_folder(&gmail_folders).expect("Should find Gmail trash");
    assert_eq!(trash.path, "[Gmail]/Corbeille");

    // 2. Microsoft 365 with English name and no attributes (fallback)
    let m365_folders = vec![
        FolderInfo::new("Inbox", vec![]),
        FolderInfo::new("Deleted Items", vec![]),
        FolderInfo::new("Sent Items", vec![]),
    ];
    let trash = find_trash_folder(&m365_folders).expect("Should find M365 trash by heuristic");
    assert_eq!(trash.path, "Deleted Items");

    // 3. French IMAP server with accents and no attributes (fallback)
    let fr_folders = vec![
        FolderInfo::new("Boîte de réception", vec![]),
        FolderInfo::new("Éléments supprimés", vec![]),
    ];
    let trash = find_trash_folder(&fr_folders).expect("Should find French trash by heuristic");
    assert_eq!(trash.path, "Éléments supprimés");

    // 4. Standard "Trash" fallback
    let dovecot_folders = vec![
        FolderInfo::new("INBOX", vec![]),
        FolderInfo::new("Trash", vec![]),
    ];
    let trash = find_trash_folder(&dovecot_folders).expect("Should find standard Trash");
    assert_eq!(trash.path, "Trash");
}

#[tokio::test]
async fn test_mock_provider_lifecycle() {
    let mut provider = MockMailProvider::new();

    // Setup folders
    provider.add_folder(FolderInfo::new("INBOX", vec![FolderAttribute::Inbox]));
    provider.add_folder(FolderInfo::new("Corbeille", vec![FolderAttribute::Trash]));

    // Setup emails in INBOX
    provider.add_email(
        "INBOX",
        RawEmailEnvelope {
            uid: 10,
            message_id: Some("<msg10@domain.com>".to_string()),
            subject: "Promo flash".to_string(),
            from: "news@shop.com".to_string(),
            date_timestamp: 1_600_000_000,
            size_bytes: 54_000,
            is_flagged: false,
            has_unsubscribe: true,
            attachment_names: vec![],
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
            ..Default::default()
        },
    );
    provider.add_email(
        "INBOX",
        RawEmailEnvelope {
            uid: 11,
            message_id: Some("<msg11@domain.com>".to_string()),
            subject: "Votre facture".to_string(),
            from: "compta@shop.com".to_string(),
            date_timestamp: 1_600_000_000,
            size_bytes: 120_000,
            is_flagged: false,
            has_unsubscribe: false,
            attachment_names: vec!["facture.pdf".to_string()],
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
            ..Default::default()
        },
    );

    // List folders
    let folders = provider.list_folders().await.unwrap();
    assert_eq!(folders.len(), 2);

    // Fetch emails
    let emails = provider.fetch_envelopes("INBOX").await.unwrap();
    assert_eq!(emails.len(), 2);

    // Move UID 10 to trash
    let moved_count = provider
        .move_to_trash("INBOX", &[10], "Corbeille")
        .await
        .unwrap();
    assert_eq!(moved_count, 1);

    // INBOX should now have only 1 email
    assert_eq!(provider.fetch_envelopes("INBOX").await.unwrap().len(), 1);
    // Corbeille should have 1 email
    assert_eq!(
        provider.fetch_envelopes("Corbeille").await.unwrap().len(),
        1
    );

    // Empty trash
    let purged_count = provider.empty_trash("Corbeille").await.unwrap();
    assert_eq!(purged_count, 1);
    assert_eq!(
        provider.fetch_envelopes("Corbeille").await.unwrap().len(),
        0
    );
}

#[tokio::test]
async fn test_folder_count_and_range_fetching() {
    let mut provider = MockMailProvider::new();
    provider.add_folder(FolderInfo::new("INBOX", vec![FolderAttribute::Inbox]));

    for uid in 1..=10 {
        provider.add_email(
            "INBOX",
            RawEmailEnvelope {
                uid,
                message_id: Some(format!("<msg{uid}@domain.com>")),
                subject: format!("Test {uid}"),
                from: "sender@domain.com".to_string(),
                date_timestamp: 1_600_000_000,
                size_bytes: 1000,
                is_flagged: false,
                has_unsubscribe: false,
                attachment_names: vec![],
                list_id: None,
                list_unsubscribe: None,
                list_unsubscribe_post: None,
                ..Default::default()
            },
        );
    }

    // Check count
    let count = provider.get_folder_message_count("INBOX").await.unwrap();
    assert_eq!(count, 10);

    // Range 1..=3
    let chunk1 = provider.fetch_envelopes_range("INBOX", 1, 3).await.unwrap();
    assert_eq!(chunk1.len(), 3);
    assert_eq!(chunk1[0].uid, 1);
    assert_eq!(chunk1[2].uid, 3);

    // Range 8..=12 (should clamp to 10)
    let chunk2 = provider
        .fetch_envelopes_range("INBOX", 8, 12)
        .await
        .unwrap();
    assert_eq!(chunk2.len(), 3);
    assert_eq!(chunk2[0].uid, 8);
    assert_eq!(chunk2[2].uid, 10);

    // Out of bounds
    let empty_chunk = provider
        .fetch_envelopes_range("INBOX", 20, 30)
        .await
        .unwrap();
    assert!(empty_chunk.is_empty());
}
