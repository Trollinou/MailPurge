import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import type {
  AccountRecord,
  DomainSortOption,
  DomainStats,
  EmailRecord,
  PurgeReport,
} from "../types";

export const useMailStore = defineStore("mail", {
  state: () => ({
    accounts: [] as AccountRecord[],
    currentAccountId: null as number | null,
    domainStats: [] as DomainStats[],
    selectedDomain: null as string | null,
    selectedDomainEmails: [] as EmailRecord[],
    selectedUidsToPurge: new Set<number>(),
    dryRunEnabled: true,
    isLoading: false,
    isPurging: false,
    activeTab: "purgeable" as "purgeable" | "protected",
    searchQuery: "",
    sortBy: "size" as DomainSortOption,
    previewedEmail: null as EmailRecord | null,
    lastReport: null as PurgeReport | null,
    statusMessage: "" as string,
  }),

  getters: {
    currentAccount: (state) =>
      state.accounts.find((a) => a.id === state.currentAccountId) || null,

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
      this.selectedUidsToPurge.clear();
      await this.refreshDomainStats();
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
        this.selectedDomainEmails = await invoke<EmailRecord[]>(
          "get_emails_by_domain",
          {
            accountId: this.currentAccountId,
            rootDomain: domain,
          }
        );

        // Pre-select all purgeable candidates by default
        for (const email of this.selectedDomainEmails) {
          if (String(email.classification).toUpperCase() === "PURGEABLE") {
            this.selectedUidsToPurge.add(email.uid);
          }
        }
      } catch (err) {
        console.error("Failed to load emails for domain:", err);
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

    openPreview(email: EmailRecord) {
      this.previewedEmail = email;
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
