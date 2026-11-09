Voici le **Document de Conception Technique (TDD)** complet, intégrant les contraintes des environnements M365 Entreprise, Google Workspace et la gestion multi-dossiers.

---

## 1. Architecture Système (Tauri v2 + Rust Hexagonal)

L'application fonctionne intégralement en local. Aucune donnée ne transite par un serveur tiers.

```
+-------------------------------------------------------------------+
|                   FRONTEND GUI (WebView Natif)                    |
|  Vue 3 / TypeScript : Tableau des domaines, Filtres, Dry-Run UI   |
+-------------------------------------------------------------------+
                                  |  ▲
            Commandes IPC (Tauri) |  | Événements (Progression scan)
                                  ▼  |
+-------------------------------------------------------------------+
|                        CORE RUST (Backend)                        |
|  +--------------------+   +-------------------+   +------------+  |
|  | Moteur d'Analyse   |   | Gestionnaire Auth |   | Cache Local|  |
|  | (Règles & Scoring) |   | (OAuth2 + Keyring)|   | (SQLite)   |  |
|  +--------------------+   +-------------------+   +------------+  |
|                                 |                                 |
|                     Trait `MailProviderPort`                      |
|                     /                       \                     |
|      +-----------------------+     +-----------------------+      |
|      | Adapteur IMAP/XOAUTH2 |     | Adapteur Mock (Tests) |      |
|      +-----------------------+     +-----------------------+      |
+-------------------------------------------------------------------+

```

### Pourquoi un cache SQLite local est indispensable ?

Puisque tu souhaites scanner **tous les dossiers et sous-dossiers**, une boîte mail ancienne peut facilement contenir 50 000 à 150 000 messages répartis dans 40 répertoires.

* Le moteur Rust synchronise uniquement les **en-têtes et métadonnées** dans un fichier SQLite local (indexé par `Account + Folder + UIDVALIDITY + UID`).
* Une fois synchronisé, le regroupement par domaine, le changement du curseur d'ancienneté (ex: passer de 60 à 45 jours dans l'interface) ou l'ajustement des mots-clés se fait en **quelques millisecondes** en interrogeant SQLite, sans solliciter le serveur IMAP.

---

## 2. Gestion de l'Authentification (M365 Entreprise & Google Workspace)

Sur les tenants **Microsoft 365 Entreprise** et **Google Workspace**, l'authentification basique (login/mot de passe) est désactivée au profit d'**OAuth2 (mécanisme SASL XOAUTH2 en IMAP)**.

### Flux d'authentification dans l'application Desktop :

