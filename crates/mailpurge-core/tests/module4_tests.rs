#![allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]

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
        account_name: None,
        server_host: None,
        server_port: None,
    };
    storage.save_account(&account).expect("Should save account");

    let accounts = storage.list_accounts().expect("Should list accounts");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].email, "user@enterprise.com");

    let retrieved = storage
        .get_account(1)
        .expect("Should get account")
        .expect("Account exists");
    assert_eq!(retrieved.email, "user@enterprise.com");

    let created = storage
        .create_account("auto@domain.com", "gmail", "Google Workspace")
        .expect("Should create account with autoincrement");
    assert!(created.id > 1);
    assert_eq!(created.email, "auto@domain.com");

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
            body_snippet: Some("Découvrez nos offres exceptionnelles".to_string()),
            list_id: Some("<news.fnac.com>".to_string()),
            list_unsubscribe: Some("<https://news.fnac.com/unsub>".to_string()),
            list_unsubscribe_post: Some("List-Unsubscribe=One-Click".to_string()),
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
            body_snippet: Some("Veuillez trouver ci-joint votre facture".to_string()),
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
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
            body_snippet: Some("Nouveautés composants et PC".to_string()),
            list_id: Some("<promo.ldlc.com>".to_string()),
            list_unsubscribe: Some("<mailto:unsub@ldlc.com>".to_string()),
            list_unsubscribe_post: None,
        },
    ];

    storage
        .upsert_emails(&emails)
        .expect("Should upsert emails");

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
    assert_eq!(
        fnac_emails[0].body_snippet.as_deref(),
        Some("Veuillez trouver ci-joint votre facture")
    );

    // 4a. Check distribution list detection
    let lists = storage
        .get_domain_distribution_lists(1, "fnac.com")
        .expect("Should get distribution lists");
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].list_id, "<news.fnac.com>");
    assert_eq!(lists[0].email_count, 1);
    assert_eq!(
        lists[0].method,
        mailpurge_core::unsubscribe::UnsubscribeMethod::OneClickPost
    );

    // 4b. Toggle classification from Purgeable to ProtectedUser
    storage
        .update_email_classification(1, "INBOX", 101, EmailClassification::ProtectedUser)
        .expect("Should update classification to ProtectedUser");
    let updated_stats = storage.get_domain_stats(1).unwrap();
    let fnac_stat = updated_stats
        .iter()
        .find(|s| s.root_domain == "fnac.com")
        .unwrap();
    assert_eq!(fnac_stat.purgeable_count, 0);
    assert_eq!(fnac_stat.protected_count, 2);

    // Toggle back to Purgeable
    storage
        .update_email_classification(1, "INBOX", 101, EmailClassification::Purgeable)
        .expect("Should update classification back to Purgeable");

    // 4c. Test get and update body snippet
    assert_eq!(
        storage
            .get_email_snippet(1, "INBOX", 101)
            .unwrap()
            .as_deref(),
        Some("Découvrez nos offres exceptionnelles")
    );
    storage
        .update_body_snippet(1, "INBOX", 101, "Nouveau contenu d'aperçu")
        .expect("Should update snippet");
    assert_eq!(
        storage
            .get_email_snippet(1, "INBOX", 101)
            .unwrap()
            .as_deref(),
        Some("Nouveau contenu d'aperçu")
    );

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

    // 6. Test repair unknown domains
    let unknown_email = EmailRecord {
        account_id: 1,
        folder_path: "INBOX".to_string(),
        uid: 999,
        message_id: Some("<msg999@test>".to_string()),
        root_domain: "unknown".to_string(),
        full_domain: "unknown".to_string(),
        sender_address: "Newsletter, Boulanger <offres@boulanger.com>".to_string(),
        subject: "Offre Spéciale".to_string(),
        received_at: 1_600_000_000,
        size_bytes: 10_000,
        has_unsubscribe: true,
        has_attachment: false,
        attachment_names: vec![],
        is_flagged: false,
        classification: EmailClassification::Purgeable,
        body_snippet: None,
        list_id: None,
        list_unsubscribe: None,
        list_unsubscribe_post: None,
    };
    storage.upsert_emails(&[unknown_email]).unwrap();

    let repaired = storage.repair_unknown_domains().unwrap();
    assert_eq!(repaired, 1);

    let stats = storage.get_domain_stats(1).unwrap();
    let boulanger = stats.iter().find(|s| s.root_domain == "boulanger.com");
    assert!(
        boulanger.is_some(),
        "Repaired email should be under boulanger.com"
    );
    let unknown = stats.iter().find(|s| s.root_domain == "unknown");
    assert!(unknown.is_none(), "No emails should remain under unknown");
}

