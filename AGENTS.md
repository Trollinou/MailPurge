# AGENTS.md — Règles de l'Art & Directives d'Ingénierie (Email Purger)

## 1. Contexte et Objectif Critique du Projet
Application Desktop 100 % locale (*on-premise*) basée sur **Tauri v2** (Backend **Rust** / Frontend TypeScript) destinée à analyser des boîtes mails IMAP (Microsoft 365, Google Workspace/Gmail, iCloud, IMAP standard), regrouper les volumétries par domaine racine (eTLD+1), gérer le désabonnement (RFC 8058 / RFC 2369) et purger les emails publicitaires obsolètes.

**RÈGLE MÉTIER ABSOLUE (Zéro Faux-Positif) :**
Un email transactionnel contenant une commande, une facture, un reçu, un contrat ou une garantie ne doit **JAMAIS** être qualifié comme `PurgeCandidate`, quel que soit son âge. En cas d'ambiguïté, le moteur doit toujours favoriser la protection (`Protected`) plutôt que la suppression.

---

## 2. Architecture Logicielle (Hexagonale / Ports & Adapters)
Le cœur Rust (`src-tauri` ou workspace `core`) doit être strictement découplé des entrées/sorties (réseau IMAP, base SQLite, IPC Tauri) :
- **`domain/` (Cœur Métier Pur) :** Aucune dépendance I/O (`async-imap`, `rusqlite`, `reqwest`, `tauri` y sont strictement interdits). Contient les entités, les Value Objects, le calculateur de score de sécurité, l'extracteur eTLD+1 et le moteur de règles de purge.
- **`ports/` (Traits) :** Définit les interfaces abstraites (`MailProviderPort`, `EmailRepositoryPort`, `UnsubscribePort`, `SecretStorePort`).
- **`adapters/` (Implémentations) :**
  - `imap_adapter` : Implémentation `async-imap` + `tokio-rustls` + `mail-parser`. Gestion RFC 6154 (`SPECIAL-USE` pour identifier `\Trash`, `\Sent`, `\All`, `\Drafts`) et RFC 6851 (`UID MOVE`).
  - `sqlite_adapter` : Cache local des en-têtes indexé par `(account_id, folder_path, uid_validity, uid)`.
  - `oauth_keyring_adapter` : Authentification OAuth2 (XOAUTH2) et stockage exclusif des secrets dans le trousseau de l'OS (`keyring`). Aucun mot de passe ou token en clair sur le disque ou dans les logs.
  - `http_unsub_adapter` : Exécution de la RFC 8058 (`POST` avec `List-Unsubscribe=One-Click`).

---

## 3. Méthodologie TDD (Test-Driven Development) Obligatoire
Tout développement d'une nouvelle fonctionnalité ou correction de bug DOIT suivre le cycle **Red -> Green -> Refactor** :
1. **Red :** Écrire les tests unitaires en premier (`rstest` pour les cas paramétrés, `proptest` pour le parsing d'entrées arbitraires, `mockall` pour mocker les `ports`).
2. **Green :** Écrire le code minimal respectant les lints stricts pour faire passer le test au vert.
3. **Refactor :** Nettoyer, optimiser et documenter en maintenant les tests au vert.

---

## 4. Standards de Code Rust (Tolérance Zéro)
- **Zéro Crash en Production :** Les appels `.unwrap()`, `.expect()`, `panic!()`, `todo!()` et `unimplemented!()` sont **interdits** dans le code source hors tests. Toute erreur doit être propagée proprement avec `Result<T, E>` via `thiserror` (dans le domaine/lib) ou `anyhow` (à la frontière applicative).
- **Zéro `unsafe` :** `unsafe_code = "forbid"` est activé globalement.
- **Typage Fort (Newtype Pattern) :** Ne pas manipuler de types primitifs bruts pour des concepts métier critiques. Utiliser des types dédiés (`ImapUid(u32)`, `RootDomain(String)`, `RetentionDays(u16)`).
- **Optimisation IMAP :**
  - Ne jamais télécharger le corps complet d'un email lors du scan initial. Utiliser uniquement `FETCH ... (UID FLAGS INTERNALDATE RFC822.SIZE BODYSTRUCTURE BODY.PEEK[HEADER.FIELDS (FROM SUBJECT MESSAGE-ID LIST-UNSUBSCRIBE LIST-UNSUBSCRIBE-POST)])`.
  - Toujours utiliser `BODY.PEEK` pour ne jamais marquer un email non lu comme lu lors de l'analyse.
  - Découper les commandes `UID MOVE` ou `UID STORE` par lots (chunks de 500 UIDs max) et compacter les plages d'UIDs (`1:5,8,10:15`).
  - L'action de purge par défaut déplace vers `\Trash` sans exécuter `EXPUNGE` sur la corbeille. Le vidage de la corbeille est une commande distincte nécessitant une confirmation explicite.
- **Encapsulation Synchrone dans Runtime Tokio (`spawn_blocking`) :** Tout appel I/O synchrone (`ureq::post`, `ureq::get`, ou accès disque SQLite avec `rusqlite`) invoqué depuis un contexte asynchrone Tokio DOIT être encapsulé dans `tokio::task::spawn_blocking(move || { ... }).await` afin de préserver la réactivité du pool de threads de travail et de l'interface Tauri.
- **Journalisation Structurée :** Utiliser exclusivement l'écosystème `tracing` (`tracing::info!`, `tracing::debug!`, `tracing::warn!`, `tracing::error!`). L'usage de `println!` ou `eprintln!` est interdit dans le code applicatif hors tests.
- **Tests Unitaires :** Dans les modules `#[cfg(test)]`, l'usage de `.unwrap()` ou `.expect("raison explicite")` est toléré via `#[allow(clippy::unwrap_used, clippy::expect_used)]`.

---

## 5. Quality Gate (Validation Obligatoire par l'Agent)
Avant de conclure toute tâche ou de proposer un commit, l'agent DOIT exécuter la Quality Gate complète et corriger toute erreur ou avertissement :

1. **Formatage :** `cargo fmt --all -- --check`
2. **Analyse Statique (Clippy Pedantic) :** `cargo clippy --all-targets --all-features -- -D warnings`
3. **Tests Unitaires, Intégration & Doc-tests :** `cargo nextest run` (et `cargo test --doc`)
4. **Audit de Sécurité & Licences :** `cargo deny check` (ou `cargo audit`)
5. **Dépendances inutilisées :** `cargo machete`

Une tâche n'est JAMAIS terminée tant que l'une de ces 5 étapes retourne un code d'erreur non nul.
