use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

use mailpurge_core::classifier::{classify_email, ClassifierConfig, EmailMetadataInput};
use mailpurge_core::domain_extractor::{extract_domain_from_list_id, extract_sender_domain};
use mailpurge_core::provider_port::MailProviderPort;
use mailpurge_core::storage::{EmailRecord, MailStorageRepository};

/// Event emitted over Tauri IPC to update the UI on scanning progress.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanProgressEvent {
    pub account_id: i64,
    pub folder: String,
    pub scanned: usize,
    pub total: usize,
    pub phase: String,
    pub message: Option<String>,
}

/// Executes mailbox folder synchronization, classification, and SQLite upsertion.
pub async fn execute_sync(
    app: &AppHandle,
    storage: &MailStorageRepository,
    account_id: i64,
    provider: &mut dyn MailProviderPort,
    cancel_flag: Arc<AtomicBool>,
) -> Result<usize, String> {
    if cancel_flag.load(Ordering::Relaxed) {
        return Ok(0);
    }

    // 1. Initial connection & discovery phase
    let _ = app.emit(
        "scan-progress",
        ScanProgressEvent {
            account_id,
            folder: "Connexion".to_string(),
            scanned: 0,
            total: 0,
            phase: "connecting".to_string(),
            message: Some("Connexion au serveur IMAP et authentification...".to_string()),
        },
    );

    let folders = provider
        .list_folders()
        .await
        .map_err(|e| format!("Erreur liste des dossiers: {e}"))?;

    // Harvest trusted recipient domains from Sent folder (implicit whitelist)
    if let Some(sent_folder) = folders.iter().find(|f| f.is_sent()) {
        if let Ok(sent_count) = provider.get_folder_message_count(&sent_folder.path).await {
            if sent_count > 0 {
                let start_seq = sent_count.saturating_sub(200).max(1);
                if let Ok(sent_envelopes) = provider
                    .fetch_envelopes_range(&sent_folder.path, start_seq, sent_count)
                    .await
                {
                    let mut trusted_domains = HashSet::new();
                    for env in sent_envelopes {
                        if let Some(to) = env.to_header {
                            for recipient in to.split(',') {
                                if let Ok(extracted) = extract_sender_domain(recipient.trim()) {
                                    if !extracted.root_domain.is_empty()
                                        && extracted.root_domain != "unknown"
                                    {
                                        trusted_domains.insert(extracted.root_domain);
                                    }
                                }
                            }
                        }
                    }
                    if !trusted_domains.is_empty() {
                        let domain_list: Vec<String> = trusted_domains.into_iter().collect();
                        let _ = storage.add_user_trusted_domains(
                            account_id,
                            &domain_list,
                            "sent_folder",
                        );
                        eprintln!(
                            "[SyncEngine] Récolté {} domaines légitimes de confiance depuis le dossier Éléments envoyés",
                            domain_list.len()
                        );
                    }
                }
            }
        }
    }

    let scannable_folders: Vec<_> = folders.into_iter().filter(|f| f.is_scannable()).collect();
    let classifier_config = ClassifierConfig::default();
    let now = chrono::Utc::now().timestamp();

    // 2. Discover message counts per folder
    let mut folders_with_counts = Vec::new();
    let mut grand_total = 0usize;

    for folder in scannable_folders {
        if cancel_flag.load(Ordering::Relaxed) {
            return Ok(0);
        }
        let count = match provider.get_folder_message_count(&folder.path).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[SyncEngine] Avertissement: échec lecture dossier '{}': {e}",
                    folder.path
                );
                0
            }
        };
        grand_total += count as usize;
        folders_with_counts.push((folder, count));
    }

    eprintln!(
        "[SyncEngine] Découverte terminée : {} dossiers scannables, {} emails au total",
        folders_with_counts.len(),
        grand_total
    );

    let _ = app.emit(
        "scan-progress",
        ScanProgressEvent {
            account_id,
            folder: "Découverte".to_string(),
            scanned: 0,
            total: grand_total,
            phase: "discovered".to_string(),
            message: Some(format!(
                "{} dossiers identifiés — {} emails au total à analyser",
                folders_with_counts.len(),
                grand_total
            )),
        },
    );

    // 3. Scan & chunk envelopes across folders
    let mut total_scanned = 0usize;
    let mut seen_message_ids: HashSet<String> = HashSet::new();

    for (folder, folder_count) in folders_with_counts {
        if cancel_flag.load(Ordering::Relaxed) {
            eprintln!("[SyncEngine] Synchronisation annulée pour le compte ID={account_id}");
            return Ok(total_scanned);
        }
        if folder_count == 0 {
            continue;
        }

        let mut seq = 1u32;
        let batch_size = 250u32;

        while seq <= folder_count {
            if cancel_flag.load(Ordering::Relaxed) {
                eprintln!("[SyncEngine] Synchronisation annulée pour le compte ID={account_id}");
                return Ok(total_scanned);
            }
            let end_seq = (seq + batch_size - 1).min(folder_count);
            let envelopes = match provider
                .fetch_envelopes_range(&folder.path, seq, end_seq)
                .await
            {
                Ok(envs) => envs,
                Err(e) => {
                    eprintln!(
                        "Erreur lecture dossier {} (plage {seq}-{end_seq}): {e}",
                        folder.path
                    );
                    seq = end_seq + 1;
                    continue;
                }
            };

            let mut email_records = Vec::with_capacity(envelopes.len());

            for env in envelopes {
                // Deduplicate multi-label emails (e.g. Gmail)
                if let Some(msg_id) = &env.message_id {
                    if !seen_message_ids.insert(msg_id.clone()) {
                        continue;
                    }
                }

                let (root_domain, full_domain, sender_address) =
                    match extract_sender_domain(&env.from) {
                        Ok(s) => (s.root_domain, s.full_domain, s.email_address),
                        Err(_) => {
                            let from_list =
                                env.list_id.as_deref().and_then(extract_domain_from_list_id);
                            if let Some(domain) = from_list {
                                (domain.clone(), domain, env.from.clone())
                            } else {
                                (
                                    "unknown".to_string(),
                                    "unknown".to_string(),
                                    env.from.clone(),
                                )
                            }
                        }
                    };
                let has_attachment = !env.attachment_names.is_empty();

                let classification = classify_email(
                    &EmailMetadataInput {
                        uid: env.uid,
                        subject: &env.subject,
                        received_at_timestamp: env.date_timestamp,
                        is_flagged: env.is_flagged,
                        has_unsubscribe: env.has_unsubscribe,
                        attachment_names: &env.attachment_names,
                        root_domain: Some(&root_domain),
                        spam_verdict: None,
                    },
                    &classifier_config,
                    now,
                );

                email_records.push(EmailRecord {
                    account_id,
                    folder_path: folder.path.clone(),
                    uid: env.uid,
                    message_id: env.message_id,
                    root_domain,
                    full_domain,
                    sender_address,
                    subject: env.subject,
                    received_at: env.date_timestamp,
                    size_bytes: env.size_bytes,
                    has_unsubscribe: env.has_unsubscribe,
                    has_attachment,
                    attachment_names: env.attachment_names,
                    is_flagged: env.is_flagged,
                    classification,
                    body_snippet: None,
                    list_id: env.list_id,
                    list_unsubscribe: env.list_unsubscribe,
                    list_unsubscribe_post: env.list_unsubscribe_post,
                    return_path: env.return_path,
                    auth_results: env.authentication_results,
                    spam_headers: env.spam_verdict_header,
                    to_recipients: env.to_header,
                });
            }

            if !email_records.is_empty() {
                storage
                    .upsert_emails(&email_records)
                    .map_err(|e| format!("Erreur persistance emails: {e}"))?;
            }

            total_scanned += (end_seq - seq + 1) as usize;
            let pct = total_scanned
                .saturating_mul(100)
                .checked_div(grand_total)
                .unwrap_or(100);

            eprintln!(
                "[SyncEngine] {} : {end_seq}/{folder_count} ({total_scanned}/{grand_total} au total - {pct}%)",
                folder.path
            );

            let _ = app.emit(
                "scan-progress",
                ScanProgressEvent {
                    account_id,
                    folder: folder.path.clone(),
                    scanned: total_scanned,
                    total: grand_total,
                    phase: "scanning".to_string(),
                    message: Some(format!(
                        "Dossier {} : {}/{} ({}/{} au total - {}%)",
                        folder.path, end_seq, folder_count, total_scanned, grand_total, pct
                    )),
                },
            );

            seq = end_seq + 1;
        }
    }

    let _ = app.emit(
        "scan-progress",
        ScanProgressEvent {
            account_id,
            folder: "Terminé".to_string(),
            scanned: total_scanned,
            total: grand_total,
            phase: "completed".to_string(),
            message: Some(format!(
                "{total_scanned} emails synchronisés et classifiés avec succès"
            )),
        },
    );

    Ok(total_scanned)
}
