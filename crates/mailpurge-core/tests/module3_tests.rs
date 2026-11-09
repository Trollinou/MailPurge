#![allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]

use mailpurge_core::classifier::EmailClassification;
use mailpurge_core::mock_provider::MockMailProvider;
use mailpurge_core::provider_port::{MailProviderPort, RawEmailEnvelope};
use mailpurge_core::purge_executor::{PurgeCandidate, PurgeConfig, PurgeExecutor};

#[tokio::test]
async fn test_7_never_purge_protected_or_recent_emails() {
    let mut provider = MockMailProvider::new();
    let trash_folder = "Corbeille";

    // Prepare 3 emails in INBOX:
    // - UID 1: Purgeable newsletter
    // - UID 2: Protected Invoice (MUST NOT BE PURGED)
    // - UID 3: Protected Flagged (MUST NOT BE PURGED)
    // - UID 4: Recent (MUST NOT BE PURGED)
    let candidates = vec![
        PurgeCandidate {
            folder_path: "INBOX".to_string(),
            uid: 1,
            size_bytes: 50_000,
            classification: EmailClassification::Purgeable,
        },
        PurgeCandidate {
            folder_path: "INBOX".to_string(),
            uid: 2,
            size_bytes: 120_000,
            classification: EmailClassification::ProtectedInvoice,
        },
        PurgeCandidate {
            folder_path: "INBOX".to_string(),
            uid: 3,
            size_bytes: 30_000,
            classification: EmailClassification::ProtectedFlagged,
        },
        PurgeCandidate {
            folder_path: "INBOX".to_string(),
            uid: 4,
            size_bytes: 15_000,
            classification: EmailClassification::Recent,
        },
    ];

    for c in &candidates {
        provider.add_email(
            &c.folder_path,
            RawEmailEnvelope {
                uid: c.uid,
                message_id: None,
                subject: format!("Mail {}", c.uid),
                from: "sender@domain.com".to_string(),
                date_timestamp: 1_600_000_000,
                size_bytes: c.size_bytes,
                is_flagged: c.classification == EmailClassification::ProtectedFlagged,
                has_unsubscribe: false,
                attachment_names: vec![],
                list_id: None,
                list_unsubscribe: None,
                list_unsubscribe_post: None,
            },
        );
    }

    let config = PurgeConfig {
        batch_size: 500,
        dry_run: false,
    };

    let report = PurgeExecutor::execute(&mut provider, &candidates, trash_folder, &config)
        .await
        .expect("Purge execution should succeed");

    assert_eq!(report.total_candidates, 4);
    assert_eq!(report.purged_count, 1);
    assert_eq!(report.purged_bytes, 50_000);
    assert_eq!(report.skipped_protected_count, 3);

    // Verify provider state: only UID 1 moved to Corbeille
    let inbox_emails = provider.fetch_envelopes("INBOX").await.unwrap();
    let inbox_uids: Vec<u32> = inbox_emails.iter().map(|e| e.uid).collect();
    assert_eq!(inbox_uids, vec![2, 3, 4]);

    let trash_emails = provider.fetch_envelopes(trash_folder).await.unwrap();
    assert_eq!(trash_emails.len(), 1);
    assert_eq!(trash_emails[0].uid, 1);
}

#[tokio::test]
async fn test_8_batch_chunking_with_limit_500() {
    let mut provider = MockMailProvider::new();
    let trash_folder = "Trash";

    // Create 1250 purgeable candidates in INBOX
    let count = 1250;
    let mut candidates = Vec::with_capacity(count);

    for i in 1..=count as u32 {
        candidates.push(PurgeCandidate {
            folder_path: "INBOX".to_string(),
            uid: i,
            size_bytes: 1_000,
            classification: EmailClassification::Purgeable,
        });
        provider.add_email(
            "INBOX",
            RawEmailEnvelope {
                uid: i,
                message_id: None,
                subject: format!("Mail {}", i),
                from: "bulk@domain.com".to_string(),
                date_timestamp: 1_600_000_000,
                size_bytes: 1_000,
                is_flagged: false,
                has_unsubscribe: true,
                attachment_names: vec![],
                list_id: None,
                list_unsubscribe: None,
                list_unsubscribe_post: None,
            },
        );
    }

    let config = PurgeConfig {
        batch_size: 500,
        dry_run: false,
    };

    let report = PurgeExecutor::execute(&mut provider, &candidates, trash_folder, &config)
        .await
        .expect("Bulk purge should succeed");

    assert_eq!(report.purged_count, 1250);
    assert_eq!(report.batches_executed, 3); // 500 + 500 + 250
    assert_eq!(report.skipped_protected_count, 0);

    // Verify operations log recorded exactly 3 chunk operations
    assert_eq!(provider.operations_log.len(), 3);
}

#[tokio::test]
async fn test_dry_run_mode_leaves_server_untouched() {
    let mut provider = MockMailProvider::new();
    let trash_folder = "Corbeille";

    let candidates = vec![PurgeCandidate {
        folder_path: "INBOX".to_string(),
        uid: 10,
        size_bytes: 40_000,
        classification: EmailClassification::Purgeable,
    }];

    provider.add_email(
        "INBOX",
        RawEmailEnvelope {
            uid: 10,
            message_id: None,
            subject: "Dry run test".to_string(),
            from: "test@domain.com".to_string(),
            date_timestamp: 1_600_000_000,
            size_bytes: 40_000,
            is_flagged: false,
            has_unsubscribe: false,
            attachment_names: vec![],
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
        },
    );

    let config = PurgeConfig {
        batch_size: 500,
        dry_run: true, // DRY RUN ON
    };

    let report = PurgeExecutor::execute(&mut provider, &candidates, trash_folder, &config)
        .await
        .expect("Dry run should succeed");

    assert!(report.is_dry_run);
    assert_eq!(report.purged_count, 1);
    assert_eq!(report.purged_bytes, 40_000);

    // Provider state must be 100% untouched
    assert_eq!(provider.operations_log.len(), 0);
    assert_eq!(provider.fetch_envelopes("INBOX").await.unwrap().len(), 1);
    assert_eq!(
        provider
            .fetch_envelopes(trash_folder)
            .await
            .unwrap_or_default()
            .len(),
        0
    );
}
