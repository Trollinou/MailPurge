use mailpurge_core::classifier::EmailClassification;
use mailpurge_core::storage::{AccountRecord, EmailRecord, MailStorageRepository};

#[test]
fn test_storage_account_and_email_lifecycle() {
    let storage = MailStorageRepository::open_in_memory().expect("Should open in-memory SQLite");
    storage.init_schema().expect("Should initialize schema");

    // 1. Add account
    let account = AccountRecord {
        id: 1,
        email: "user@enterprise.com".to_string(),
        provider_type: "microsoft365".to_string(),
        display_name: "Pro M365".to_string(),
    };
    storage.save_account(&account).expect("Should save account");

    let accounts = storage.list_accounts().expect("Should list accounts");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].email, "user@enterprise.com");

    // 2. Insert batch of emails from different domains and folders
    let emails = vec![
        EmailRecord {
            account_id: 1,
            folder_path: "INBOX".to_string(),
            uid: 101,
            message_id: Some("<msg101@fnac.com>".to_string()),
            root_domain: "fnac.com".to_string(),
            full_domain: "news.fnac.com".to_string(),
            sender_address: "promo@news.fnac.com".to_string(),
            subject: "Super promo Fnac".to_string(),
            received_at: 1_600_000_000,
            size_bytes: 40_000,
            has_unsubscribe: true,
            has_attachment: false,
            attachment_names: vec![],
            is_flagged: false,
            classification: EmailClassification::Purgeable,
        },
        EmailRecord {
            account_id: 1,
            folder_path: "INBOX".to_string(),
            uid: 102,
            message_id: Some("<msg102@fnac.com>".to_string()),
            root_domain: "fnac.com".to_string(),
            full_domain: "commandes.fnac.com".to_string(),
            sender_address: "facturation@commandes.fnac.com".to_string(),
            subject: "Votre facture Fnac".to_string(),
            received_at: 1_600_000_000,
            size_bytes: 120_000,
            has_unsubscribe: false,
            has_attachment: true,
            attachment_names: vec!["facture.pdf".to_string()],
            is_flagged: false,
            classification: EmailClassification::ProtectedInvoice,
        },
        EmailRecord {
            account_id: 1,
            folder_path: "Archives/2023".to_string(),
            uid: 201,
            message_id: Some("<msg201@ldlc.com>".to_string()),
            root_domain: "ldlc.com".to_string(),
            full_domain: "newsletter.ldlc.com".to_string(),
            sender_address: "info@newsletter.ldlc.com".to_string(),
            subject: "Nouveautés tech".to_string(),
            received_at: 1_590_000_000,
            size_bytes: 50_000,
            has_unsubscribe: true,
            has_attachment: false,
            attachment_names: vec![],
            is_flagged: false,
            classification: EmailClassification::Purgeable,
        },
    ];

    storage.upsert_emails(&emails).expect("Should upsert emails");

    // 3. Check Domain Stats Aggregation
    let stats = storage
        .get_domain_stats(1)
        .expect("Should aggregate domain stats");

    assert_eq!(stats.len(), 2);
    // Ordered by purgeable_size DESC: ldlc.com (50_000) > fnac.com (40_000)
    assert_eq!(stats[0].root_domain, "ldlc.com");
    assert_eq!(stats[0].total_emails, 1);
    assert_eq!(stats[0].purgeable_count, 1);
    assert_eq!(stats[0].purgeable_size, 50_000);
    assert_eq!(stats[0].protected_count, 0);

    assert_eq!(stats[1].root_domain, "fnac.com");
    assert_eq!(stats[1].total_emails, 2);
    assert_eq!(stats[1].total_size, 160_000);
    assert_eq!(stats[1].purgeable_count, 1);
    assert_eq!(stats[1].purgeable_size, 40_000);
    assert_eq!(stats[1].protected_count, 1);

    // 4. Query emails by domain
    let fnac_emails = storage
        .get_emails_by_domain(1, "fnac.com")
        .expect("Should query emails for fnac.com");
    assert_eq!(fnac_emails.len(), 2);

    // 5. Delete purged emails
    storage
        .delete_emails(1, "INBOX", &[101])
        .expect("Should delete email 101");

    let updated_stats = storage.get_domain_stats(1).unwrap();
    let fnac_stat = updated_stats
        .iter()
        .find(|s| s.root_domain == "fnac.com")
        .unwrap();
    assert_eq!(fnac_stat.total_emails, 1);
    assert_eq!(fnac_stat.purgeable_count, 0);
}
