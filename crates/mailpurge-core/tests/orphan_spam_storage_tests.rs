//! Tests for orphan spam detection, shared domain intelligence, and community rules storage.
#![allow(clippy::assert_is_empty, clippy::too_many_lines)]

use mailpurge_core::classifier::EmailClassification;
use mailpurge_core::domain::community_rules::EspWhitelistEntry;
use mailpurge_core::domain::orphan_spam::{DnsStatus, OrphanCategory};
use mailpurge_core::storage::{EmailRecord, MailStorageRepository};

#[test]
fn test_storage_seeds_default_community_rules() {
    let repo = MailStorageRepository::open_in_memory().expect("open memory db");
    repo.init_schema().expect("init schema");

    let esps = repo.get_esp_whitelist().expect("get esps");
    assert!(!esps.is_empty());
    assert!(esps
        .iter()
        .any(|e| e.root_domain == "sendgrid.net" && e.is_enabled));

    let tlds = repo.get_suspicious_tlds().expect("get tlds");
    assert!(!tlds.is_empty());
    assert!(tlds.iter().any(|t| t.tld == ".xyz" && t.is_enabled));

    assert!(repo
        .is_disposable_domain("yopmail.com")
        .expect("check yopmail"));
    assert!(!repo
        .is_disposable_domain("lemonde.fr")
        .expect("check lemonde"));
}

#[test]
fn test_user_can_customize_and_override_rules() {
    let repo = MailStorageRepository::open_in_memory().expect("open memory db");
    repo.init_schema().expect("init schema");

    // Disable .xyz
    repo.set_tld_enabled(".xyz", false).expect("disable xyz");
    let tlds = repo.get_suspicious_tlds().expect("get tlds");
    let xyz = tlds.iter().find(|t| t.tld == ".xyz").expect("find xyz");
    assert!(!xyz.is_enabled);

    // Add custom enterprise ESP
    let custom_esp = EspWhitelistEntry {
        root_domain: "smtp-internal.company.com".to_string(),
        provider_name: "Internal SMTP Gateway".to_string(),
        is_builtin: false,
        is_user_added: true,
        is_enabled: true,
        updated_at: chrono::Utc::now().timestamp(),
    };
    repo.upsert_esp_whitelist_entry(&custom_esp)
        .expect("upsert custom esp");

    let esps = repo.get_esp_whitelist().expect("get esps");
    assert!(esps
        .iter()
        .any(|e| e.root_domain == "smtp-internal.company.com" && e.is_user_added));
}

#[test]
fn test_shared_domain_intelligence_ttl_and_reuse() {
    let repo = MailStorageRepository::open_in_memory().expect("open memory db");
    repo.init_schema().expect("init schema");

    let statuses = vec![
        (
            "dead-spammer.com".to_string(),
            DnsStatus::DeadNxDomain,
            30 * 86_400,
        ),
        ("live-shop.com".to_string(), DnsStatus::Alive, 7 * 86_400),
    ];
    repo.upsert_shared_domain_dns_batch(&statuses)
        .expect("upsert batch");

    let intel = repo
        .get_shared_domain_intelligence(&[
            "dead-spammer.com".to_string(),
            "live-shop.com".to_string(),
            "unknown.com".to_string(),
        ])
        .expect("get intel");

    assert_eq!(intel.len(), 2);
    let dead = intel.get("dead-spammer.com").expect("dead");
    assert_eq!(dead.dns_status, Some(DnsStatus::DeadNxDomain));
    assert_eq!(dead.dns_ttl_seconds, 30 * 86_400);

    let live = intel.get("live-shop.com").expect("live");
    assert_eq!(live.dns_status, Some(DnsStatus::Alive));
}

