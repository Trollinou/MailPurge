use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, State};

mod sync_engine;

use mailpurge_core::auth::{
    build_authorization_url, exchange_code_for_tokens, extract_email_from_id_token, generate_pkce,
    listen_for_oauth_code, KeyringStore, OAuthProvider,
};

use mailpurge_core::classifier::{
    classify_email, ClassifierConfig, EmailClassification, EmailMetadataInput,
};
use mailpurge_core::domain_extractor::extract_sender_domain;
use mailpurge_core::mock_provider::MockMailProvider;
use mailpurge_core::provider_port::MailProviderPort;
use mailpurge_core::purge_executor::{PurgeCandidate, PurgeConfig, PurgeExecutor, PurgeReport};
use mailpurge_core::storage::{
    AccountRecord, DistributionListGroup, DomainStats, EmailRecord, MailStorageRepository,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnsubscribeReport {
    pub message: String,
    pub emails_purged: usize,
    pub bytes_freed: u64,
}

pub struct AppState {
    pub storage: MailStorageRepository,
    pub mock_provider: Mutex<MockMailProvider>,
    pub secret_cache: Arc<Mutex<HashMap<String, String>>>,
}

fn get_trash_folder_path(provider: &dyn MailProviderPort) -> String {
    match provider.list_folders() {
        Ok(folders) => mailpurge_core::folder_scanner::find_trash_folder(&folders)
            .map(|f| f.path.clone())
            .unwrap_or_else(|| "Trash".to_string()),
        Err(e) => {
            eprintln!("[IMAP] Impossible de lister les dossiers pour la corbeille ({e}), repli sur 'Trash'");
            "Trash".to_string()
        }
    }
}

fn resolve_imap_config(
    account: &AccountRecord,
    secret_cache: &Mutex<HashMap<String, String>>,
) -> Result<mailpurge_core::imap_provider::ImapConfig, String> {
    let secret = {
        let mut cache = secret_cache
            .lock()
            .map_err(|_| "Verrouillage cache secrets échoué".to_string())?;
        if let Some(s) = cache.get(&account.email) {
            s.clone()
        } else {
            let s = KeyringStore::get_secret(&account.email).map_err(|e| {
                format!("Impossible de lire les identifiants dans le trousseau: {e}")
            })?;
            cache.insert(account.email.clone(), s.clone());
            s
        }
    };

    let (server, port, auth) = match account.provider_type.as_str() {
        "google_oauth" | "google" => {
            let mut access_token = secret.clone();
            if let Ok(refreshed) = mailpurge_core::auth::refresh_access_token(
                &OAuthProvider::Google,
                "812345678901-placeholder.apps.googleusercontent.com",
                None,
                &secret,
            ) {
                access_token = refreshed.access_token;
            }
            (
                "imap.gmail.com".to_string(),
                993,
                mailpurge_core::imap_provider::ImapAuthMethod::XOAuth2 { access_token },
            )
        }
        "gmail" | "google_app_password" => (
            "imap.gmail.com".to_string(),
            993,
            mailpurge_core::imap_provider::ImapAuthMethod::Password(secret),
        ),
        "microsoft365" | "m365_oauth" => {
            let mut access_token = secret.clone();
            if let Ok(refreshed) = mailpurge_core::auth::refresh_access_token(
                &OAuthProvider::Microsoft365,
                "d3590ed6-52b3-4102-aeff-aad2292ab01c",
                None,
                &secret,
            ) {
                access_token = refreshed.access_token;
            }
            (
                "outlook.office365.com".to_string(),
                993,
                mailpurge_core::imap_provider::ImapAuthMethod::XOAuth2 { access_token },
            )
        }
        "outlook" | "outlook_app_password" | "hotmail" => (
            "outlook.office365.com".to_string(),
            993,
            mailpurge_core::imap_provider::ImapAuthMethod::Password(secret),
        ),
        "icloud" => (
            "imap.mail.me.com".to_string(),
            993,
            mailpurge_core::imap_provider::ImapAuthMethod::Password(secret),
        ),
        _ => {
            let mut srv = "imap.mail.me.com".to_string();
            let mut prt = 993;
            if let Some(start) = account.display_name.find('(') {
                if let Some(end) = account.display_name.rfind(')') {
                    let inside = &account.display_name[start + 1..end];
                    if let Some((h, p)) = inside.split_once(':') {
                        srv = h.trim().to_string();
                        if let Ok(parsed_p) = p.trim().parse::<u16>() {
                            prt = parsed_p;
                        }
                    }
                }
            }
            (
                srv,
                prt,
                mailpurge_core::imap_provider::ImapAuthMethod::Password(secret),
            )
        }
    };

    Ok(mailpurge_core::imap_provider::ImapConfig {
        server,
        port,
        username: account.email.clone(),
        auth,
    })
}

#[tauri::command]
fn get_accounts(state: State<AppState>) -> Result<Vec<AccountRecord>, String> {
    state
        .storage
        .list_accounts()
        .map_err(|e| format!("Erreur liste des comptes: {e}"))
}

#[tauri::command]
fn delete_account(state: State<AppState>, account_id: i64) -> Result<(), String> {
    if let Ok(Some(account)) = state.storage.get_account(account_id) {
        let _ = KeyringStore::delete_secret(&account.email);
        if let Ok(mut cache) = state.secret_cache.lock() {
            cache.remove(&account.email);
        }
    }
    state
        .storage
        .delete_account(account_id)
        .map_err(|e| format!("Erreur suppression compte: {e}"))
}

#[tauri::command]
fn update_account_password(
    state: State<AppState>,
    account_id: i64,
    password: String,
) -> Result<(), String> {
    let account = state
        .storage
        .get_account(account_id)
        .map_err(|e| format!("Erreur lecture compte: {e}"))?
        .ok_or_else(|| format!("Compte introuvable: {account_id}"))?;

    // Validate password with IMAP server before saving
    let mut srv = "imap.free.fr".to_string();
    let mut prt = 993;
    if let Some(start) = account.display_name.find('(') {
        if let Some(end) = account.display_name.rfind(')') {
            let inside = &account.display_name[start + 1..end];
            if let Some((h, p)) = inside.split_once(':') {
                srv = h.trim().to_string();
                if let Ok(parsed_p) = p.trim().parse::<u16>() {
                    prt = parsed_p;
                }
            } else {
                srv = inside.trim().to_string();
            }
        }
    }

    let imap_config = mailpurge_core::imap_provider::ImapConfig {
        server: srv,
        port: prt,
        username: account.email.clone(),
        auth: mailpurge_core::imap_provider::ImapAuthMethod::Password(password.clone()),
    };

    let test_provider = mailpurge_core::imap_provider::ImapProvider::new(imap_config);
    let mut session = test_provider
        .connect_session()
        .map_err(|e| format!("Échec d'authentification avec ce mot de passe: {e}"))?;
    let _ = session.logout();

    KeyringStore::save_secret(&account.email, &password)
        .map_err(|e| format!("Erreur trousseau système: {e}"))?;

    if let Ok(mut cache) = state.secret_cache.lock() {
        cache.insert(account.email, password);
    }

    Ok(())
}

#[tauri::command]
fn get_domain_stats(state: State<AppState>, account_id: i64) -> Result<Vec<DomainStats>, String> {
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
fn fetch_email_preview(
    state: State<AppState>,
    account_id: i64,
    folder_path: String,
    uid: u32,
) -> Result<String, String> {
    // 1. Vérifier si l'aperçu est déjà présent dans le cache SQLite local (et valide, sans CSS résiduel)
    if let Ok(Some(cached)) = state
        .storage
        .get_email_snippet(account_id, &folder_path, uid)
    {
        let trimmed = cached.trim();
        let is_corrupted = trimmed.starts_with("<!DOCTYPE")
            || trimmed.starts_with("<html")
            || trimmed.starts_with("<head")
            || trimmed.starts_with("<body")
            || trimmed.starts_with("<style")
            || trimmed.starts_with("<!--")
            || trimmed.starts_with("@media")
            || trimmed.contains("{font-family")
            || trimmed.contains("!important");
        if !trimmed.is_empty() && !is_corrupted {
            return Ok(cached);
        }
    }

    let account = state
        .storage
        .get_account(account_id)
        .map_err(|e| format!("Erreur compte: {e}"))?
        .ok_or_else(|| format!("Compte introuvable: {account_id}"))?;

    let snippet = if account.id == 1 && account.email == "etienne@entreprise-m365.fr" {
        let mut provider = state
            .mock_provider
            .lock()
            .map_err(|_| "Verrouillage mock échoué".to_string())?;
        provider
            .fetch_body_snippet(&folder_path, uid)
            .unwrap_or_default()
    } else {
        let imap_config = resolve_imap_config(&account, &state.secret_cache)?;
        let mut provider = mailpurge_core::imap_provider::ImapProvider::new(imap_config);
        provider
            .fetch_body_snippet(&folder_path, uid)
            .map_err(|e| format!("Erreur chargement aperçu: {e}"))?
    };

    if !snippet.is_empty() {
        let _ = state
            .storage
            .update_body_snippet(account_id, &folder_path, uid, &snippet);
    }

    Ok(snippet)
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

    let account = state
        .storage
        .get_account(account_id)
        .map_err(|e| format!("Erreur compte: {e}"))?
        .ok_or_else(|| format!("Compte introuvable: {account_id}"))?;

    let report = if account.id == 1 && account.email == "etienne@entreprise-m365.fr" {
        let mut provider = state
            .mock_provider
            .lock()
            .map_err(|_| "Verrouillage provider échoué".to_string())?;
        PurgeExecutor::execute(&mut *provider, &candidates, "Corbeille", &config)
            .map_err(|e| format!("Erreur exécution de purge: {e}"))?
    } else {
        let imap_config = resolve_imap_config(&account, &state.secret_cache)?;
        let mut provider = mailpurge_core::imap_provider::ImapProvider::new(imap_config);
        let trash_folder = get_trash_folder_path(&provider);
        eprintln!("[Purge] Dossier corbeille identifié : '{trash_folder}'");
        PurgeExecutor::execute(&mut provider, &candidates, &trash_folder, &config)
            .map_err(|e| format!("Erreur exécution de purge: {e}"))?
    };

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
fn update_email_classification(
    state: State<AppState>,
    account_id: i64,
    folder_path: String,
    uid: u32,
    new_classification: String,
) -> Result<(), String> {
    let classification = match new_classification.as_str() {
        "PURGEABLE" => EmailClassification::Purgeable,
        "PROTECTED_INVOICE" => EmailClassification::ProtectedInvoice,
        "PROTECTED_FLAGGED" => EmailClassification::ProtectedFlagged,
        "PROTECTED_USER" => EmailClassification::ProtectedUser,
        "PROTECTED_PERSONAL" => EmailClassification::ProtectedPersonal,
        "RECENT" => EmailClassification::Recent,
        _ => return Err(format!("Classification inconnue: {new_classification}")),
    };

    state
        .storage
        .update_email_classification(account_id, &folder_path, uid, classification)
        .map_err(|e| format!("Erreur mise à jour classification: {e}"))
}

#[tauri::command]
fn empty_trash(state: State<AppState>, account_id: i64) -> Result<usize, String> {
    let account = state
        .storage
        .get_account(account_id)
        .map_err(|e| format!("Erreur compte: {e}"))?
        .ok_or_else(|| format!("Compte introuvable: {account_id}"))?;

    if account.id == 1 && account.email == "etienne@entreprise-m365.fr" {
        let mut provider = state
            .mock_provider
            .lock()
            .map_err(|_| "Verrouillage provider échoué".to_string())?;
        provider
            .empty_trash("Corbeille")
            .map_err(|e| format!("Erreur vidage corbeille: {e}"))
    } else {
        let imap_config = resolve_imap_config(&account, &state.secret_cache)?;
        let mut provider = mailpurge_core::imap_provider::ImapProvider::new(imap_config);
        let trash_folder = get_trash_folder_path(&provider);
        eprintln!("[EmptyTrash] Dossier corbeille identifié : '{trash_folder}'");
        provider
            .empty_trash(&trash_folder)
            .map_err(|e| format!("Erreur vidage corbeille: {e}"))
    }
}

#[tauri::command]
fn get_distribution_lists(
    state: State<AppState>,
    account_id: i64,
    root_domain: String,
) -> Result<Vec<DistributionListGroup>, String> {
    state
        .storage
        .get_domain_distribution_lists(account_id, &root_domain)
        .map_err(|e| format!("Erreur listes de diffusion: {e}"))
}

#[tauri::command]
fn unsubscribe_and_purge(
    state: State<AppState>,
    account_id: i64,
    root_domain: String,
    list_id: String,
    move_to_trash: bool,
    dry_run: bool,
) -> Result<UnsubscribeReport, String> {
    let lists = state
        .storage
        .get_domain_distribution_lists(account_id, &root_domain)
        .map_err(|e| format!("Erreur récupération listes: {e}"))?;

    let group = lists
        .into_iter()
        .find(|l| l.list_id == list_id)
        .ok_or_else(|| format!("Liste de diffusion non trouvée: {list_id}"))?;

    let unsub_info = mailpurge_core::unsubscribe::UnsubscribeInfo {
        method: group.method,
        target: group.target,
        post_body: group.post_body,
    };
    let unsub_message = mailpurge_core::unsubscribe::execute_unsubscribe(&unsub_info, dry_run)?;

    let mut emails_purged = 0;
    let mut bytes_freed = 0;

    if move_to_trash && !group.uids.is_empty() {
        let emails = state
            .storage
            .get_emails_by_domain(account_id, &root_domain)
            .map_err(|e| format!("Erreur lecture emails: {e}"))?;

        let candidates: Vec<PurgeCandidate> = emails
            .into_iter()
            .filter(|e| group.uids.contains(&e.uid))
            .map(|e| PurgeCandidate {
                folder_path: e.folder_path,
                uid: e.uid,
                size_bytes: e.size_bytes,
                // Lors d'un désabonnement explicite avec mise en corbeille,
                // l'utilisateur souhaite supprimer TOUS les emails affiliés à cette liste
                // (y compris ceux protégés uniquement par la rétention de date récente).
                classification: EmailClassification::Purgeable,
            })
            .collect();

        let config = PurgeConfig {
            batch_size: 500,
            dry_run,
        };

        let account = state
            .storage
            .get_account(account_id)
            .map_err(|e| format!("Erreur compte: {e}"))?
            .ok_or_else(|| format!("Compte introuvable: {account_id}"))?;

        let report = if account.id == 1 && account.email == "etienne@entreprise-m365.fr" {
            let mut provider = state
                .mock_provider
                .lock()
                .map_err(|_| "Verrouillage provider échoué".to_string())?;
            PurgeExecutor::execute(&mut *provider, &candidates, "Corbeille", &config)
                .map_err(|e| format!("Erreur mise en corbeille: {e}"))?
        } else {
            let imap_config = resolve_imap_config(&account, &state.secret_cache)?;
            let mut provider = mailpurge_core::imap_provider::ImapProvider::new(imap_config);
            let trash_folder = get_trash_folder_path(&provider);
            PurgeExecutor::execute(&mut provider, &candidates, &trash_folder, &config)
                .map_err(|e| format!("Erreur mise en corbeille: {e}"))?
        };

        emails_purged = report.purged_count;
        bytes_freed = report.purged_bytes;

        if !dry_run {
            for (folder, purged_uids) in &report.details_by_folder {
                state
                    .storage
                    .delete_emails(account_id, folder, purged_uids)
                    .map_err(|e| format!("Erreur mise à jour cache SQLite: {e}"))?;
            }
        }
    }

    Ok(UnsubscribeReport {
        message: unsub_message,
        emails_purged,
        bytes_freed,
    })
}

#[tauri::command]
fn start_oauth_flow(
    state: State<'_, AppState>,
    provider: String,
    client_id: Option<String>,
) -> Result<AccountRecord, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Erreur socket locale: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Erreur port local: {e}"))?
        .port();

    let pkce = generate_pkce();
    let now_millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let state_csrf = format!("state_{now_millis}");
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let auth_provider = match provider.as_str() {
        "google" => OAuthProvider::Google,
        _ => OAuthProvider::Microsoft365,
    };

    let client_id_str = client_id.as_deref().unwrap_or(match auth_provider {
        OAuthProvider::Google => "812345678901-placeholder.apps.googleusercontent.com",
        OAuthProvider::Microsoft365 => "d3590ed6-52b3-4102-aeff-aad2292ab01c",
    });

    let auth_url = build_authorization_url(
        &auth_provider,
        client_id_str,
        &redirect_uri,
        &state_csrf,
        &pkce.challenge,
    );

    let _ = mailpurge_core::open::that(&auth_url);

    let code = listen_for_oauth_code(&listener, &state_csrf)
        .map_err(|e| format!("Échec du rappel OAuth: {e}"))?;

    let tokens = exchange_code_for_tokens(
        &auth_provider,
        client_id_str,
        None,
        &code,
        &pkce.verifier,
        &redirect_uri,
    )
    .map_err(|e| format!("Échec de l'obtention des jetons: {e}"))?;

    let email = tokens
        .id_token
        .as_deref()
        .and_then(extract_email_from_id_token)
        .unwrap_or_else(|| format!("compte_{now_millis}@mailpurge"));

    let secret = tokens.refresh_token.unwrap_or(tokens.access_token);
    KeyringStore::save_secret(&email, &secret)
        .map_err(|e| format!("Erreur trousseau système: {e}"))?;

    if let Ok(mut cache) = state.secret_cache.lock() {
        cache.insert(email.clone(), secret);
    }

    let provider_name = match auth_provider {
        OAuthProvider::Google => "Google Workspace",
        OAuthProvider::Microsoft365 => "Microsoft 365",
    };
    let display_name = format!("{email} ({provider_name})");

    state
        .storage
        .create_account(&email, &provider, &display_name)
        .map_err(|e| format!("Erreur enregistrement compte: {e}"))
}

#[tauri::command]
fn add_imap_account(
    state: State<'_, AppState>,
    email: String,
    password: String,
    server: String,
    port: u16,
    provider_type: String,
) -> Result<AccountRecord, String> {
    // 1. Validate credentials with IMAP server before saving
    let config = mailpurge_core::imap_provider::ImapConfig {
        server: server.clone(),
        port,
        username: email.clone(),
        auth: mailpurge_core::imap_provider::ImapAuthMethod::Password(password.clone()),
    };
    let test_provider = mailpurge_core::imap_provider::ImapProvider::new(config);
    let mut session = test_provider
        .connect_session()
        .map_err(|e| format!("Échec de connexion au serveur IMAP ({server}:{port}) : {e}"))?;
    let _ = session.logout();

    // 2. Save secret to OS keyring
    KeyringStore::save_secret(&email, &password)
        .map_err(|e| format!("Erreur trousseau système: {e}"))?;

    if let Ok(mut cache) = state.secret_cache.lock() {
        cache.insert(email.clone(), password);
    }

    let display_name = format!("{email} ({server}:{port})");
    state
        .storage
        .create_account(&email, &provider_type, &display_name)
        .map_err(|e| format!("Erreur enregistrement compte: {e}"))
}

#[tauri::command]
fn sync_account(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    account_id: i64,
) -> Result<(), String> {
    let storage = state.storage.clone();
    let secret_cache = state.secret_cache.clone();
    let account = storage
        .get_account(account_id)
        .map_err(|e| format!("Erreur lecture compte: {e}"))?
        .ok_or_else(|| format!("Compte introuvable: {account_id}"))?;

    if account.id == 1 && account.email == "etienne@entreprise-m365.fr" {
        // Mode Démo : synchroniser via le mock_provider
        let mut mock = state
            .mock_provider
            .lock()
            .map_err(|_| "Verrouillage mock échoué".to_string())?;
        sync_engine::execute_sync(&app, &storage, account_id, &mut *mock)?;
        return Ok(());
    }

    // Compte Réel : exécuter en arrière-plan
    let app_clone = app.clone();
    std::thread::spawn(move || {
        eprintln!(
            "[Sync] Lancement synchronisation en arrière-plan pour '{}' (ID={})...",
            account.email, account_id
        );
        let run_res = (|| -> Result<usize, String> {
            let imap_config = resolve_imap_config(&account, &secret_cache)?;
            eprintln!(
                "[Sync] Configuration IMAP résolue : serveur={}:{}, user={}",
                imap_config.server, imap_config.port, imap_config.username
            );
            let mut provider = mailpurge_core::imap_provider::ImapProvider::new(imap_config);
            sync_engine::execute_sync(&app_clone, &storage, account_id, &mut provider)
        })();

        if let Err(e) = &run_res {
            eprintln!("[Sync] ERREUR synchronisation compte ID={account_id} : {e}");
            let _ = app_clone.emit(
                "scan-progress",
                sync_engine::ScanProgressEvent {
                    account_id,
                    folder: "ERROR".to_string(),
                    scanned: 0,
                    total: 0,
                    phase: "error".to_string(),
                    message: Some(e.clone()),
                },
            );
        } else if let Ok(count) = run_res {
            eprintln!("[Sync] SUCCÈS : {count} emails traités pour compte ID={account_id}");
        }
    });

    Ok(())
}

#[tauri::command]
fn recalculate_retention(
    state: State<'_, AppState>,
    account_id: i64,
    retention_days: u16,
) -> Result<Vec<DomainStats>, String> {
    let now = chrono::Utc::now().timestamp();
    let config = ClassifierConfig {
        retention_days: u32::from(retention_days),
        ..ClassifierConfig::default()
    };

    let stats = state
        .storage
        .get_domain_stats(account_id)
        .map_err(|e| format!("Erreur stats: {e}"))?;

    for stat in &stats {
        let emails = state
            .storage
            .get_emails_by_domain(account_id, &stat.root_domain)
            .map_err(|e| format!("Erreur emails: {e}"))?;

        let mut updated_emails = Vec::with_capacity(emails.len());
        for mut email in emails {
            if email.classification != EmailClassification::ProtectedUser {
                let new_class = classify_email(
                    &EmailMetadataInput {
                        uid: email.uid,
                        subject: &email.subject,
                        received_at_timestamp: email.received_at,
                        is_flagged: email.is_flagged,
                        has_unsubscribe: email.has_unsubscribe,
                        attachment_names: &email.attachment_names,
                        root_domain: Some(&email.root_domain),
                    },
                    &config,
                    now,
                );
                email.classification = new_class;
            }
            updated_emails.push(email);
        }

        state
            .storage
            .upsert_emails(&updated_emails)
            .map_err(|e| format!("Erreur mise à jour: {e}"))?;
    }

    state
        .storage
        .get_domain_stats(account_id)
        .map_err(|e| format!("Erreur rafraîchissement stats: {e}"))
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
        body_snippet: &'static str,
        list_id: Option<&'static str>,
        list_unsub: Option<&'static str>,
        list_post: Option<&'static str>,
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
            body_snippet: "Profitez de remises exceptionnelles sur les rayons informatique, audio et jeux vidéo pendant 48 heures seulement. Livraison gratuite dès 25€ d'achat.",
            list_id: Some("<bons-plans.news.fnac.com>"),
            list_unsub: Some("<https://news.fnac.com/oneclick/unsub?id=demo8842>, <mailto:desabonnement@fnac.com>"),
            list_post: Some("List-Unsubscribe=One-Click"),
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
            body_snippet: "Il ne vous reste que quelques heures pour profiter de nos ventes flash exclusives adhérents Fnac+. Ne manquez pas cette opportunité.",
            list_id: Some("<bons-plans.news.fnac.com>"),
            list_unsub: Some("<https://news.fnac.com/oneclick/unsub?id=demo8842>, <mailto:desabonnement@fnac.com>"),
            list_post: Some("List-Unsubscribe=One-Click"),
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
            body_snippet: "Bonjour Etienne, veuillez trouver ci-joint en format PDF votre facture d'un montant de 549,99 € réglée par carte bancaire pour votre commande #98234.",
            list_id: None,
            list_unsub: None,
            list_post: None,
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
            body_snippet: "Votre extension de garantie 3 ans constructeur a bien été enregistrée pour votre écran Dell UltraSharp. Conservez précieusement ce justificatif.",
            list_id: None,
            list_unsub: None,
            list_post: None,
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
            body_snippet: "Les derniers GPU et CPU nouvelle génération sont enfin arrivés chez LDLC. Découvrez nos benchmarks complets et configurez votre machine sur mesure.",
            list_id: Some("<newsletter.ldlc.com>"),
            list_unsub: Some("<https://secure.ldlc.com/desabonnement-newsletter>"),
            list_post: None,
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
            body_snippet: "Notre équipe technique a concocté pour vous le meilleur rapport performance/prix pour jouer en 1440p sans compromis. Montage express garanti.",
            list_id: Some("<newsletter.ldlc.com>"),
            list_unsub: Some("<https://secure.ldlc.com/desabonnement-newsletter>"),
            list_post: None,
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
            body_snippet: "Votre commande LDLC n°4410 contenant votre alimentation modulaire a été expédiée via Chronopost. Numéro de suivi disponible sur votre espace client.",
            list_id: None,
            list_unsub: None,
            list_post: None,
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
            body_snippet: "Thank you for shopping with us. We'll send a confirmation once your package ships. View your order details anytime in your Amazon account.",
            list_id: None,
            list_unsub: None,
            list_post: None,
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
            body_snippet: "Veuillez trouver ci-joint votre facture officielle mentionnant la TVA déductible pour votre achat professionnel auprès du vendeur tiers Marketplace.",
            list_id: None,
            list_unsub: None,
            list_post: None,
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
            body_snippet: "Don't miss today's top lightning deals on headphones, smart home devices, and charging stations. Limited stock available.",
            list_id: Some("<deals.amazon.co.uk>"),
            list_unsub: Some("<https://www.amazon.co.uk/gp/gss/manage>"),
            list_post: None,
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
            body_snippet: "Deuxième démarque des soldes chez Boulanger ! Électroménager, TV OLED et multimédia à prix cassés en magasin et sur boulanger.com.",
            list_id: Some("<promos.boulanger.fr>"),
            list_unsub: Some("<mailto:desabonnement-auto@news.boulanger.fr?subject=desinscription%20newsletter>"),
            list_post: None,
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
            body_snippet: "Merci pour votre visite dans notre magasin Boulanger. Retrouvez votre ticket de caisse dématérialisé et les conditions de retour sous 15 jours.",
            list_id: None,
            list_unsub: None,
            list_post: None,
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
            body_snippet: "Plusieurs personnes de votre réseau ont consulté votre profil cette semaine. Découvrez leurs activités et restez connecté.",
            list_id: Some("<updates.linkedin.com>"),
            list_unsub: Some("<https://www.linkedin.com/e/v2?e=unsubscribe-one-click>, <mailto:list-unsubscribe@linkedin.com>"),
            list_post: Some("List-Unsubscribe=One-Click"),
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
            body_snippet: "Un recruteur vous a envoyé un message direct concernant une opportunité de poste Lead Developer Rust / Tauri. Cliquez pour y répondre.",
            list_id: Some("<messages.linkedin.com>"),
            list_unsub: Some("<https://www.linkedin.com/e/v2?e=unsubscribe-messages>"),
            list_post: None,
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
            body_snippet: "Découvrez 8 offres d'emploi récentes correspondant à vos critères de recherche Développeur Rust en télétravail ou hybride.",
            list_id: Some("<jobs.linkedin.com>"),
            list_unsub: Some("<https://www.linkedin.com/e/v2?e=unsubscribe-jobs>, <mailto:jobs-unsubscribe@linkedin.com>"),
            list_post: Some("List-Unsubscribe=One-Click"),
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
            body_snippet: "Merci d'avoir choisi Uber. Total de la course : 24,50 €. Prise en charge à Gare de Lyon et dépose à République. Facture téléchargeable.",
            list_id: None,
            list_unsub: None,
            list_post: None,
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
            body_snippet: "Détail du pourboire de 5,00 € versé à votre chauffeur suite à votre trajet du 15 mai. Email marqué d'un drapeau pour note de frais.",
            list_id: None,
            list_unsub: None,
            list_post: None,
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
            body_snippet: "Confirmation du renouvellement annuel automatique de vos domaines et services d'hébergement web OVHcloud. Contrat et bon de commande joints.",
            list_id: None,
            list_unsub: None,
            list_post: None,
        },

        // ORANGE.FR (Personal ISP mail - Zero false positive protection)
        DemoItem {
            from: "pierre.dupont@orange.fr",
            subject: "Photos de nos vacances en Bretagne",
            date_ts: now - (95 * 86400),
            size_bytes: 4_200_000,
            has_unsub: false,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
            body_snippet: "Salut Etienne ! J'espère que tout va bien de ton côté. Voici les photos de notre séjour en Bretagne le mois dernier. À très bientôt !",
            list_id: None,
            list_unsub: None,
            list_post: None,
        },

        // GMAIL.COM (Personal Webmail - Zero false positive protection)
        DemoItem {
            from: "claire.martin@gmail.com",
            subject: "Organisation de la réunion d'anciens élèves",
            date_ts: now - (85 * 86400),
            size_bytes: 380_000,
            has_unsub: false,
            has_att: false,
            atts: vec![],
            is_flagged: false,
            folder: "INBOX",
            body_snippet: "Bonjour Etienne, nous organisons une grande soirée retrouvailles le mois prochain à Paris. Fais-moi savoir si tu es disponible !",
            list_id: None,
            list_unsub: None,
            list_post: None,
        },
    ];

    let mut email_records = Vec::new();
    let mut mock_provider = state
        .mock_provider
        .lock()
        .map_err(|_| "Verrouillage provider échoué".to_string())?;

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
            root_domain: Some(&sender.root_domain),
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
            body_snippet: Some(item.body_snippet.to_string()),
            list_id: item.list_id.map(|s| s.to_string()),
            list_unsubscribe: item.list_unsub.map(|s| s.to_string()),
            list_unsubscribe_post: item.list_post.map(|s| s.to_string()),
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
                list_id: item.list_id.map(|s| s.to_string()),
                list_unsubscribe: item.list_unsub.map(|s| s.to_string()),
                list_unsubscribe_post: item.list_post.map(|s| s.to_string()),
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
        secret_cache: Arc::new(Mutex::new(HashMap::new())),
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
            load_demo_data,
            update_email_classification,
            get_distribution_lists,
            unsubscribe_and_purge,
            start_oauth_flow,
            add_imap_account,
            sync_account,
            recalculate_retention,
            delete_account,
            update_account_password,
            fetch_email_preview
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
