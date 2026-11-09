use std::fs;
use std::sync::Mutex;
use tauri::State;

use mailpurge_core::classifier::{classify_email, ClassifierConfig, EmailMetadataInput};
use mailpurge_core::domain_extractor::extract_sender_domain;
use mailpurge_core::mock_provider::MockMailProvider;
use mailpurge_core::provider_port::MailProviderPort;
use mailpurge_core::purge_executor::{PurgeCandidate, PurgeConfig, PurgeExecutor, PurgeReport};
use mailpurge_core::storage::{
    AccountRecord, DomainStats, EmailRecord, MailStorageRepository,
};

pub struct AppState {
    pub storage: MailStorageRepository,
    pub mock_provider: Mutex<MockMailProvider>,
}

#[tauri::command]
fn get_accounts(state: State<AppState>) -> Result<Vec<AccountRecord>, String> {
    state
        .storage
        .list_accounts()
        .map_err(|e| format!("Erreur liste des comptes: {e}"))
}

#[tauri::command]
fn get_domain_stats(
    state: State<AppState>,
    account_id: i64,
) -> Result<Vec<DomainStats>, String> {
    state
        .storage
        .get_domain_stats(account_id)
        .map_err(|e| format!("Erreur statistiques domaines: {e}"))
}

#[tauri::command]
fn get_emails_by_domain(
    state: State<AppState>,
    account_id: i64,
    root_domain: String,
) -> Result<Vec<EmailRecord>, String> {
    state
        .storage
        .get_emails_by_domain(account_id, &root_domain)
        .map_err(|e| format!("Erreur récupération des emails: {e}"))
}

#[tauri::command]
fn execute_purge(
    state: State<AppState>,
    account_id: i64,
    root_domain: String,
    uids: Vec<u32>,
    dry_run: bool,
) -> Result<PurgeReport, String> {
    // 1. Fetch domain emails from storage
    let emails = state
        .storage
        .get_emails_by_domain(account_id, &root_domain)
        .map_err(|e| format!("Erreur lecture emails: {e}"))?;

    // 2. Filter candidates matching requested UIDs
    let candidates: Vec<PurgeCandidate> = emails
        .into_iter()
        .filter(|e| uids.contains(&e.uid))
        .map(|e| PurgeCandidate {
            folder_path: e.folder_path,
            uid: e.uid,
            size_bytes: e.size_bytes,
            classification: e.classification,
        })
        .collect();

    let config = PurgeConfig {
        batch_size: 500,
        dry_run,
    };

    let mut provider = state.mock_provider.lock().unwrap();
    let report = PurgeExecutor::execute(&mut *provider, &candidates, "Corbeille", &config)
        .map_err(|e| format!("Erreur exécution de purge: {e}"))?;

    // 3. If real execution succeeded, delete purged emails from storage
    if !dry_run {
        for (folder, purged_uids) in &report.details_by_folder {
            state
                .storage
                .delete_emails(account_id, folder, purged_uids)
                .map_err(|e| format!("Erreur mise à jour cache SQLite: {e}"))?;
        }
    }

    Ok(report)
}

#[tauri::command]
fn empty_trash(state: State<AppState>, _account_id: i64) -> Result<usize, String> {
    let mut provider = state.mock_provider.lock().unwrap();
    provider
        .empty_trash("Corbeille")
        .map_err(|e| format!("Erreur vidage corbeille: {e}"))
}