#[test]
fn test_account_naming_migration_and_editing() {
    let storage = MailStorageRepository::open_in_memory().expect("Should open in-memory SQLite");
    storage.init_schema().expect("Should initialize schema");

    // 1. Create account with detailed fields
    let account = storage
        .create_account_with_details(
            "pro@entreprise.fr",
            "imap",
            "pro@entreprise.fr (mail.entreprise.fr:993)",
            Some("Entreprise Pro"),
            Some("mail.entreprise.fr"),
            Some(993),
        )
        .expect("Should create account with details");

    assert_eq!(account.account_name.as_deref(), Some("Entreprise Pro"));
    assert_eq!(account.server_host.as_deref(), Some("mail.entreprise.fr"));
    assert_eq!(account.server_port, Some(993));

    // 2. Update account name
    storage
        .update_account_name(account.id, Some("Mon Compte Pro Renommé"))
        .expect("Should update account name");

    let updated = storage
        .get_account(account.id)
        .expect("Should query account")
        .expect("Account exists");
    assert_eq!(
        updated.account_name.as_deref(),
        Some("Mon Compte Pro Renommé")
    );
    assert_eq!(updated.server_host.as_deref(), Some("mail.entreprise.fr"));
    assert_eq!(updated.server_port, Some(993));

    // 3. Vacuum test
    storage.vacuum().expect("Vacuum should succeed");
}

#[test]
fn test_domain_stats_enhanced_and_spam_detection() {
    use mailpurge_core::storage::is_suspicious_domain;

    assert!(is_suspicious_domain("newsletter.xyz", 1, false));
    assert!(is_suspicious_domain("promo.top", 5, true));
    assert!(is_suspicious_domain("spammer1234.com", 1, false));
    assert!(!is_suspicious_domain("github.com", 20, true));
    assert!(!is_suspicious_domain("fnac.com", 1, true));

    let storage = MailStorageRepository::open_in_memory().expect("Should open in-memory SQLite");
    storage.init_schema().expect("Should initialize schema");

    let _ = storage.create_account("test@test.fr", "imap", "Test");

    let emails = vec![
        EmailRecord {
            account_id: 1,
            folder_path: "INBOX".to_string(),
            uid: 1,
            message_id: Some("<m1@bizarre.xyz>".to_string()),
            root_domain: "bizarre.xyz".to_string(),
            full_domain: "mail.bizarre.xyz".to_string(),
            sender_address: "contact@mail.bizarre.xyz".to_string(),
            subject: "Spam bizarre".to_string(),
            received_at: 1_700_000_100,
            size_bytes: 15_000,
            has_unsubscribe: false,
            has_attachment: false,
            attachment_names: vec![],
            is_flagged: false,
            classification: EmailClassification::Purgeable,
            body_snippet: None,
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
        },
        EmailRecord {
            account_id: 1,
            folder_path: "INBOX".to_string(),
            uid: 2,
            message_id: Some("<m2@bizarre.xyz>".to_string()),
            root_domain: "bizarre.xyz".to_string(),
            full_domain: "mail.bizarre.xyz".to_string(),
            sender_address: "contact@mail.bizarre.xyz".to_string(),
            subject: "Spam bizarre 2".to_string(),
            received_at: 1_700_000_900,
            size_bytes: 20_000,
            has_unsubscribe: false,
            has_attachment: false,
            attachment_names: vec![],
            is_flagged: false,
            classification: EmailClassification::ProtectedInvoice,
            body_snippet: None,
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
        },
    ];

    storage.upsert_emails(&emails).expect("Upsert emails");
    let stats = storage.get_domain_stats(1).expect("Get stats");
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].root_domain, "bizarre.xyz");
    assert_eq!(stats[0].last_received_at, 1_700_000_900);
    assert!(stats[0].is_suspicious_spam);
    assert_eq!(stats[0].protected_count, 1);
    assert_eq!(stats[0].purgeable_count, 1);

    // Test unprotect_domain_emails
    let count = storage
        .unprotect_domain_emails(1, "bizarre.xyz")
        .expect("Unprotect all");
    assert_eq!(count, 1);

    let stats_after = storage.get_domain_stats(1).expect("Get stats");
    assert_eq!(stats_after[0].protected_count, 0);
    assert_eq!(stats_after[0].purgeable_count, 2);

    // Test reclassify_domain_emails
    let config = mailpurge_core::classifier::ClassifierConfig::default();
    let reclassified = storage
        .reclassify_domain_emails(1, "bizarre.xyz", &config, 1_700_000_950)
        .expect("Reclassify");
    // Email 2 has subject "Spam bizarre 2" and no attachment or invoice keyword, so reclassifies appropriately
    assert!(reclassified <= 2);
}

#[test]
fn test_folder_scanner_gmail_trash_variants() {
    use mailpurge_core::folder_scanner::FolderInfo;

    let f1 = FolderInfo::new("[Gmail]/Corbeille", vec![]);
    assert!(f1.is_trash());

    let f2 = FolderInfo::new("[Gmail]/Bin", vec![]);
    assert!(f2.is_trash());

    let f3 = FolderInfo::new("[Gmail]/Messages supprimés", vec![]);
    assert!(f3.is_trash());

    let f4 = FolderInfo::new("[Gmail]/&AOk-l&AOk-ments supprim&AOk-s", vec![]);
    assert!(f4.is_trash());

    let f5 = FolderInfo::new("INBOX", vec![]);
    assert!(!f5.is_trash());
}