1. L'application embarque un mini-serveur HTTP éphémère sur `[http://127.0.0.1](http://127.0.0.1):<port_aléatoire>` (avec PKCE) ou utilise le **Device Code Flow**.
2. L'utilisateur se connecte dans son navigateur habituel (qui gère nativement le MFA / Authenticator / SSO de l'entreprise).
3. Le *Refresh Token* et l'*Access Token* récupérés sont stockés dans le **Trousseau natif de l'OS** via la crate `keyring` (Trousseau macOS / Gestionnaire d'identification Windows).

| Fournisseur | Méthode d'Auth IMAP | Scope OAuth2 requis | Serveur IMAP |
| --- | --- | --- | --- |
| **Microsoft 365 (Pro/Perso)** | `AUTHENTICATE XOAUTH2` | `[https://outlook.office.com/IMAP.AccessAsUser.All](https://outlook.office.com/IMAP.AccessAsUser.All)` + `offline_access` | `outlook.office365.com:993` |
| **Gmail / Workspace** | `AUTHENTICATE XOAUTH2` | `[https://mail.google.com/](https://mail.google.com/)` | `imap.gmail.com:993` |
| **iCloud** | `LOGIN` (Mot de passe d'app) | N/A | `imap.mail.me.com:993` |
| **IMAP Standard** | `LOGIN` / `PLAIN` | N/A | Configurable |

> **Point de vigilance M365 Entreprise / Workspace :** Pour utiliser XOAUTH2 avec ton propre outil local, il faudra déclarer une *App Registration* dans Microsoft Entra ID (Azure AD) et un *Client ID Desktop* dans Google Cloud Console. Si l'administrateur du tenant M365 bloque les applications tierces non vérifiées, un administrateur du tenant devra cliquer sur *"Grant admin consent"* pour ton application interne (opération unique qui prend 30 secondes).

---

## 3. Découverte Multi-Dossiers & Particularités de la Corbeille (RFC 6154)

Scanner tous les dossiers et déplacer les mails en corbeille sans se tromper de répertoire (que la boîte soit configurée en français, en anglais ou avec des libellés Gmail) repose sur l'extension **IMAP SPECIAL-USE (RFC 6154)**.

Lors de la commande `LIST "" "*"`, le serveur renvoie des attributs normalisés indépendants de la langue :

* `\Trash` : La Corbeille (`[Gmail]/Corbeille`, `Deleted Items`, `Éléments supprimés`, `Trash`).
* `\Junk` : Les Spams (à exclure ou traiter à part).
* `\Sent` : Les messages envoyés (à exclure du scan par défaut).
* `\Drafts` : Les brouillons (à exclure impérativement).
* **`\All` (`[Gmail]/Tous les messages`) :** **Critique sur Gmail/Workspace.** Gmail expose chaque libellé comme un dossier IMAP ET expose tous les mails dans `\All`. Pour éviter de scanner les mêmes mails 3 fois, la stratégie optimale sur Gmail dépend de ton usage : soit on scanne tous les dossiers sauf `\All`, `\Trash`, `\Sent`, `\Spam`, soit on déduplique par `X-GM-MSGID` (identifiant unique Gmail).

### Le Workflow de Suppression en 2 Temps :

1. **Mise en corbeille sécurisée (Action par défaut) :**
* Utilisation de l'extension `UID MOVE <sequence_set> "<Nom_Dossier_Trash>"` (RFC 6851, supportée par Gmail, M365, iCloud et Dovecot).
* *Fallback* automatique pour les vieux serveurs IMAP : `UID COPY` vers `\Trash`, puis `UID STORE +FLAGS.SILENT (\Deleted)`, puis `UID EXPUNGE` (RFC 4315) sur le dossier source uniquement.
* **Résultat :** Les mails disparaissent de leurs dossiers de classement respectifs et atterrissent tous dans la Corbeille. L'utilisateur peut aller vérifier sa corbeille depuis son client mail habituel.


2. **Vidage de la corbeille (Action explicite dans l'UI) :**
* Un bouton dédié *"Vider la corbeille du compte X"* sélectionne uniquement le dossier porteur de l'attribut `\Trash`, applique `\Deleted` et exécute `EXPUNGE`.



---

## 4. Extraction du Domaine & Modèle de Données (SQLite)

Pour regrouper intelligemment les emails d'une même société qui utilise des sous-domaines (ex: `news.fnac.com`, `commandes.fnac.com`, `info.boulanger.fr`), un simple `split('@')` ne suffit pas. Il faut extraire l'**eTLD+1 (Domaine Racine Enregistrable)** à l'aide de la crate `psl` (Public Suffix List) ou `addr`.

* `promo@newsletter.ldlc.com` $\rightarrow$ Domaine racine : **`ldlc.com`** / Sous-domaine : `newsletter.ldlc.com`
* `facture@service-client.amazon.co.uk` $\rightarrow$ Domaine racine : **`amazon.co.uk`** (et non `co.uk`)

### Schéma Relationnel Minimal (Cache SQLite)

```sql
CREATE TABLE emails (
    account_id INTEGER NOT NULL,
    folder_path TEXT NOT NULL,
    uid INTEGER NOT NULL,
    message_id TEXT,               -- Pour dédupliquer si un mail est dans plusieurs dossiers
    root_domain TEXT NOT NULL,     -- ex: "ldlc.com" (indexé pour le group by)
    full_domain TEXT NOT NULL,     -- ex: "news.ldlc.com"
    sender_address TEXT NOT NULL,  -- ex: "promo@news.ldlc.com"
    subject TEXT NOT NULL,
    received_at INTEGER NOT NULL,  -- Timestamp Unix
    size_bytes INTEGER NOT NULL,
    has_unsubscribe BOOLEAN NOT NULL,
    has_attachment BOOLEAN NOT NULL,
    attachment_names TEXT,         -- JSON array ex: '["facture_123.pdf"]'
    is_flagged BOOLEAN NOT NULL,
    classification TEXT NOT NULL,  -- 'PURGEABLE', 'PROTECTED_INVOICE', 'RECENT'
    PRIMARY KEY (account_id, folder_path, uid)
);

CREATE INDEX idx_domain_stats ON emails(account_id, root_domain, classification);

```

Grâce à cet index, la requête qui alimente ton tableau de bord principal est instantanée :

```sql
SELECT 
    root_domain,
    COUNT(*) as total_emails,
    SUM(size_bytes) as total_size,
    SUM(CASE WHEN classification = 'PURGEABLE' THEN 1 ELSE 0 END) as purgeable_count,
    SUM(CASE WHEN classification = 'PURGEABLE' THEN size_bytes ELSE 0 END) as purgeable_size,
    SUM(CASE WHEN classification = 'PROTECTED_INVOICE' THEN 1 ELSE 0 END) as protected_count
FROM emails
WHERE account_id = ?1
GROUP BY root_domain
ORDER BY purgeable_size DESC;

```

---

## 5. Plan d'Implémentation TDD (Cycle Rouge $\rightarrow$ Vert $\rightarrow$ Refactor)

Pour démarrer le développement proprement en Rust, nous allons découper le code en **4 modules indépendants**, chacun piloté par ses tests unitaires avant même d'écrire la moindre ligne d'interface graphique :

### Module 1 : `domain_extractor` & `classifier` (100 % pur Rust, 0 dépendance réseau)

* **Test 1 :** Extraire correctement le `root_domain` sur des adresses complexes (`"Service Client" <no-reply@mail.support.societe.fr>` $\rightarrow$ `societe.fr`).
* **Test 2 :** Détecter les mots-clés transactionnels insensibles à la casse et aux accents (`Facture`, `Commande`, `Reçu`, `Order`, `Invoice`, `Billet`, `Réservation`, `Garantie`, `Contrat`, `Avoir`).
* **Test 3 :** Protéger systématiquement un email de plus de 60 jours s'il possède un fichier `.pdf` joint ET ne contient pas d'en-tête `List-Unsubscribe`, même si son sujet est ambigu.
* **Test 4 :** Valider le groupement de séquences d'UIDs IMAP (transformer `[1, 2, 3, 7, 9, 10]` en `"1:3,7,9:10"`) pour éviter d'envoyer des commandes IMAP trop longues.

### Module 2 : `folder_scanner` (Mock IMAP)

* **Test 5 :** Filtrer la liste des dossiers renvoyés par `LIST` pour exclure automatiquement `\Trash`, `\Sent`, `\Drafts`, `\Junk`, tout en conservant l'intégralité de l'arborescence personnalisée de l'utilisateur (ex: `INBOX/Achats`, `Archives/2024`).
* **Test 6 :** Identifier dynamiquement le chemin exact du dossier `\Trash` quel que soit le fournisseur (Gmail, M365, iCloud).

### Module 3 : `purge_executor` (Sécurité des opérations)

* **Test 7 :** Vérifier qu'aucun email marqué `Protected` en base ne peut être inclus dans un lot d'`UID MOVE`, même si l'utilisateur demande la purge complète d'un domaine.
* **Test 8 :** Vérifier le découpage par lots (*chunking* par paquets de 500 UIDs) lors du déplacement vers la corbeille pour respecter les limites de longueur de ligne des serveurs Microsoft Exchange / M365.

---

### Conception de l'Interface Graphique (Vue Principale)

L'écran principal de l'application pourra se structurer en 3 panneaux :

1. **Barre latérale gauche :** Liste des comptes configurés + jauge d'espace récupérable + Bouton *"Vider la corbeille"* par compte.
2. **Panneau central (Tableau des Domaines) :** Classé par **Volume Purgeable (Mo/Go)** ou **Nombre de mails**. Pour chaque domaine (ex: `fnac.com`), tu vois en un clin d'œil : *Total (450 mails) | Purgeables > 60j (412 mails - 85 Mo) | Protégés Factures/Commandes (38 mails)*.
3. **Panneau de droite (Inspecteur / Filet de sécurité) :** En cliquant sur `fnac.com`, tu as deux onglets : *"Candidats à la corbeille (412)"* et *"Protégés (38)"*, avec la possibilité de cocher/décocher manuellement un mail ou d'ajouter une adresse expéditrice (ex: `compta@fnac.com`) en liste blanche permanente avant de cliquer sur **"Mettre en corbeille"**.