#[tauri::command]
fn load_demo_data(state: State<AppState>) -> Result<AccountRecord, String> {
    let account = AccountRecord {
        id: 1,
        email: "etienne@entreprise-m365.fr".to_string(),
        provider_type: "microsoft365".to_string(),
        display_name: "Microsoft 365 Pro (Etienne)".to_string(),
    };

    state
        .storage
        .save_account(&account)
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().timestamp();
    let config = ClassifierConfig::default();

    struct DemoItem {
        from: &'static str,
        subject: &'static str,
        date_ts: i64,
        size_bytes: u64,
        has_unsub: bool,
        has_att: bool,
        atts: Vec<String>,
        is_flagged: bool,
        folder: &'static str,
    }

    let samples = vec![
        // FNAC (Purgeable newsletters & protected invoice)
        DemoItem {
            from: "no-reply@news.fnac.com",
            subject: "Grandes Promos du Week-end : jusqu'à -40%",
            date_ts: now - (75 * 86400),
            size_bytes: 850_000,
            has_unsub: true,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },
        DemoItem {
            from: "no-reply@news.fnac.com",
            subject: "Dernières heures pour en profiter !",
            date_ts: now - (80 * 86400),
            size_bytes: 720_000,
            has_unsub: true,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },
        DemoItem {
            from: "facturation@commandes.fnac.com",
            subject: "Votre facture pour la commande 98234",
            date_ts: now - (90 * 86400),
            size_bytes: 240_000,
            has_unsub: false,
            has_att: true,
            atts: vec!["facture_fnac_98234.pdf".to_string()],
            is_flagged: false,
            folder: "Achats/2023",
        },
        DemoItem {
            from: "sav@fnac.com",
            subject: "Certificat de garantie de votre matériel",
            date_ts: now - (120 * 86400),
            size_bytes: 180_000,
            has_unsub: false,
            has_att: true,
            atts: vec!["garantie.pdf".to_string()],
            is_flagged: false,
            folder: "Achats/2023",
        },

        // LDLC (Tech newsletters & protected order)
        DemoItem {
            from: "info@newsletter.ldlc.com",
            subject: "Nouveaux processeurs et cartes graphiques en stock",
            date_ts: now - (65 * 86400),
            size_bytes: 1_250_000,
            has_unsub: true,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },
        DemoItem {
            from: "info@newsletter.ldlc.com",
            subject: "Config PC Gamer du mois",
            date_ts: now - (70 * 86400),
            size_bytes: 980_000,
            has_unsub: true,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },
        DemoItem {
            from: "commandes@ldlc.com",
            subject: "Confirmation de votre commande LDLC #4410",
            date_ts: now - (100 * 86400),
            size_bytes: 150_000,
            has_unsub: false,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },

        // AMAZON (Large invoices & daily marketing)
        DemoItem {
            from: "auto-confirm@amazon.co.uk",
            subject: "Your Amazon.co.uk order confirmation",
            date_ts: now - (85 * 86400),
            size_bytes: 110_000,
            has_unsub: false,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },
        DemoItem {
            from: "vendeur-facture@service-client.amazon.co.uk",
            subject: "Facture avec TVA pour commande 402-123",
            date_ts: now - (95 * 86400),
            size_bytes: 320_000,
            has_unsub: false,
            has_att: true,
            atts: vec!["invoice.pdf".to_string()],
            is_flagged: false,
            folder: "Achats/2023",
        },
        DemoItem {
            from: "marketing@promo.amazon.co.uk",
            subject: "Lightning deals on Electronics",
            date_ts: now - (72 * 86400),
            size_bytes: 1_850_000,
            has_unsub: true,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },

        // BOULANGER
        DemoItem {
            from: "contact@news.boulanger.fr",
            subject: "Les soldes continuent chez Boulanger",
            date_ts: now - (68 * 86400),
            size_bytes: 910_000,
            has_unsub: true,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },
        DemoItem {
            from: "service-client@commandes.boulanger.fr",
            subject: "Votre reçu de paiement en magasin",
            date_ts: now - (110 * 86400),
            size_bytes: 210_000,
            has_unsub: false,
            has_att: true,
            atts: vec!["recu_boulanger.pdf".to_string()],
            is_flagged: false,
            folder: "INBOX",
        },

        // LINKEDIN (Voluminous notifications)
        DemoItem {
            from: "updates@linkedin.com",
            subject: "Vous avez 12 nouvelles notifications cette semaine",
            date_ts: now - (62 * 86400),
            size_bytes: 450_000,
            has_unsub: true,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },
        DemoItem {
            from: "messages@linkedin.com",
            subject: "Nouveau message de votre réseau",
            date_ts: now - (66 * 86400),
            size_bytes: 310_000,
            has_unsub: true,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },
        DemoItem {
            from: "jobs@linkedin.com",
            subject: "Nouvelles offres d'emploi pour Développeur Rust",
            date_ts: now - (64 * 86400),
            size_bytes: 620_000,
            has_unsub: true,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
        },

        // UBER (Flagged & recent items)
        DemoItem {
            from: "receipts@uber.com",
            subject: "Votre reçu Uber pour votre course",
            date_ts: now - (12 * 86400),
            size_bytes: 140_000,
            has_unsub: false,
            has_att: true,
            atts: vec!["uber_receipt.pdf".to_string()],
            is_flagged: false,
            folder: "INBOX",
        },
        DemoItem {
            from: "receipts@uber.com",
            subject: "Course du 15 mai avec pourboire",
            date_ts: now - (180 * 86400),
            size_bytes: 130_000,
            has_unsub: false,
            has_att: false,
            atts: vec![],
            is_flagged: true,
            folder: "INBOX",
        },

        // OVHCLOUD (Protected contract)
        DemoItem {
            from: "billing@ovhcloud.com",
            subject: "Votre contrat de renouvellement de nom de domaine",
            date_ts: now - (130 * 86400),
            size_bytes: 290_000,
            has_unsub: false,
            has_att: true,
            atts: vec!["contrat_ovh.pdf".to_string()],
            is_flagged: false,
            folder: "Achats/2023",
        },
    ];

    let mut email_records = Vec::new();
    let mut mock_provider = state.mock_provider.lock().unwrap();

    for (i, item) in samples.into_iter().enumerate() {
        let uid = (i + 1) as u32;
        let sender = extract_sender_domain(item.from).unwrap_or_else(|_| {
            mailpurge_core::domain_extractor::ExtractedSender {
                raw_from: item.from.to_string(),
                display_name: None,
                email_address: item.from.to_string(),
                full_domain: "inconnu.com".to_string(),
                root_domain: "inconnu.com".to_string(),
            }
        });

        let meta = EmailMetadataInput {
            uid,
            subject: item.subject,
            received_at_timestamp: item.date_ts,
            is_flagged: item.is_flagged,
            has_unsubscribe: item.has_unsub,
            attachment_names: &item.atts,
        };

        let classification = classify_email(&meta, &config, now);

        email_records.push(EmailRecord {
            account_id: 1,
            folder_path: item.folder.to_string(),
            uid,
            message_id: Some(format!("<demo_{uid}@mailpurge>")),
            root_domain: sender.root_domain,
            full_domain: sender.full_domain,
            sender_address: sender.email_address.clone(),
            subject: item.subject.to_string(),
            received_at: item.date_ts,
            size_bytes: item.size_bytes,
            has_unsubscribe: item.has_unsub,
            has_attachment: item.has_att,
            attachment_names: item.atts.clone(),
            is_flagged: item.is_flagged,
            classification,
        });

        mock_provider.add_email(
            item.folder,
            mailpurge_core::provider_port::RawEmailEnvelope {
                uid,
                message_id: Some(format!("<demo_{uid}@mailpurge>")),
                subject: item.subject.to_string(),
                from: sender.email_address,
                date_timestamp: item.date_ts,
                size_bytes: item.size_bytes,
                is_flagged: item.is_flagged,
                has_unsubscribe: item.has_unsub,
                attachment_names: item.atts,
            },
        );
    }

    state
        .storage
        .upsert_emails(&email_records)
        .map_err(|e| format!("Erreur insertion démo: {e}"))?;

    Ok(account)
}

pub fn run() {
    // Choose local storage directory
    let db_path = if let Some(mut data_dir) = dirs::data_local_dir() {
        data_dir.push("mailpurge");
        let _ = fs::create_dir_all(&data_dir);
        data_dir.push("mailpurge.db");
        data_dir
    } else {
        std::path::PathBuf::from("mailpurge.db")
    };

    let storage = MailStorageRepository::open(&db_path)
        .or_else(|_| MailStorageRepository::open_in_memory())
        .expect("Impossible d'initialiser SQLite");
    let _ = storage.init_schema();

    let app_state = AppState {
        storage,
        mock_provider: Mutex::new(MockMailProvider::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_accounts,
            get_domain_stats,
            get_emails_by_domain,
            execute_purge,
            empty_trash,
            load_demo_data
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
