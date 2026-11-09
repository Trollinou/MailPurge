# MailPurge 🛡️✉️

> Application Desktop locale (*100% on-premise*) de nettoyage sécurisé et intelligent de boîtes mails IMAP & OAuth2.
> Conçue avec **Tauri v2**, **Rust** et **Vue 3 / TypeScript**.

---

## 🌟 Fonctionnalités Clés

- **Découverte & Regroupement par Domaine Racine (eTLD+1) :**
  - Agrégation volumétrique et comptage des emails par entité légale via la *Public Suffix List* (`psl`).
  - Extracteur multi-stratégies ultra-robuste résistant aux virgules non échappées, caractères multi-octets UTF-8, chevrons atypiques et identifiants de liste (`List-ID`).
- **Filet de Sécurité Zéro Faux-Positif :**
  - Les emails transactionnels (factures, reçus de paiement, bons de commande, garanties, contrats) sont sanctuarisés et protégés contre toute purge accidentelle.
  - Protection des expéditeurs personnels (fournisseurs FAI et webmails grand public).
  - Protection automatique des courriels récents (< 60 jours par défaut, configurable).
- **Désabonnement en 1 Clic (RFC 8058 & RFC 2369) :**
  - Prise en charge du One-Click `List-Unsubscribe=One-Click` via requête HTTPS `POST`.
  - Prise en charge des liens de désinscription web (`https://...`) et des URI `mailto:`.
  - Action combinée « Se désabonner + Mettre en corbeille » pour éliminer tous les courriels affiliés à une liste sans être bloqué par l'ancienneté.
- **Moteur IMAP Conforme & Optimisé :**
  - Connexion TLS port 993 avec mise en cache des sessions et des identifiants (réduction drastique des sollicitations du trousseau OS).
  - Détection dynamique du dossier Corbeille conforme à la **RFC 6154** (`SPECIAL-USE` `\Trash`).
  - Prise en charge de la **RFC 6851** (`UID MOVE`) avec repli atomique `UID COPY` + `STORE +FLAGS (\Deleted)` + `UID EXPUNGE`.
  - Échappement rigoureux des noms de boîtes aux lettres (`quote_mailbox_name`).
- **Aperçu Épuré et Sécurisé des Messages :**
  - Extraction à la demande sans marquer l'email comme lu (`BODY.PEEK[]`).
  - Décodage récursif de l'arborescence MIME (*Quoted-Printable*, *Base64*, *Multipart/Alternative*).
  - Élimination intégrale du code CSS (`<style>`, `@media`, règles typographiques) et des scripts (`<script>`) pour un texte clair et immédiatement lisible.
- **Confidentialité & Sécurité Maximale :**
  - 100 % local : aucun serveur tiers, aucune télémétrie.
  - Stockage des secrets exclusivement dans le trousseau sécurisé de l'OS (**macOS Keychain**, **Windows Credential Manager**, **Secret Service** sous Linux).

---

## 🏛️ Architecture Logicielle (Hexagonale / Ports & Adapters)

Le cœur du projet est découplé de l'infrastructure selon les principes de l'architecture hexagonale :

```
mailpurge/
├── crates/mailpurge-core/
│   ├── src/
│   │   ├── domain/               # Entités & Règles métier pures (Zéro I/O)
│   │   │   ├── retention.rs      # Calculateur d'ancienneté et rétention
│   │   │   └── ...
│   │   ├── ports/                # Interfaces abstraites (Traits)
│   │   │   └── ...
│   │   ├── adapters/             # Implémentations techniques
│   │   ├── classifier.rs         # Moteur de scoring & classification transactionnelle
│   │   ├── domain_extractor.rs   # Extracteur résilient eTLD+1 & sanitisation
│   │   ├── folder_scanner.rs     # Détection RFC 6154 des dossiers
│   │   ├── imap_provider.rs      # Adaptateur IMAP TLS (async/blocking + rustls)
│   │   ├── storage.rs            # Adaptateur SQLite avec migrations auto
│   │   └── unsubscribe.rs        # Moteur HTTP/Mailto RFC 8058 / RFC 2369
│   └── tests/                    # Tests unitaires, TDD & intégration
├── src-tauri/                    # Couche applicative & IPC Tauri v2
│   ├── src/
│   │   ├── lib.rs                # Commandes Tauri exposées au frontend
│   │   └── sync_engine.rs        # Moteur de synchronisation en arrière-plan
│   └── tauri.conf.json
└── src/                          # Frontend Vue 3 + Tailwind CSS + Lucide Icons
    ├── stores/mailStore.ts       # Store Pinia centralisé
    └── components/
```

---

## 🛠️ Prérequis & Installation

- **Rust** (1.80+ recommandé)
- **Node.js** (v18+ ou v20+)
- **Cargo Tauri CLI** (`cargo install tauri-cli`)

### Lancement en mode développement

```bash
# Installation des dépendances frontend
npm install

# Lancement de l'application Tauri en mode dev
cargo tauri dev
```

### Compilation de production

```bash
cargo tauri build
```

---

## 🧪 Quality Gate (Validation & Intégrité)

Le projet applique une politique stricte de tolérance zéro sur la qualité du code :

1. **Formatage :** `cargo fmt --all -- --check`
2. **Analyse Statique (Clippy Pedantic) :** `cargo clippy --all-targets --all-features -- -D warnings`
3. **Tests Unitaires & Intégration :** `cargo test --workspace`
4. **Dépendances Inutilisées :** `cargo machete`
5. **Compilation Frontend :** `npm run build`

Pour exécuter la suite de tests complète :
```bash
cargo test --workspace
```