#[test]
fn test_get_orphan_summary_categorization() {
    let repo = MailStorageRepository::open_in_memory().expect("open memory db");
    repo.init_schema().expect("init schema");

    let now = chrono::Utc::now().timestamp();

    // Account 1
    repo.save_account(&mailpurge_core::storage::AccountRecord {
        id: 1,
        email: "user@test.com".to_string(),
        provider_type: "imap".to_string(),
        display_name: "Test Acc".to_string(),
        account_name: None,
        server_host: Some("imap.test.com".to_string()),
        server_port: Some(993),
    })
    .expect("save account");

    let emails = vec![
        // Domain 1: dead-spammer.com (1 email) -> will be marked DeadNxDomain in intel
        EmailRecord {
            account_id: 1,
            folder_path: "INBOX".to_string(),
            uid: 101,
            message_id: Some("<m101@dead-spammer.com>".to_string()),
            root_domain: "dead-spammer.com".to_string(),
            full_domain: "dead-spammer.com".to_string(),
            sender_address: "promo@dead-spammer.com".to_string(),
            subject: "Super promo".to_string(),
            received_at: now - 100_000,
            size_bytes: 4096,
            has_unsubscribe: false,
            has_attachment: false,
            attachment_names: Vec::new(),
            is_flagged: false,
            classification: EmailClassification::Purgeable,
            body_snippet: Some("promo snippet".to_string()),
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
            return_path: Some("bounce@dead-spammer.com".to_string()),
            auth_results: None,
            spam_headers: None,
            to_recipients: Some("user@test.com".to_string()),
        },
        // Domain 2: auth-fail-spammer.com (1 email) -> spf=fail
        EmailRecord {
            account_id: 1,
            folder_path: "INBOX".to_string(),
            uid: 102,
            message_id: Some("<m102@auth-fail.com>".to_string()),
            root_domain: "auth-fail-spammer.com".to_string(),
            full_domain: "auth-fail-spammer.com".to_string(),
            sender_address: "contact@auth-fail-spammer.com".to_string(),
            subject: "Fake security alert".to_string(),
            received_at: now - 100_000,
            size_bytes: 5000,
            has_unsubscribe: false,
            has_attachment: false,
            attachment_names: Vec::new(),
            is_flagged: false,
            classification: EmailClassification::Purgeable,
            body_snippet: Some("alert".to_string()),
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
            return_path: Some("bounce@compromised-server.net".to_string()),
            auth_results: Some("spf=fail (sender IP not allowed)".to_string()),
            spam_headers: Some("X-Spam-Flag: YES".to_string()),
            to_recipients: Some("user@test.com".to_string()),
        },
        // Domain 3: disposable yopmail.com (1 email)
        EmailRecord {
            account_id: 1,
            folder_path: "INBOX".to_string(),
            uid: 103,
            message_id: Some("<m103@yopmail.com>".to_string()),
            root_domain: "yopmail.com".to_string(),
            full_domain: "yopmail.com".to_string(),
            sender_address: "temp@yopmail.com".to_string(),
            subject: "One time link".to_string(),
            received_at: now - 100_000,
            size_bytes: 2000,
            has_unsubscribe: false,
            has_attachment: false,
            attachment_names: Vec::new(),
            is_flagged: false,
            classification: EmailClassification::Purgeable,
            body_snippet: None,
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
            return_path: None,
            auth_results: None,
            spam_headers: None,
            to_recipients: None,
        },
        // Domain 4: genuine-merchant.com with protected invoice
        EmailRecord {
            account_id: 1,
            folder_path: "INBOX".to_string(),
            uid: 104,
            message_id: Some("<m104@genuine-merchant.com>".to_string()),
            root_domain: "genuine-merchant.com".to_string(),
            full_domain: "genuine-merchant.com".to_string(),
            sender_address: "facturation@genuine-merchant.com".to_string(),
            subject: "Votre facture #4892".to_string(),
            received_at: now - 100_000,
            size_bytes: 25000,
            has_unsubscribe: false,
            has_attachment: true,
            attachment_names: vec!["facture_4892.pdf".to_string()],
            is_flagged: false,
            classification: EmailClassification::ProtectedInvoice,
            body_snippet: Some("Merci pour votre commande".to_string()),
            list_id: None,
            list_unsubscribe: None,
            list_unsubscribe_post: None,
            return_path: Some("bounce@genuine-merchant.com".to_string()),
            auth_results: Some("spf=pass dkim=pass".to_string()),
            spam_headers: None,
            to_recipients: Some("user@test.com".to_string()),
        },
    ];

    repo.upsert_emails(&emails).expect("upsert emails");

    // Seed intel for dead-spammer.com
    repo.upsert_shared_domain_dns_batch(&[(
        "dead-spammer.com".to_string(),
        DnsStatus::DeadNxDomain,
        30 * 86_400,
    )])
    .expect("upsert dns");

    let summary = repo.get_orphan_summary(Some(1), 3).expect("get summary");
    assert_eq!(summary.threshold, 3);
    assert_eq!(summary.account_id, Some(1));

    // genuine-merchant.com has a genuine protected invoice and no hard spam, so it is NOT in orphan categories
    let dead_group = summary
        .groups
        .iter()
        .find(|g| g.category == OrphanCategory::DeadDomain)
        .expect("dead group");
    assert!(dead_group.domains.contains(&"dead-spammer.com".to_string()));

    let server_group = summary
        .groups
        .iter()
        .find(|g| g.category == OrphanCategory::ServerSpamVerdict)
        .expect("server group");
    assert!(server_group
        .domains
        .contains(&"auth-fail-spammer.com".to_string()));

    let disp_group = summary
        .groups
        .iter()
        .find(|g| g.category == OrphanCategory::DisposableDomain)
        .expect("disposable group");
    assert!(disp_group.domains.contains(&"yopmail.com".to_string()));

    // Test get_orphan_candidates_for_purge
    let candidates = repo
        .get_orphan_candidates_for_purge(1, Some(OrphanCategory::DeadDomain), 3)
        .expect("get candidates");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].1, 101); // uid 101

    // Test tag_orphan_domains
    let tagged_count = repo
        .tag_orphan_domains(Some(1), Some(OrphanCategory::DeadDomain), 3)
        .expect("tag orphan domains");
    assert_eq!(tagged_count, 1);

    // Verify domain stats now has orphan_category
    let stats = repo.get_domain_stats(1).expect("get domain stats");
    let dead_stat = stats
        .iter()
        .find(|s| s.root_domain == "dead-spammer.com")
        .expect("find dead-spammer stat");
    assert_eq!(
        dead_stat.orphan_category.as_deref(),
        Some(OrphanCategory::DeadDomain.as_str())
    );

    // Test unmark_orphan_domain without whitelist
    repo.unmark_orphan_domain(1, "dead-spammer.com", false)
        .expect("unmark orphan domain");
    let stats_after = repo.get_domain_stats(1).expect("get domain stats");
    let dead_stat_after = stats_after
        .iter()
        .find(|s| s.root_domain == "dead-spammer.com")
        .expect("find dead-spammer stat");
    assert_eq!(dead_stat_after.orphan_category, None);

    // Test add_user_trusted_domains
    repo.add_user_trusted_domains(1, &["dead-spammer.com".to_string()], "user_manual")
        .expect("add user trusted domain");
    // Dead spammer now user-trusted and has no server spam flag => should be excluded from orphan summary!
    let summary_trusted = repo
        .get_orphan_summary(Some(1), 3)
        .expect("get summary after trust");
    let dead_group_trusted = summary_trusted
        .groups
        .iter()
        .find(|g| g.category == OrphanCategory::DeadDomain)
        .expect("dead group");
    assert!(!dead_group_trusted
        .domains
        .contains(&"dead-spammer.com".to_string()));
}
