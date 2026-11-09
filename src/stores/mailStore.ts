import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AccountRecord,
  DistributionListGroup,
  DomainSortOption,
  DomainStats,
  EmailRecord,
  PurgeReport,
  ScanProgressEvent,
  UnsubscribeReport,
} from "../types";

export const useMailStore = defineStore("mail", {
  state: () => ({
    accounts: [] as AccountRecord[],
    currentAccountId: null as number | null,
    domainStats: [] as DomainStats[],
    selectedDomain: null as string | null,
    selectedDomainEmails: [] as EmailRecord[],
    distributionLists: [] as DistributionListGroup[],
    selectedUidsToPurge: new Set<number>(),
    dryRunEnabled: true,
    isLoading: false,
    isPurging: false,
    isSyncing: false,
    scanProgress: null as ScanProgressEvent | null,
    retentionDays: 60 as number,
    activeTab: "purgeable" as "purgeable" | "subscriptions" | "protected",
    searchQuery: "",
    sortBy: "size" as DomainSortOption,
    previewedEmail: null as EmailRecord | null,
    lastReport: null as PurgeReport | null,
    statusMessage: "" as string,
  }),

  getters: {
    currentAccount: (state) =>
      state.accounts.find((a) => a.id === state.currentAccountId) || null,

    currentDomainStats: (state) =>
      state.domainStats.find((d) => d.root_domain === state.selectedDomain) || null,

    filteredDomainStats: (state) => {
      const q = state.searchQuery.trim().toLowerCase();
      let list = state.domainStats;
      if (q) {
        list = list.filter((d) => d.root_domain.toLowerCase().includes(q));
      }

      const sorted = [...list];
      if (state.sortBy === "size") {
        sorted.sort((a, b) => b.purgeable_size - a.purgeable_size);
      } else if (state.sortBy === "count") {
        sorted.sort((a, b) => b.total_emails - a.total_emails);
      } else if (state.sortBy === "alpha") {
        sorted.sort((a, b) => a.root_domain.localeCompare(b.root_domain));
      }

      return sorted;
    },

    totalPurgeableBytes: (state) => {
      return state.domainStats.reduce((acc, d) => acc + d.purgeable_size, 0);
    },

    totalPurgeableCount: (state) => {
      return state.domainStats.reduce((acc, d) => acc + d.purgeable_count, 0);
    },

    purgeableEmails: (state) => {
      return state.selectedDomainEmails.filter(
        (e) => String(e.classification).toUpperCase() === "PURGEABLE"
      );
    },

    protectedEmails: (state) => {
      return state.selectedDomainEmails.filter(
        (e) => String(e.classification).toUpperCase() !== "PURGEABLE"
      );
    },

    allPurgeableSelected: (state) => {
      const purgeable = state.selectedDomainEmails.filter(
        (e) => String(e.classification).toUpperCase() === "PURGEABLE"
      );
      if (purgeable.length === 0) return false;
      return purgeable.every((e) => state.selectedUidsToPurge.has(e.uid));
    },
  },

  actions: {
    async initSyncListener() {
      try {
        console.log("[InitSyncListener] Attaching scan-progress listener...");
        await listen<ScanProgressEvent>("scan-progress", (event) => {
          console.log("[ScanProgressEvent]", event.payload);
          this.scanProgress = event.payload;
          if (
            event.payload.phase === "connecting" ||
            event.payload.phase === "discovering" ||
            event.payload.phase === "discovered" ||
            event.payload.phase === "scanning"
          ) {
            this.isSyncing = true;
            if (event.payload.message) {
              this.statusMessage = event.payload.message;
            }
            if (event.payload.scanned > 0 && event.payload.scanned % 500 === 0) {
              this.refreshDomainStats();
            }
          } else if (event.payload.phase === "completed") {
            this.isSyncing = false;
            this.statusMessage =
              event.payload.message || "Synchronisation terminée avec succès.";
            this.refreshDomainStats();
          } else if (event.payload.phase === "error") {
            this.isSyncing = false;
            this.statusMessage =
              "Erreur synchronisation : " + (event.payload.message || "Échec inconnu");
          }
        });
        console.log("[InitSyncListener] Listener attached successfully!");
      } catch (err) {
        console.error("Could not attach sync listener:", err);
      }
    },

    async setRetentionDays(days: number) {
      this.retentionDays = days;
      if (!this.currentAccountId) return;
      try {
        this.isLoading = true;
        this.statusMessage = `Recalcul de la rétention (${days} jours)...`;
        this.domainStats = await invoke<DomainStats[]>("recalculate_retention", {
          accountId: this.currentAccountId,
          retentionDays: days,
        });
        if (this.selectedDomain) {
          await this.selectDomain(this.selectedDomain);
        }
        this.statusMessage = `Règle mise à jour : emails de plus de ${days} jours éligibles.`;
      } catch (err) {
        this.statusMessage = "Erreur recalcul rétention : " + String(err);
      } finally {
        this.isLoading = false;
      }
    },

    async syncCurrentAccount() {
      if (!this.currentAccountId) return;
      try {
        this.isSyncing = true;
        this.statusMessage = "Démarrage de l'aspiration des dossiers IMAP...";
        this.scanProgress = {
          account_id: this.currentAccountId,
          folder: "Connexion",
          scanned: 0,
          total: 0,
          phase: "connecting",
          message: "Connexion au serveur IMAP...",
        };
        await invoke("sync_account", { accountId: this.currentAccountId });
      } catch (err) {
        this.isSyncing = false;
        this.statusMessage = "Erreur démarrage synchronisation : " + String(err);
      }
    },

    async startOAuthFlow(provider: "microsoft365" | "google", clientId?: string) {
      try {
        this.isLoading = true;
        this.statusMessage = `Ouverture du navigateur pour connexion ${provider}...`;
        const account = await invoke<AccountRecord>("start_oauth_flow", {
          provider,
          clientId: clientId || null,
        });
        await this.fetchAccounts();
        await this.selectAccount(account.id);
        this.statusMessage = `Compte ${account.email} connecté avec succès !`;
        await this.syncCurrentAccount();
        return account;
      } catch (err) {
        this.statusMessage = "Échec connexion OAuth : " + String(err);
        throw err;
      } finally {
        this.isLoading = false;
      }
    },

    async addImapAccount(params: {
      email: string;
      password: string;
      server: string;
      port: number;
      providerType: string;
    }) {
      try {
        this.isLoading = true;
        this.statusMessage = `Vérification des identifiants et connexion à ${params.server}...`;
        const account = await invoke<AccountRecord>("add_imap_account", params);
        await this.fetchAccounts();
        await this.selectAccount(account.id);
        this.statusMessage = `Compte ${account.email} connecté avec succès ! Début du scan...`;
        await this.syncCurrentAccount();
        return account;
      } catch (err) {
        this.statusMessage = "Échec ajout compte IMAP : " + String(err);
        throw err;
      } finally {
        this.isLoading = false;
      }
    },

    async fetchAccounts() {
      try {
        this.isLoading = true;
        this.accounts = await invoke<AccountRecord[]>("get_accounts");
        if (this.accounts.length > 0 && !this.currentAccountId) {
          await this.selectAccount(this.accounts[0].id);
        }
      } catch (err) {
        console.warn("Could not fetch accounts:", err);
      } finally {
        this.isLoading = false;
      }
    },

    async selectAccount(accountId: number) {
      this.currentAccountId = accountId;
      this.selectedDomain = null;
      this.selectedDomainEmails = [];
      this.distributionLists = [];
      this.selectedUidsToPurge.clear();
      await this.refreshDomainStats();
    },

    async deleteAccount(accountId: number) {
      try {
        this.isLoading = true;
        this.statusMessage = "Suppression du compte en cours...";
        await invoke("delete_account", { accountId });
        this.accounts = this.accounts.filter((a) => a.id !== accountId);
        if (this.currentAccountId === accountId) {
          if (this.accounts.length > 0) {
            await this.selectAccount(this.accounts[0].id);
          } else {
            this.currentAccountId = null;
            this.domainStats = [];
            this.selectedDomain = null;
            this.selectedDomainEmails = [];
          }
        }
        this.statusMessage = "Compte supprimé avec succès.";
      } catch (err) {
        this.statusMessage = "Erreur suppression compte : " + String(err);
        throw err;
      } finally {
        this.isLoading = false;
      }
    },

    async updateAccountPassword(accountId: number, password: string) {
      try {
        this.isLoading = true;
        this.statusMessage = "Vérification et mise à jour du mot de passe...";
        await invoke("update_account_password", { accountId, password });
        this.statusMessage = "Mot de passe mis à jour avec succès ! Lancement du scan...";
        await this.syncCurrentAccount();
      } catch (err) {
        this.statusMessage = "Échec mise à jour mot de passe : " + String(err);
        throw err;
      } finally {
        this.isLoading = false;
      }
    },

    async refreshDomainStats() {
      if (!this.currentAccountId) return;
      try {
        this.isLoading = true;
        this.domainStats = await invoke<DomainStats[]>("get_domain_stats", {
          accountId: this.currentAccountId,
        });

        if (this.domainStats.length > 0 && !this.selectedDomain) {
          await this.selectDomain(this.domainStats[0].root_domain);
        }
      } catch (err) {
        console.error("Failed to load domain stats:", err);
      } finally {
        this.isLoading = false;
      }
    },

    async selectDomain(domain: string) {
      this.selectedDomain = domain;
      this.selectedUidsToPurge.clear();
      if (!this.currentAccountId) return;

      try {
        this.isLoading = true;
        const [emails, lists] = await Promise.all([
          invoke<EmailRecord[]>("get_emails_by_domain", {
            accountId: this.currentAccountId,
            rootDomain: domain,
          }),
          invoke<DistributionListGroup[]>("get_distribution_lists", {
            accountId: this.currentAccountId,
            rootDomain: domain,
          }),
        ]);

        this.selectedDomainEmails = emails;
        this.distributionLists = lists;

        // Pre-select all purgeable candidates by default
        for (const email of this.selectedDomainEmails) {
          if (String(email.classification).toUpperCase() === "PURGEABLE") {
            this.selectedUidsToPurge.add(email.uid);
          }
        }
      } catch (err) {
        console.error("Failed to load domain data:", err);
      } finally {
        this.isLoading = false;
      }
    },

    toggleSelectAllPurgeable() {
      const purgeable = this.purgeableEmails;
      if (this.allPurgeableSelected) {
        for (const e of purgeable) {
          this.selectedUidsToPurge.delete(e.uid);
        }
      } else {
        for (const e of purgeable) {
          this.selectedUidsToPurge.add(e.uid);
        }
      }
    },

    toggleSelectUid(uid: number) {
      if (this.selectedUidsToPurge.has(uid)) {
        this.selectedUidsToPurge.delete(uid);
      } else {
        this.selectedUidsToPurge.add(uid);
      }
    },

    async openPreview(email: EmailRecord) {
      this.previewedEmail = email;
      const isCorrupted =
        !email.body_snippet ||
        email.body_snippet.trim() === "" ||
        email.body_snippet.trim().startsWith("<!DOCTYPE") ||
        email.body_snippet.trim().startsWith("<html") ||
        email.body_snippet.trim().startsWith("<head") ||
        email.body_snippet.trim().startsWith("<style") ||
        email.body_snippet.trim().startsWith("<!--") ||
        email.body_snippet.trim().startsWith("@media") ||
        email.body_snippet.includes("{font-family") ||
        email.body_snippet.includes("!important");

      if (isCorrupted && this.currentAccountId) {
        if (this.previewedEmail) {
          this.previewedEmail.body_snippet = undefined;
        }
        try {
          const snippet = await invoke<string>("fetch_email_preview", {
            accountId: this.currentAccountId,
            folderPath: email.folder_path,
            uid: email.uid,
          });
          if (snippet && snippet.trim() !== "") {
            email.body_snippet = snippet;
            if (this.previewedEmail && this.previewedEmail.uid === email.uid) {
              this.previewedEmail.body_snippet = snippet;
            }
          }
        } catch (err) {
          console.warn("Could not fetch email preview:", err);
        }
      }
    },

    closePreview() {
      this.previewedEmail = null;
    },

    async toggleEmailProtection(email: EmailRecord) {
      if (!this.currentAccountId) return;
      const isPurgeable = String(email.classification).toUpperCase() === "PURGEABLE";
      const targetClassification = isPurgeable ? "PROTECTED_USER" : "PURGEABLE";

      try {
        await invoke("update_email_classification", {
          accountId: this.currentAccountId,
          folderPath: email.folder_path,
          uid: email.uid,
          newClassification: targetClassification,
        });

        // Update local state immediately
        email.classification = targetClassification;
        if (targetClassification === "PURGEABLE") {
          this.selectedUidsToPurge.add(email.uid);
          this.statusMessage = `Email marqué comme candidat à la corbeille.`;
        } else {
          this.selectedUidsToPurge.delete(email.uid);
          this.statusMessage = `Email protégé manuellement.`;
        }

        // Refresh stats from SQLite in background
        await this.refreshDomainStats();
      } catch (err) {
        this.statusMessage = "Erreur lors du changement de statut: " + String(err);
      }
    },

    async loadDemoData() {
      try {
        this.isLoading = true;
        this.statusMessage = "Chargement des données de démonstration...";
        const account = await invoke<AccountRecord>("load_demo_data");
        await this.fetchAccounts();
        await this.selectAccount(account.id);
        this.statusMessage = "Données de démonstration chargées avec succès !";
      } catch (err) {
        this.statusMessage = "Erreur lors du chargement démo: " + String(err);
      } finally {
        this.isLoading = false;
      }
    },

    async executePurge() {
      if (!this.currentAccountId || !this.selectedDomain) return;
      const uids = Array.from(this.selectedUidsToPurge);
      if (uids.length === 0) return;

      try {
        this.isPurging = true;
        const report = await invoke<PurgeReport>("execute_purge", {
          accountId: this.currentAccountId,
          rootDomain: this.selectedDomain,
          uids,
          dryRun: this.dryRunEnabled,
        });

        this.lastReport = report;

        if (!this.dryRunEnabled) {
          this.statusMessage = `${report.purged_count} emails déplacés vers la corbeille.`;
          await this.refreshDomainStats();
          if (this.selectedDomain) {
            await this.selectDomain(this.selectedDomain);
          }
        } else {
          this.statusMessage = `[Simulation] ${report.purged_count} emails éligibles (${formatBytes(report.purged_bytes)}). Aucun email déplacé.`;
        }
      } catch (err) {
        this.statusMessage = "Erreur lors de la purge: " + String(err);
      } finally {
        this.isPurging = false;
      }
    },

    async unsubscribeAndPurge(listId: string, moveToTrash: boolean) {
      if (!this.currentAccountId || !this.selectedDomain) return;
      try {
        this.isPurging = true;
        const report = await invoke<UnsubscribeReport>("unsubscribe_and_purge", {
          accountId: this.currentAccountId,
          rootDomain: this.selectedDomain,
          listId,
          moveToTrash,
          dryRun: this.dryRunEnabled,
        });

        let msg = report.message;
        if (moveToTrash && report.emails_purged > 0) {
          msg += ` • ${report.emails_purged} email(s) mis en corbeille (${formatBytes(report.bytes_freed)} libéré(s)).`;
        }
        this.statusMessage = msg;

        await this.refreshDomainStats();
        if (this.selectedDomain) {
          await this.selectDomain(this.selectedDomain);
        }
      } catch (err) {
        this.statusMessage = "Erreur désabonnement: " + String(err);
      } finally {
        this.isPurging = false;
      }
    },

    async emptyTrash() {
      if (!this.currentAccountId) return;
      try {
        this.isPurging = true;
        const deletedCount = await invoke<number>("empty_trash", {
          accountId: this.currentAccountId,
        });
        this.statusMessage = `Corbeille vidée : ${deletedCount} emails purgés définitivement.`;
        await this.refreshDomainStats();
      } catch (err) {
        this.statusMessage = "Erreur lors du vidage corbeille: " + String(err);
      } finally {
        this.isPurging = false;
      }
    },
  },
});

export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 o";
  const k = 1024;
  const sizes = ["o", "Ko", "Mo", "Go"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}
