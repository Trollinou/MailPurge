<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useMailStore, formatBytes } from "./stores/mailStore";
import AddAccountModal from "./components/AddAccountModal.vue";
import EditAccountModal from "./components/EditAccountModal.vue";
import OrphanSpamModal from "./components/OrphanSpamModal.vue";
import SpamRulesSettingsModal from "./components/SpamRulesSettingsModal.vue";
import type { AccountRecord, DomainStats } from "./types";
import {
  Mail,
  Trash2,
  ShieldCheck,
  ShieldAlert,
  CheckSquare,
  Square,
  Search,
  Database,
  FileText,
  Star,
  CheckCircle2,
  AlertCircle,
  AlertTriangle,
  Inbox,
  Clock,
  Sparkles,
  Eye,
  X,
  ArrowUpDown,
  Shield,
  RotateCcw,
  Zap,
  ExternalLink,
  Send,
  BellRing,
  User,
  Plus,
  RefreshCw,
  Loader2,
  KeyRound,
  Edit3,
  Target,
  Filter,
  ChevronDown,
  Skull,
  Flame,
  Bookmark,
} from "lucide-vue-next";

const store = useMailStore();
const isAddAccountModalOpen = ref(false);
const isEditAccountModalOpen = ref(false);
const accountToEdit = ref<AccountRecord | null>(null);

const isOrphanSpamModalOpen = ref(false);
const isSpamRulesModalOpen = ref(false);
const isTagFilterOpen = ref(false);

function openOrphanSpamModal() {
  isOrphanSpamModalOpen.value = true;
  store.fetchOrphanSummary(store.orphanScope, store.orphanThreshold);
}

async function unmarkDomainWithFeedback(rootDomain: string) {
  try {
    await store.unmarkOrphanDomain(rootDomain, true);
  } catch (e) {
    console.error("Failed to unmark domain", e);
  }
}

const isPasswordModalOpen = ref(false);
const newPassword = ref("");
const passwordError = ref("");
const isSubmittingPassword = ref(false);

const accountToDelete = ref<AccountRecord | null>(null);
const isDeletingAccount = ref(false);

const forcePurgeDomain = ref<DomainStats | null>(null);
const isExecutingForcePurge = ref(false);

const isConfirmUnprotectModalOpen = ref(false);

function openEditAccountModal(account: AccountRecord) {
  accountToEdit.value = account;
  isEditAccountModalOpen.value = true;
}

async function openForcePurgeModal(domain: DomainStats) {
  forcePurgeDomain.value = domain;
  if (store.selectedDomain !== domain.root_domain) {
    await store.selectDomain(domain.root_domain);
  }
}

async function handleExecuteForcePurge() {
  if (!forcePurgeDomain.value) return;
  try {
    isExecutingForcePurge.value = true;
    await store.purgeAllDomainEmails(forcePurgeDomain.value.root_domain);
    forcePurgeDomain.value = null;
  } catch (err) {
    console.error("Force purge error:", err);
  } finally {
    isExecutingForcePurge.value = false;
  }
}

async function handleConfirmUnprotect() {
  if (!store.selectedDomain) return;
  try {
    await store.unprotectAllDomainEmails(store.selectedDomain);
    isConfirmUnprotectModalOpen.value = false;
  } catch (err) {
    console.error("Unprotect all error:", err);
  }
}

async function handleReclassifyDomain() {
  if (!store.selectedDomain) return;
  try {
    await store.reclassifyDomainEmails(store.selectedDomain);
  } catch (err) {
    console.error("Reclassify domain error:", err);
  }
}

async function executeDeleteAccount() {
  if (!accountToDelete.value) return;
  try {
    isDeletingAccount.value = true;
    await store.deleteAccount(accountToDelete.value.id);
    accountToDelete.value = null;
  } catch (err) {
    console.error("Delete account error:", err);
  } finally {
    isDeletingAccount.value = false;
  }
}

async function handlePasswordSubmit() {
  if (!store.currentAccountId || !newPassword.value) return;
  try {
    isSubmittingPassword.value = true;
    passwordError.value = "";
    await store.updateAccountPassword(store.currentAccountId, newPassword.value);
    isPasswordModalOpen.value = false;
    newPassword.value = "";
  } catch (err) {
    passwordError.value = String(err);
  } finally {
    isSubmittingPassword.value = false;
  }
}

onMounted(async () => {
  await store.initSyncListener();
  await store.fetchAccounts();
  await store.fetchCommunityRules();
});

function formatDate(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  return d.toLocaleDateString("fr-FR", {
    day: "2-digit",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatRelativeDate(ts: number): string {
  if (!ts) return "";
  const now = Math.floor(Date.now() / 1000);
  const diff = now - ts;
  if (diff < 60) return "à l'instant";
  if (diff < 3600) return `il y a ${Math.floor(diff / 60)} min`;
  if (diff < 86400) return `il y a ${Math.floor(diff / 3600)} h`;
  if (diff < 86400 * 30) return `il y a ${Math.floor(diff / 86400)} j`;
  if (diff < 86400 * 365) return `il y a ${Math.floor(diff / (86400 * 30))} mois`;
  const years = Math.floor(diff / (86400 * 365));
  return `il y a ${years} an${years > 1 ? "s" : ""}`;
}
</script>

<template>
  <div class="flex h-screen w-screen overflow-hidden bg-slate-950 text-slate-100 text-sm">
    <!-- ======================================================== -->
    <!-- PANNEAU 1 : BARRE LATÉRALE GAUCHE (Comptes & Jauge)      -->
    <!-- ======================================================== -->
    <aside class="w-72 bg-slate-900 border-r border-slate-800 flex flex-col justify-between shrink-0">
      <div>
        <!-- App Header / Logo -->
        <div class="p-4 border-b border-slate-800 flex items-center justify-between">
          <div class="flex items-center space-x-2.5">
            <div class="w-8 h-8 rounded-lg bg-indigo-600 flex items-center justify-center shadow-lg shadow-indigo-600/30">
              <Mail class="w-4 h-4 text-white" />
            </div>
            <div>
              <h1 class="font-bold text-base tracking-tight text-white">MailPurge</h1>
              <p class="text-[11px] text-slate-400">Nettoyage Intelligent</p>
            </div>
          </div>
          <span class="text-[10px] font-mono px-2 py-0.5 rounded-full bg-indigo-950 text-indigo-400 border border-indigo-800/60">
            v0.1
          </span>
        </div>

        <!-- Section Comptes -->
        <div class="p-3">
          <div class="flex items-center justify-between px-2 mb-2 text-xs font-semibold text-slate-400 uppercase tracking-wider">
            <span>Comptes</span>
            <div class="flex items-center space-x-2">
              <button
                @click="isAddAccountModalOpen = true"
                class="text-sky-400 hover:text-sky-300 flex items-center space-x-1 text-[11px] normal-case font-medium hover:underline"
                title="Ajouter un compte réel (M365, Gmail, iCloud, IMAP)"
              >
                <Plus class="w-3 h-3" />
                <span>+ Compte</span>
              </button>
              <button
                v-if="store.accounts.length === 0"
                @click="store.loadDemoData"
                class="text-indigo-400 hover:text-indigo-300 flex items-center space-x-1 text-[11px] normal-case"
                title="Recharger le jeu d'essai démo"
              >
                <Sparkles class="w-3 h-3" />
                <span>Démo</span>
              </button>
            </div>
          </div>

          <div class="space-y-1">
            <div
              v-for="account in store.accounts"
              :key="account.id"
              @click="store.selectAccount(account.id)"
              :class="[
                'w-full text-left p-2.5 rounded-lg transition-all flex flex-col space-y-1 cursor-pointer',
                store.currentAccountId === account.id
                  ? 'bg-indigo-600/20 text-white border border-indigo-500/40 shadow-sm'
                  : 'hover:bg-slate-800/60 text-slate-300 border border-transparent'
              ]"
            >
              <div class="flex items-center justify-between w-full">
                <span class="font-medium truncate text-xs">{{ account.account_name || account.email }}</span>
                <div class="flex items-center gap-1">
                  <button
                    v-if="store.currentAccountId === account.id"
                    @click.stop="store.syncCurrentAccount"
                    :disabled="store.isSyncing"
                    class="text-slate-400 hover:text-sky-400 p-1 rounded hover:bg-slate-800/80 transition"
                    title="Synchroniser la boîte maintenant"
                  >
                    <RefreshCw :class="['w-3 h-3', store.isSyncing ? 'animate-spin text-sky-400' : '']" />
                  </button>
                  <button
                    @click.stop="openEditAccountModal(account)"
                    class="text-slate-400 hover:text-sky-400 p-1 rounded hover:bg-slate-800/80 transition"
                    title="Modifier le nom du compte"
                  >
                    <Edit3 class="w-3 h-3" />
                  </button>
                  <button
                    v-if="account.id !== 1"
                    @click.stop="isPasswordModalOpen = true"
                    class="text-slate-400 hover:text-sky-400 p-1 rounded hover:bg-slate-800/80 transition"
                    title="Mettre à jour le mot de passe"
                  >
                    <KeyRound class="w-3 h-3" />
                  </button>
                  <button
                    @click.stop="accountToDelete = account"
                    class="text-slate-400 hover:text-rose-400 p-1 rounded hover:bg-slate-800/80 transition"
                    title="Supprimer ce compte"
                  >
                    <Trash2 class="w-3 h-3" />
                  </button>
                  <span class="text-[10px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 font-mono uppercase">
                    {{ account.provider_type }}
                  </span>
                </div>
              </div>
              <span v-if="account.account_name" class="text-[11px] text-slate-400 truncate">{{ account.email }}</span>
              <span v-else-if="account.server_host" class="text-[11px] text-slate-400 truncate font-mono text-[10px]">{{ account.server_host }}</span>
            </div>

            <div
              v-if="store.accounts.length === 0"
              class="p-4 text-center rounded-lg border border-dashed border-slate-800 text-slate-500 text-xs space-y-2"
            >
              <p>Aucun compte connecté</p>
              <button
                @click="store.loadDemoData"
                class="text-indigo-400 hover:text-indigo-300 inline-flex items-center gap-1 text-[11px] underline"
              >
                <Sparkles class="w-3 h-3" />
                <span>Charger le mode démo</span>
              </button>
            </div>
          </div>

          <!-- Indicateur de Scan IMAP en Direct -->
          <div
            v-if="store.isSyncing"
            class="mt-2 p-2.5 bg-sky-950/40 border border-sky-500/30 rounded-xl space-y-1.5"
          >
            <div class="flex items-center justify-between text-[11px] text-sky-300 font-medium">
              <span class="flex items-center gap-1.5 truncate">
                <Loader2 class="w-3 h-3 animate-spin shrink-0 text-sky-400" />
                <span class="truncate">{{ store.scanProgress?.folder || 'Synchronisation...' }}</span>
              </span>
              <span class="shrink-0 font-mono text-[10px]">
                <span v-if="store.scanProgress && store.scanProgress.total > 0">
                  {{ store.scanProgress.scanned }} / {{ store.scanProgress.total }}
                </span>
                <span v-else>...</span>
              </span>
            </div>
            <div class="w-full bg-slate-900 rounded-full h-1.5 overflow-hidden">
              <div
                class="bg-sky-500 h-1.5 rounded-full transition-all duration-300"
                :style="{
                  width: store.scanProgress && store.scanProgress.total > 0
                    ? Math.min(100, (store.scanProgress.scanned / store.scanProgress.total) * 100) + '%'
                    : '50%'
                }"
              ></div>
            </div>
            <p class="text-[10px] text-slate-400 truncate">
              {{ store.scanProgress?.message || "Préparation de l'aspiration..." }}
            </p>
          </div>
        </div>

        <!-- Curseur d'Ancienneté Dynamique -->
        <div class="px-4 py-3 mx-3 mt-1 rounded-xl bg-slate-800/40 border border-slate-800/80 space-y-2">
          <div class="flex items-center justify-between text-xs">
            <span class="text-slate-400 flex items-center space-x-1.5">
              <Clock class="w-3.5 h-3.5 text-indigo-400" />
              <span>Ancienneté requise</span>
            </span>
            <span class="font-bold text-indigo-300 text-xs font-mono">
              > {{ store.retentionDays }} jours
            </span>
          </div>
          <div class="grid grid-cols-4 gap-1 p-0.5 bg-slate-900 border border-slate-800 rounded-lg">
            <button
              v-for="days in [30, 45, 60, 90]"
              :key="days"
              @click="store.setRetentionDays(days)"
              :class="[
                'py-1 text-[11px] font-mono rounded font-medium transition',
                store.retentionDays === days
                  ? 'bg-indigo-600 text-white shadow'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
              ]"
            >
              {{ days }}j
            </button>
          </div>
        </div>

        <!-- Jauge d'Espace Récupérable -->
        <div class="px-4 py-3 mx-3 mt-2 rounded-xl bg-slate-800/40 border border-slate-800/80">
          <div class="flex items-center justify-between text-xs mb-1.5">
            <span class="text-slate-400 flex items-center space-x-1.5">
              <Database class="w-3.5 h-3.5 text-indigo-400" />
              <span>Espace purgeable</span>
            </span>
            <span class="font-bold text-indigo-300 text-xs font-mono">
              {{ formatBytes(store.totalPurgeableBytes) }}
            </span>
          </div>
          <div class="w-full bg-slate-900 rounded-full h-2 overflow-hidden border border-slate-700/50">
            <div
              class="bg-gradient-to-r from-indigo-500 to-rose-500 h-2 rounded-full transition-all duration-500"
              :style="{ width: store.totalPurgeableBytes > 0 ? '65%' : '0%' }"
            ></div>
          </div>
          <p class="text-[11px] text-slate-400 mt-2">
            <strong class="text-slate-200 font-semibold">{{ store.totalPurgeableCount }}</strong> emails prêts à être nettoyés
          </p>
        </div>
      </div>

      <!-- Footer Gauche : Bouton Vider la Corbeille -->
      <div class="p-3 border-t border-slate-800 space-y-2">
        <button
          @click="openOrphanSpamModal"
          class="w-full py-2.5 px-3 rounded-xl bg-gradient-to-r from-rose-950/60 to-purple-950/60 hover:from-rose-900/60 hover:to-purple-900/60 text-rose-200 border border-rose-500/30 flex items-center justify-between text-xs font-semibold shadow-sm transition group"
        >
          <div class="flex items-center gap-2">
            <Target class="w-4 h-4 text-rose-400 group-hover:scale-110 transition-transform" />
            <span>Chasse aux Orphelins</span>
          </div>
          <span class="px-1.5 py-0.5 rounded-full text-[10px] bg-rose-500/20 text-rose-300 font-mono border border-rose-500/30">
            Spams
          </span>
        </button>

        <button
          @click="store.emptyTrash"
          :disabled="store.isPurging || !store.currentAccountId"
          class="w-full py-2 px-3 rounded-lg bg-rose-950/40 hover:bg-rose-900/50 text-rose-300 border border-rose-800/50 hover:border-rose-700 flex items-center justify-center space-x-2 text-xs transition disabled:opacity-50"
        >
          <Trash2 class="w-3.5 h-3.5" />
          <span>Vider la Corbeille du compte</span>
        </button>
      </div>
    </aside>

    <!-- ======================================================== -->
    <!-- PANNEAU 2 : TABLEAU DES DOMAINES (Central)               -->
    <!-- ======================================================== -->
    <main class="flex-1 flex flex-col min-w-0 bg-slate-950 border-r border-slate-800">
      <!-- En-tête avec Recherche et Tri -->
      <header class="p-4 border-b border-slate-800 flex items-center justify-between gap-4">
        <div>
          <h2 class="text-base font-semibold text-white truncate max-w-sm" :title="store.currentAccount?.account_name || store.currentAccount?.email">
            {{ store.currentAccount?.account_name || store.currentAccount?.email || "Domaines Détectés" }}
          </h2>
          <p class="text-xs text-slate-400">
            {{ store.filteredDomainStats.length }} domaine{{ store.filteredDomainStats.length > 1 ? 's' : '' }} détecté{{ store.filteredDomainStats.length > 1 ? 's' : '' }} (eTLD+1)
          </p>
        </div>

        <div class="flex items-center space-x-2.5">
          <!-- Tri Dropdown -->
          <div class="relative">
            <select
              v-model="store.sortBy"
              class="bg-slate-900 border border-slate-700/60 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-indigo-500 cursor-pointer font-medium"
              title="Critère de tri des domaines"
            >
              <option value="size">Trier : Volume (Octets)</option>
              <option value="count">Trier : Nb Mails</option>
              <option value="alpha">Trier : Nom (A-Z)</option>
              <option value="date">Trier : Plus Récent</option>
            </select>
          </div>

          <!-- Filtre par Tags Dropdown -->
          <div class="relative">
            <button
              @click="isTagFilterOpen = !isTagFilterOpen"
              :class="[
                'flex items-center gap-1.5 px-2.5 py-1.5 text-xs rounded-lg border transition font-medium',
                store.selectedTags.length > 0
                  ? 'bg-indigo-600/20 text-indigo-300 border-indigo-500/40 shadow-sm'
                  : 'bg-slate-900 text-slate-400 border-slate-700/60 hover:text-slate-200'
              ]"
              title="Filtrer les domaines par étiquettes et alertes"
            >
              <Filter class="w-3.5 h-3.5" />
              <span>Tags</span>
              <span
                v-if="store.selectedTags.length > 0"
                class="px-1.5 py-0.2 rounded-full bg-indigo-600 text-white text-[10px] font-bold"
              >
                {{ store.selectedTags.length }}
              </span>
              <ChevronDown class="w-3 h-3 text-slate-400" />
            </button>

            <!-- Dropdown popover with tag checkboxes -->
            <div
              v-if="isTagFilterOpen"
              class="absolute right-0 mt-1.5 w-64 bg-slate-900 border border-slate-700 rounded-xl shadow-2xl z-30 p-2.5 space-y-1 text-xs"
            >
              <div class="flex items-center justify-between pb-1.5 mb-1 border-b border-slate-800 px-1 text-slate-400 font-semibold text-[11px]">
                <span>Filtrer par tags</span>
                <button
                  v-if="store.selectedTags.length > 0"
                  @click="store.selectedTags = []"
                  class="text-indigo-400 hover:text-indigo-300 text-[10px]"
                >
                  Effacer tout
                </button>
              </div>
              <label class="flex items-center gap-2 p-1.5 rounded hover:bg-slate-800 cursor-pointer text-slate-200">
                <input type="checkbox" value="ALL_ORPHANS" v-model="store.selectedTags" class="rounded border-slate-700 bg-slate-800 text-indigo-500" />
                <span>🎯 Tous les orphelins marqués</span>
              </label>
              <label class="flex items-center gap-2 p-1.5 rounded hover:bg-slate-800 cursor-pointer text-slate-200">
                <input type="checkbox" value="DEAD_DOMAIN" v-model="store.selectedTags" class="rounded border-slate-700 bg-slate-800 text-indigo-500" />
                <span>💀 Domaine Mort (NXDOMAIN)</span>
              </label>
              <label class="flex items-center gap-2 p-1.5 rounded hover:bg-slate-800 cursor-pointer text-slate-200">
                <input type="checkbox" value="SERVER_SPAM" v-model="store.selectedTags" class="rounded border-slate-700 bg-slate-800 text-indigo-500" />
                <span>🛡️ Échec Auth / Rejet Serveur</span>
              </label>
              <label class="flex items-center gap-2 p-1.5 rounded hover:bg-slate-800 cursor-pointer text-slate-200">
                <input type="checkbox" value="STRUCTURAL_ANOMALY" v-model="store.selectedTags" class="rounded border-slate-700 bg-slate-800 text-indigo-500" />
                <span>⚠️ Anomalie Expéditeur / TLD</span>
              </label>
              <label class="flex items-center gap-2 p-1.5 rounded hover:bg-slate-800 cursor-pointer text-slate-200">
                <input type="checkbox" value="DISPOSABLE" v-model="store.selectedTags" class="rounded border-slate-700 bg-slate-800 text-indigo-500" />
                <span>🚫 Domaine Jetable (Burner)</span>
              </label>
              <div class="border-t border-slate-800 my-1"></div>
              <label class="flex items-center gap-2 p-1.5 rounded hover:bg-slate-800 cursor-pointer text-slate-200">
                <input type="checkbox" value="PROTECTED" v-model="store.selectedTags" class="rounded border-slate-700 bg-slate-800 text-indigo-500" />
                <span>🔒 Avec emails protégés</span>
              </label>
              <label class="flex items-center gap-2 p-1.5 rounded hover:bg-slate-800 cursor-pointer text-slate-200">
                <input type="checkbox" value="SPAM_SUSPECT" v-model="store.selectedTags" class="rounded border-slate-700 bg-slate-800 text-indigo-500" />
                <span>⚠️ Spam suspect</span>
              </label>
              <label class="flex items-center gap-2 p-1.5 rounded hover:bg-slate-800 cursor-pointer text-slate-200">
                <input type="checkbox" value="PERSONAL" v-model="store.selectedTags" class="rounded border-slate-700 bg-slate-800 text-indigo-500" />
                <span>👤 Messagerie Personnelle</span>
              </label>
            </div>
          </div>

          <!-- Barre de Recherche -->
          <div class="relative w-56">
            <Search class="w-4 h-4 absolute left-3 top-2.5 text-slate-500" />
            <input
              v-model="store.searchQuery"
              type="text"
              placeholder="Filtrer..."
              class="w-full bg-slate-900 border border-slate-700/60 rounded-lg pl-9 pr-3 py-1.5 text-xs text-slate-100 placeholder-slate-500 focus:outline-none focus:border-indigo-500"
            />
          </div>

          <!-- Bouton Chasse aux Orphelins -->
          <button
            @click="openOrphanSpamModal"
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-rose-500/15 hover:bg-rose-500/25 text-rose-300 border border-rose-500/30 transition-colors shadow-sm shrink-0"
            title="Analyser et purger les domaines éphémères (longue traîne)"
          >
            <Target class="w-3.5 h-3.5 text-rose-400" />
            <span>Chasse aux Orphelins</span>
          </button>
        </div>
      </header>

      <!-- Bandeau de Progression de Synchronisation IMAP en Direct -->
      <div
        v-if="store.isSyncing"
        class="bg-sky-950/60 border-b border-sky-500/30 px-4 py-3 flex items-center justify-between gap-4 animate-in fade-in slide-in-from-top-2 duration-200"
      >
        <div class="flex items-center gap-3 min-w-0">
          <div class="w-8 h-8 rounded-lg bg-sky-500/20 border border-sky-500/30 flex items-center justify-center text-sky-400 shrink-0">
            <Loader2 class="w-4 h-4 animate-spin" />
          </div>
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <span class="text-xs font-semibold text-sky-200">
                Synchronisation en cours
              </span>
              <span
                v-if="store.scanProgress?.folder"
                class="px-2 py-0.5 rounded-full text-[10px] font-mono bg-sky-900/60 text-sky-300 border border-sky-700/40 truncate"
              >
                {{ store.scanProgress.folder }}
              </span>
            </div>
            <p class="text-xs text-sky-300/80 truncate">
              {{ store.scanProgress?.message || "Analyse et indexation des emails..." }}
            </p>
          </div>
        </div>

        <div class="flex items-center gap-3 shrink-0">
          <div class="text-right">
            <div class="text-xs font-mono font-medium text-sky-200">
              <span v-if="store.scanProgress && store.scanProgress.total > 0">
                {{ store.scanProgress.scanned }} / {{ store.scanProgress.total }} emails
                ({{ Math.round((store.scanProgress.scanned / store.scanProgress.total) * 100) }}%)
              </span>
              <span v-else>
                Initialisation...
              </span>
            </div>
            <div class="w-44 bg-slate-900 rounded-full h-2 mt-1 overflow-hidden border border-sky-900/50">
              <div
                class="bg-gradient-to-r from-sky-500 to-indigo-500 h-2 rounded-full transition-all duration-300"
                :style="{
                  width: store.scanProgress && store.scanProgress.total > 0
                    ? Math.min(100, (store.scanProgress.scanned / store.scanProgress.total) * 100) + '%'
                    : '40%'
                }"
              ></div>
            </div>
          </div>

          <button
            @click="store.isSyncing = false"
            class="text-slate-400 hover:text-slate-200 p-1.5 rounded-lg hover:bg-slate-800/60 transition shrink-0"
            title="Arrêter l'affichage"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <!-- Bandeau d'Erreur de Synchronisation -->
      <div
        v-if="!store.isSyncing && store.scanProgress?.phase === 'error'"
        class="bg-red-950/60 border-b border-red-500/30 px-4 py-3 flex items-center justify-between gap-4 animate-in fade-in"
      >
        <div class="flex items-center gap-3 min-w-0">
          <div class="w-8 h-8 rounded-lg bg-red-500/20 border border-red-500/30 flex items-center justify-center text-red-400 shrink-0">
            <AlertCircle class="w-4 h-4" />
          </div>
          <div class="min-w-0">
            <span class="text-xs font-semibold text-red-200">Échec de synchronisation</span>
            <p class="text-xs text-red-300/80 truncate">
              {{ store.scanProgress?.message || 'Erreur lors de la synchronisation' }}
            </p>
          </div>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <button
            v-if="store.currentAccountId && store.currentAccountId !== 1"
            @click="isPasswordModalOpen = true"
            class="px-2.5 py-1 bg-sky-900/60 hover:bg-sky-800 text-sky-200 border border-sky-700/50 rounded-lg text-xs font-medium transition"
          >
            Saisir le mot de passe
          </button>
          <button
            v-if="store.currentAccountId"
            @click="accountToDelete = store.currentAccount"
            class="px-2.5 py-1 bg-rose-950/60 hover:bg-rose-900 text-rose-300 border border-rose-800/50 rounded-lg text-xs font-medium transition"
          >
            Supprimer le compte
          </button>
          <button
            @click="store.syncCurrentAccount"
            class="px-2.5 py-1 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 rounded-lg text-xs font-medium transition"
          >
            Réessayer
          </button>
        </div>
      </div>

      <!-- Liste des Domaines -->
      <div class="flex-1 overflow-y-auto divide-y divide-slate-800/60">
        <div
          v-for="domain in store.filteredDomainStats"
          :key="domain.root_domain"
          @click="store.selectDomain(domain.root_domain)"
          :class="[
            'p-4 cursor-pointer transition-colors flex items-center justify-between',
            store.selectedDomain === domain.root_domain
              ? 'bg-slate-900/90 border-l-4 border-indigo-500'
              : 'hover:bg-slate-900/40'
          ]"
        >
          <!-- Domaine et expéditeur -->
          <div class="flex items-center space-x-3 min-w-0">
            <div class="w-9 h-9 rounded-lg bg-slate-800 border border-slate-700 flex items-center justify-center font-bold text-xs uppercase text-slate-300">
              {{ domain.root_domain.slice(0, 2) }}
            </div>
            <div class="min-w-0">
              <div class="flex items-center space-x-2">
                <span class="font-semibold text-slate-200 text-sm truncate">{{ domain.root_domain }}</span>
                <span
                  v-if="domain.is_personal_domain"
                  class="px-1.5 py-0.5 rounded text-[10px] font-medium bg-sky-950/80 text-sky-300 border border-sky-800/60 flex items-center space-x-1 shrink-0"
                  title="Fournisseur d'accès ou messagerie personnelle (Sanctuarisé par défaut)"
                >
                  <User class="w-3 h-3 text-sky-400" />
                  <span>Messagerie Personnelle</span>
                </span>
                <span
                  v-if="domain.list_count && domain.list_count > 0"
                  class="px-1.5 py-0.5 rounded text-[10px] font-medium bg-indigo-950/80 text-indigo-300 border border-indigo-800/60 shrink-0"
                  title="Nombre de listes d'envoi ou désabonnements distincts"
                >
                  {{ domain.list_count }} liste{{ domain.list_count > 1 ? 's' : '' }}
                </span>
                <span
                  v-if="domain.is_suspicious_spam"
                  class="px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-950/80 text-amber-300 border border-amber-800/60 flex items-center space-x-1 shrink-0"
                  title="TLD ou motifs d'envoi suspects (spam potentiel)"
                >
                  <AlertTriangle class="w-3 h-3 text-amber-400" />
                  <span>Spam suspect</span>
                </span>
                <span
                  v-if="domain.orphan_category"
                  :class="[
                    'px-1.5 py-0.5 rounded text-[10px] font-medium border flex items-center space-x-1 shrink-0',
                    domain.orphan_category === 'DEAD_DOMAIN'
                      ? 'bg-slate-800 text-slate-300 border-slate-600'
                      : domain.orphan_category === 'SERVER_SPAM' || domain.orphan_category === 'SERVER_SPAM_VERDICT'
                        ? 'bg-rose-950/80 text-rose-300 border-rose-800/60'
                        : domain.orphan_category === 'STRUCTURAL_ANOMALY'
                          ? 'bg-amber-950/80 text-amber-300 border-amber-800/60'
                          : 'bg-orange-950/80 text-orange-300 border-orange-800/60'
                  ]"
                  :title="'Catégorie orphelin : ' + domain.orphan_category"
                >
                  <Skull v-if="domain.orphan_category === 'DEAD_DOMAIN'" class="w-3 h-3 text-slate-400" />
                  <ShieldAlert v-else-if="domain.orphan_category === 'SERVER_SPAM' || domain.orphan_category === 'SERVER_SPAM_VERDICT'" class="w-3 h-3 text-rose-400" />
                  <AlertTriangle v-else-if="domain.orphan_category === 'STRUCTURAL_ANOMALY'" class="w-3 h-3 text-amber-400" />
                  <Flame v-else class="w-3 h-3 text-orange-400" />
                  <span>
                    {{
                      domain.orphan_category === 'DEAD_DOMAIN'
                        ? '💀 Domaine Mort'
                        : domain.orphan_category === 'SERVER_SPAM' || domain.orphan_category === 'SERVER_SPAM_VERDICT'
                          ? '🛡️ Rejet Auth'
                          : domain.orphan_category === 'STRUCTURAL_ANOMALY'
                            ? '⚠️ Anomalie'
                            : '🚫 Jetable'
                    }}
                  </span>
                </span>
                <span class="text-[11px] text-slate-500">({{ domain.total_emails }} mails)</span>
              </div>
              <div class="flex items-center space-x-2 text-xs mt-1">
                <span class="text-slate-400">Total : {{ formatBytes(domain.total_size) }}</span>
                <span v-if="domain.last_received_at" class="text-slate-500">•</span>
                <span v-if="domain.last_received_at" class="text-slate-400" :title="formatDate(domain.last_received_at)">
                  Dernier : {{ formatRelativeDate(domain.last_received_at) }}
                </span>
              </div>
            </div>
          </div>

          <!-- Statistiques & Badges -->
          <div class="flex items-center space-x-3 shrink-0">
            <!-- Purgeable Badge -->
            <div class="text-right">
              <div class="text-xs font-semibold text-rose-400 font-mono">
                {{ formatBytes(domain.purgeable_size) }}
              </div>
              <div class="text-[11px] text-slate-400">
                {{ domain.purgeable_count }} purgeables > 60j
              </div>
            </div>

            <!-- Protected Shield Badge -->
            <div
              v-if="domain.protected_count > 0"
              class="flex items-center space-x-1 px-2 py-1 rounded bg-emerald-950/60 text-emerald-300 border border-emerald-800/40 text-[11px]"
              title="Emails sanctuarisés (Factures, flags, protection manuelle)"
            >
              <ShieldCheck class="w-3.5 h-3.5" />
              <span>{{ domain.protected_count }} protégé{{ domain.protected_count > 1 ? 's' : '' }}</span>
            </div>

            <!-- Action Dé-taguer / Faire Confiance -->
            <button
              v-if="domain.orphan_category"
              @click.stop="unmarkDomainWithFeedback(domain.root_domain)"
              class="flex items-center space-x-1 px-2 py-1 rounded-lg text-xs font-medium text-emerald-300 bg-emerald-950/40 hover:bg-emerald-900/50 border border-emerald-800/40 transition shrink-0"
              title="Retirer l'alerte et marquer ce domaine comme de confiance (ne sera plus classé en spam)"
            >
              <ShieldCheck class="w-3.5 h-3.5 text-emerald-400" />
              <span class="text-[11px]">Faire confiance</span>
            </button>

            <!-- Action Purger tout le domaine -->
            <button
              @click.stop="openForcePurgeModal(domain)"
              class="p-1.5 rounded-lg text-slate-500 hover:text-rose-400 hover:bg-rose-950/40 border border-transparent hover:border-rose-800/40 transition shrink-0"
              title="Purger définitivement TOUS les emails de ce domaine (y compris protégés)"
            >
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        <div
          v-if="store.filteredDomainStats.length === 0 && !store.isSyncing"
          class="p-16 text-center space-y-4"
        >
          <div
            class="w-12 h-12 rounded-2xl bg-slate-900 border border-slate-800 flex items-center justify-center text-slate-400 mx-auto"
          >
            <Inbox class="w-6 h-6 text-slate-500" />
          </div>
          <div class="space-y-1">
            <h3 class="text-sm font-semibold text-slate-300">
              {{ store.accounts.length === 0 ? "Aucun compte connecté" : "Aucun email synchronisé" }}
            </h3>
            <p class="text-xs text-slate-500 max-w-sm mx-auto leading-relaxed">
              {{ store.accounts.length === 0 ? "Ajoutez une boîte de messagerie ou lancez la démonstration pour commencer." : "Cette boîte de messagerie n'a pas encore été analysée ou ne contient aucun email en cache local." }}
            </p>
          </div>
          <button
            v-if="store.accounts.length > 0"
            @click="store.syncCurrentAccount"
            class="inline-flex items-center gap-2 px-4 py-2.5 bg-sky-500 hover:bg-sky-400 text-slate-950 font-semibold rounded-xl text-xs transition shadow-lg shadow-sky-500/10"
          >
            <RefreshCw class="w-3.5 h-3.5" />
            <span>Lancer la synchronisation</span>
          </button>
          <div v-else class="flex items-center justify-center gap-2">
            <button
              @click="isAddAccountModalOpen = true"
              class="inline-flex items-center gap-2 px-4 py-2.5 bg-sky-500 hover:bg-sky-400 text-slate-950 font-semibold rounded-xl text-xs transition shadow-lg shadow-sky-500/10"
            >
              <Plus class="w-3.5 h-3.5" />
              <span>Ajouter un compte</span>
            </button>
            <button
              @click="store.loadDemoData"
              class="inline-flex items-center gap-2 px-4 py-2.5 bg-indigo-600/30 hover:bg-indigo-600/40 text-indigo-300 border border-indigo-500/40 font-semibold rounded-xl text-xs transition"
            >
              <Sparkles class="w-3.5 h-3.5" />
              <span>Charger la démo</span>
            </button>
          </div>
        </div>
      </div>
    </main>

    <!-- ======================================================== -->
    <!-- PANNEAU 3 : INSPECTEUR / FILET DE SÉCURITÉ (Droite)      -->
    <!-- ======================================================== -->
    <aside class="w-96 bg-slate-900 flex flex-col justify-between shrink-0">
      <!-- En-tête de l'Inspecteur -->
      <div class="p-4 border-b border-slate-800 flex items-center justify-between">
        <div>
          <div class="flex items-center space-x-2">
            <h3 class="font-semibold text-white text-sm">
              {{ store.selectedDomain || "Sélectionnez un domaine" }}
            </h3>
            <span
              v-if="store.currentDomainStats?.is_personal_domain"
              class="px-1.5 py-0.5 rounded text-[10px] bg-sky-950 text-sky-300 border border-sky-800/60"
            >
              Messagerie Personnelle
            </span>
          </div>
          <p class="text-xs text-slate-400">Inspection détaillée & filet de sécurité</p>
        </div>
      </div>

      <!-- Bannière d'information pour domaines personnels -->
      <div
        v-if="store.currentDomainStats?.is_personal_domain"
        class="mx-3 mt-3 p-2 rounded-lg bg-sky-950/40 border border-sky-800/40 text-[11px] text-sky-200 flex items-start space-x-2"
      >
        <ShieldCheck class="w-4 h-4 text-sky-400 shrink-0 mt-0.5" />
        <span class="leading-tight">
          Ce domaine est un fournisseur de messagerie personnelle : les correspondances privées sont sanctuarisées par défaut contre la purge automatique.
        </span>
      </div>

      <!-- Onglets : Candidats vs Listes vs Protégés -->
      <div class="flex border-b border-slate-800 text-xs">
        <button
          @click="store.activeTab = 'purgeable'"
          :class="[
            'flex-1 py-2.5 text-center font-medium border-b-2 transition flex items-center justify-center space-x-1',
            store.activeTab === 'purgeable'
              ? 'border-indigo-500 text-white bg-slate-800/40'
              : 'border-transparent text-slate-400 hover:text-slate-200'
          ]"
        >
          <span>Candidats</span>
          <span class="px-1.5 py-0.2 rounded-full text-[10px] bg-slate-800 font-mono">
            {{ store.purgeableEmails.length }}
          </span>
        </button>

        <button
          @click="store.activeTab = 'subscriptions'"
          :class="[
            'flex-1 py-2.5 text-center font-medium border-b-2 transition flex items-center justify-center space-x-1',
            store.activeTab === 'subscriptions'
              ? 'border-purple-500 text-white bg-slate-800/40'
              : 'border-transparent text-slate-400 hover:text-slate-200'
          ]"
        >
          <BellRing class="w-3.5 h-3.5 text-purple-400" />
          <span>Listes</span>
          <span class="px-1.5 py-0.2 rounded-full text-[10px] bg-slate-800 font-mono">
            {{ store.distributionLists.length }}
          </span>
        </button>

        <button
          @click="store.activeTab = 'protected'"
          :class="[
            'flex-1 py-2.5 text-center font-medium border-b-2 transition flex items-center justify-center space-x-1',
            store.activeTab === 'protected'
              ? 'border-emerald-500 text-white bg-slate-800/40'
              : 'border-transparent text-slate-400 hover:text-slate-200'
          ]"
        >
          <ShieldCheck class="w-3.5 h-3.5 text-emerald-400" />
          <span>Protégés</span>
          <span class="px-1.5 py-0.2 rounded-full text-[10px] bg-slate-800 font-mono">
            {{ store.protectedEmails.length }}
          </span>
        </button>
      </div>

      <!-- Contenu de la liste -->
      <div class="flex-1 overflow-y-auto p-3 space-y-2">
        <!-- Barre de sélection globale pour l'onglet purgeable -->
        <div
          v-if="store.activeTab === 'purgeable' && store.purgeableEmails.length > 0"
          class="flex items-center justify-between pb-2 mb-1 border-b border-slate-800 text-xs text-slate-400"
        >
          <button
            @click="store.toggleSelectAllPurgeable"
            class="flex items-center space-x-2 text-indigo-400 hover:text-indigo-300"
          >
            <component :is="store.allPurgeableSelected ? CheckSquare : Square" class="w-4 h-4" />
            <span>Tout sélectionner ({{ store.selectedUidsToPurge.size }} / {{ store.purgeableEmails.length }})</span>
          </button>

          <button
            @click="handleReclassifyDomain"
            :disabled="store.isLoading"
            class="px-2.5 py-1 rounded text-xs font-medium bg-slate-800 hover:bg-slate-700 text-slate-300 flex items-center space-x-1.5 transition disabled:opacity-50"
            title="Forcer la réévaluation automatique de tous les emails du domaine selon les règles de protection"
          >
            <RotateCcw class="w-3.5 h-3.5 text-sky-400" />
            <span>Réévaluer les règles</span>
          </button>
        </div>

        <!-- Liste des emails candidats purgeables -->
        <div v-if="store.activeTab === 'purgeable'" class="space-y-2">
          <div
            v-for="email in store.purgeableEmails"
            :key="email.uid"
            :class="[
              'p-2.5 rounded-lg border transition-all flex flex-col space-y-2',
              store.selectedUidsToPurge.has(email.uid)
                ? 'bg-indigo-950/20 border-indigo-500/40 text-slate-100'
                : 'bg-slate-900/60 border-slate-800 text-slate-400 hover:border-slate-700'
            ]"
          >
            <div class="flex items-start space-x-2.5">
              <button
                @click.stop="store.toggleSelectUid(email.uid)"
                class="mt-0.5 shrink-0 focus:outline-none"
              >
                <component
                  :is="store.selectedUidsToPurge.has(email.uid) ? CheckSquare : Square"
                  :class="['w-4 h-4', store.selectedUidsToPurge.has(email.uid) ? 'text-indigo-400' : 'text-slate-600']"
                />
              </button>

              <div class="min-w-0 flex-1 cursor-pointer" @click="store.openPreview(email)">
                <div class="font-medium text-xs text-slate-200 line-clamp-1 hover:text-indigo-300 transition">
                  {{ email.subject }}
                </div>
                <div class="flex items-center justify-between text-[11px] text-slate-400 mt-1">
                  <span class="truncate">{{ email.sender_address }}</span>
                  <span class="font-mono shrink-0 ml-2">{{ formatBytes(email.size_bytes) }}</span>
                </div>
                <div class="flex items-center space-x-2 text-[10px] text-slate-400 mt-1">
                  <span class="flex items-center space-x-1">
                    <Inbox class="w-3 h-3" />
                    <span>{{ email.folder_path }}</span>
                  </span>
                  <span>•</span>
                  <span>{{ formatDate(email.received_at) }}</span>
                </div>
              </div>
            </div>

            <!-- Actions sur l'email : Aperçu et Basculer vers Protégé -->
            <div class="flex items-center justify-end space-x-2 pt-1 border-t border-slate-800/60 text-xs">
              <button
                @click.stop="store.openPreview(email)"
                class="px-2 py-0.5 rounded text-[11px] bg-slate-800 text-slate-300 hover:bg-slate-700 flex items-center space-x-1 transition"
                title="Prévisualiser cet email"
              >
                <Eye class="w-3 h-3 text-indigo-400" />
                <span>Aperçu</span>
              </button>

              <button
                @click.stop="store.toggleEmailProtection(email)"
                class="px-2 py-0.5 rounded text-[11px] bg-emerald-950/70 text-emerald-300 border border-emerald-800/40 hover:bg-emerald-900/60 flex items-center space-x-1 transition"
                title="Basculer vers les protégés (ne jamais supprimer)"
              >
                <Shield class="w-3 h-3 text-emerald-400" />
                <span>Protéger</span>
              </button>
            </div>
          </div>

          <div v-if="store.purgeableEmails.length === 0" class="p-8 text-center text-slate-500 text-xs">
            Aucun email purgeable pour ce domaine.
          </div>
        </div>

        <!-- Onglet : Listes & Abonnements -->
        <div v-if="store.activeTab === 'subscriptions'" class="space-y-3">
          <div
            v-for="list in store.distributionLists"
            :key="list.list_id"
            class="p-3 rounded-lg bg-slate-900/80 border border-slate-800 space-y-2.5 shadow-sm"
          >
            <!-- En-tête de la liste / badges de méthode RFC -->
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <div class="font-semibold text-xs text-slate-100 truncate" :title="list.list_name">
                  {{ list.list_name }}
                </div>
                <div class="text-[11px] text-slate-400 truncate mt-0.5" :title="list.sender_address">
                  {{ list.sender_address }}
                </div>
                <div v-if="list.last_received_at" class="text-[10px] text-slate-400 mt-0.5" :title="formatDate(list.last_received_at)">
                  Dernier message : {{ formatRelativeDate(list.last_received_at) }} ({{ formatDate(list.last_received_at) }})
                </div>
              </div>

              <!-- Badge Méthode Détectée -->
              <div class="shrink-0">
                <span
                  v-if="list.method === 'ONE_CLICK_POST'"
                  class="px-2 py-0.5 rounded text-[10px] font-medium bg-emerald-950/70 text-emerald-300 border border-emerald-800/60 flex items-center space-x-1"
                  title="Désinscription directe en 1-clic en arrière-plan (RFC 8058)"
                >
                  <Zap class="w-3 h-3 text-emerald-400 fill-emerald-400" />
                  <span>1-Clic Direct</span>
                </span>
                <span
                  v-else-if="list.method === 'MAILTO'"
                  class="px-2 py-0.5 rounded text-[10px] font-medium bg-sky-950/70 text-sky-300 border border-sky-800/60 flex items-center space-x-1"
                  title="Envoi d'un email de désabonnement via votre client mail (RFC 2369)"
                >
                  <Send class="w-3 h-3 text-sky-400" />
                  <span>Email mailto</span>
                </span>
                <span
                  v-else-if="list.method === 'WEB_URL'"
                  class="px-2 py-0.5 rounded text-[10px] font-medium bg-amber-950/70 text-amber-300 border border-amber-800/60 flex items-center space-x-1"
                  title="Ouverture de la page web de gestion des préférences"
                >
                  <ExternalLink class="w-3 h-3 text-amber-400" />
                  <span>Lien Web</span>
                </span>
              </div>
            </div>

            <!-- Volume et identifiant -->
            <div class="flex items-center justify-between text-[11px] text-slate-400 pt-1 border-t border-slate-800/50">
              <span>{{ list.email_count }} email{{ list.email_count > 1 ? 's' : '' }}</span>
              <span class="font-mono text-slate-300">{{ formatBytes(list.total_size) }}</span>
            </div>

            <!-- Boutons d'action -->
            <div v-if="list.method !== 'NONE'" class="flex flex-col space-y-1.5 pt-1">
              <!-- Action principale : Désabonner + Corbeille -->
              <button
                @click="store.unsubscribeAndPurge(list.list_id, true)"
                :disabled="store.isPurging"
                class="w-full px-2.5 py-1.5 rounded text-xs font-medium bg-rose-600/20 hover:bg-rose-600/30 text-rose-300 border border-rose-500/30 flex items-center justify-center space-x-1.5 transition disabled:opacity-50"
              >
                <Trash2 class="w-3.5 h-3.5 text-rose-400" />
                <span>Se désabonner + Mettre en corbeille</span>
              </button>

              <!-- Action secondaire : Désabonner uniquement -->
              <button
                @click="store.unsubscribeAndPurge(list.list_id, false)"
                :disabled="store.isPurging"
                class="w-full px-2.5 py-1 rounded text-[11px] bg-slate-800 hover:bg-slate-700 text-slate-300 flex items-center justify-center space-x-1 transition disabled:opacity-50"
              >
                <BellRing class="w-3 h-3 text-slate-400" />
                <span>Se désabonner uniquement</span>
              </button>
            </div>

            <div v-else class="p-2 rounded bg-amber-950/30 border border-amber-800/40 text-[11px] text-amber-300/90 flex items-center space-x-1.5">
              <AlertCircle class="w-3.5 h-3.5 text-amber-400 shrink-0" />
              <span>Aucun mécanisme de désabonnement standard (RFC 8058 / RFC 2369) détecté.</span>
            </div>
          </div>

          <div v-if="store.distributionLists.length === 0" class="p-8 text-center text-slate-500 text-xs">
            Aucune liste de diffusion détectée pour ce domaine.
          </div>
        </div>

        <!-- Liste des emails protégés -->
        <div v-if="store.activeTab === 'protected'" class="space-y-2">
          <!-- Barre d'action tout déprotéger -->
          <div
            v-if="store.protectedEmails.length > 0"
            class="flex items-center justify-between pb-2 mb-1 border-b border-slate-800 text-xs text-slate-400"
          >
            <span class="font-medium text-slate-300">{{ store.protectedEmails.length }} email(s) protégé(s)</span>
            <button
              @click="isConfirmUnprotectModalOpen = true"
              :disabled="store.isLoading"
              class="px-2.5 py-1 rounded text-xs font-medium bg-amber-950/50 hover:bg-amber-900/60 text-amber-300 border border-amber-800/50 flex items-center space-x-1.5 transition disabled:opacity-50"
              title="Reclasser tous les emails protégés de ce domaine comme candidats à la purge"
            >
              <ShieldAlert class="w-3.5 h-3.5 text-amber-400" />
              <span>Tout déprotéger</span>
            </button>
          </div>

          <div
            v-for="email in store.protectedEmails"
            :key="email.uid"
            class="p-2.5 rounded-lg bg-slate-900/60 border border-slate-800/80 text-slate-300 space-y-2"
          >
            <div class="flex items-center justify-between">
              <span
                v-if="email.classification?.toUpperCase() === 'PROTECTED_INVOICE'"
                class="px-2 py-0.5 rounded bg-emerald-950 text-emerald-300 border border-emerald-800/50 text-[10px] flex items-center space-x-1"
              >
                <FileText class="w-3 h-3" />
                <span>Facture / Commande (PDF)</span>
              </span>
              <span
                v-else-if="email.classification?.toUpperCase() === 'PROTECTED_FLAGGED'"
                class="px-2 py-0.5 rounded bg-amber-950 text-amber-300 border border-amber-800/50 text-[10px] flex items-center space-x-1"
              >
                <Star class="w-3 h-3 text-amber-400" />
                <span>Email Étoilé / Marqué</span>
              </span>
              <span
                v-else-if="email.classification?.toUpperCase() === 'PROTECTED_USER'"
                class="px-2 py-0.5 rounded bg-purple-950 text-purple-300 border border-purple-800/50 text-[10px] flex items-center space-x-1"
              >
                <ShieldCheck class="w-3 h-3 text-purple-400" />
                <span>Protection Manuelle</span>
              </span>
              <span
                v-else-if="email.classification?.toUpperCase() === 'PROTECTED_PERSONAL'"
                class="px-2 py-0.5 rounded bg-sky-950 text-sky-300 border border-sky-800/50 text-[10px] flex items-center space-x-1"
              >
                <User class="w-3 h-3 text-sky-400" />
                <span>Messagerie Personnelle</span>
              </span>
              <span
                v-else-if="email.classification?.toUpperCase() === 'RECENT'"
                class="px-2 py-0.5 rounded bg-blue-950 text-blue-300 border border-blue-800/50 text-[10px] flex items-center space-x-1"
              >
                <Clock class="w-3 h-3" />
                <span>Récent (&lt; 60 jours)</span>
              </span>
              <span class="text-[11px] font-mono text-slate-500">{{ formatBytes(email.size_bytes) }}</span>
            </div>

            <div
              class="font-medium text-xs text-slate-200 line-clamp-1 cursor-pointer hover:text-indigo-300 transition"
              @click="store.openPreview(email)"
            >
              {{ email.subject }}
            </div>

            <div class="flex items-center justify-between text-[11px] text-slate-400">
              <span class="truncate">{{ email.sender_address }}</span>
              <span class="font-mono text-slate-500 shrink-0 ml-2">{{ formatDate(email.received_at) }}</span>
            </div>

            <div v-if="email.attachment_names.length > 0" class="text-[11px] text-emerald-400 flex items-center space-x-1">
              <FileText class="w-3 h-3" />
              <span>{{ email.attachment_names.join(', ') }}</span>
            </div>

            <!-- Actions sur l'email protégé : Aperçu et Rendre purgeable -->
            <div class="flex items-center justify-end space-x-2 pt-1 border-t border-slate-800/60 text-xs">
              <button
                @click.stop="store.openPreview(email)"
                class="px-2 py-0.5 rounded text-[11px] bg-slate-800 text-slate-300 hover:bg-slate-700 flex items-center space-x-1 transition"
                title="Prévisualiser cet email"
              >
                <Eye class="w-3 h-3 text-indigo-400" />
                <span>Aperçu</span>
              </button>

              <button
                @click.stop="store.toggleEmailProtection(email)"
                class="px-2 py-0.5 rounded text-[11px] bg-rose-950/70 text-rose-300 border border-rose-800/40 hover:bg-rose-900/60 flex items-center space-x-1 transition"
                title="Basculer vers les candidats corbeille"
              >
                <Trash2 class="w-3 h-3 text-rose-400" />
                <span>Rendre purgeable</span>
              </button>
            </div>
          </div>

          <div v-if="store.protectedEmails.length === 0" class="p-8 text-center text-slate-500 text-xs">
            Aucun email protégé pour ce domaine.
          </div>
        </div>
      </div>

      <!-- Action Footer : Dry-run & Mettre en corbeille -->
      <div class="p-4 border-t border-slate-800 bg-slate-900/90 space-y-3">
        <!-- Toggle Dry Run -->
        <div class="flex items-center justify-between text-xs">
          <label for="dry-run-toggle" class="text-slate-300 flex items-center space-x-1.5 cursor-pointer">
            <span class="font-medium">Mode Simulation (Dry-Run)</span>
          </label>
          <input
            id="dry-run-toggle"
            v-model="store.dryRunEnabled"
            type="checkbox"
            class="rounded bg-slate-800 border-slate-700 text-indigo-600 focus:ring-indigo-500 w-4 h-4 cursor-pointer"
          />
        </div>

        <!-- Bouton de Purge -->
        <button
          @click="store.executePurge"
          :disabled="store.isPurging || store.selectedUidsToPurge.size === 0"
          :class="[
            'w-full py-2.5 px-4 rounded-lg font-medium text-xs flex items-center justify-center space-x-2 transition shadow-lg disabled:opacity-50',
            store.dryRunEnabled
              ? 'bg-amber-600 hover:bg-amber-500 text-white shadow-amber-900/30'
              : 'bg-rose-600 hover:bg-rose-500 text-white shadow-rose-900/30'
          ]"
        >
          <component :is="store.dryRunEnabled ? AlertCircle : Trash2" class="w-4 h-4" />
          <span>
            {{ store.dryRunEnabled ? 'Simuler la mise en corbeille' : 'Mettre en corbeille' }}
            ({{ store.selectedUidsToPurge.size }})
          </span>
        </button>

        <!-- Notification de statut -->
        <div
          v-if="store.statusMessage"
          :class="[
            'p-2.5 rounded-lg border text-xs flex items-start space-x-2 transition',
            store.statusMessage.toLowerCase().includes('erreur') || store.statusMessage.toLowerCase().includes('échec')
              ? 'bg-rose-950/70 border-rose-800/60 text-rose-200'
              : 'bg-slate-800/80 border-slate-700 text-slate-300'
          ]"
        >
          <component
            :is="store.statusMessage.toLowerCase().includes('erreur') || store.statusMessage.toLowerCase().includes('échec') ? AlertCircle : CheckCircle2"
            :class="[
              'w-4 h-4 shrink-0 mt-0.5',
              store.statusMessage.toLowerCase().includes('erreur') || store.statusMessage.toLowerCase().includes('échec')
                ? 'text-rose-400'
                : 'text-emerald-400'
            ]"
          />
          <span class="break-words leading-relaxed flex-1">{{ store.statusMessage }}</span>
        </div>
      </div>
    </aside>

    <!-- ======================================================== -->
    <!-- MODAL : PRÉVISUALISATION D'UN EMAIL                      -->
    <!-- ======================================================== -->
    <div
      v-if="store.previewedEmail"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-4"
      @click.self="store.closePreview"
    >
      <div class="bg-slate-900 border border-slate-700 rounded-xl max-w-2xl w-full max-h-[85vh] flex flex-col shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-150">
        <!-- Modal Header -->
        <div class="p-4 border-b border-slate-800 flex items-start justify-between bg-slate-950/50">
          <div class="min-w-0 pr-4">
            <div class="flex items-center space-x-2 mb-1">
              <span
                v-if="store.previewedEmail.classification?.toUpperCase() === 'PURGEABLE'"
                class="px-2 py-0.5 rounded bg-rose-950 text-rose-300 border border-rose-800/50 text-[10px]"
              >
                Candidat Corbeille
              </span>
              <span
                v-else-if="store.previewedEmail.classification?.toUpperCase() === 'PROTECTED_INVOICE'"
                class="px-2 py-0.5 rounded bg-emerald-950 text-emerald-300 border border-emerald-800/50 text-[10px]"
              >
                Facture / Reçu protégé (PDF)
              </span>
              <span
                v-else-if="store.previewedEmail.classification?.toUpperCase() === 'PROTECTED_FLAGGED'"
                class="px-2 py-0.5 rounded bg-amber-950 text-amber-300 border border-amber-800/50 text-[10px]"
              >
                Message Étoilé
              </span>
              <span
                v-else-if="store.previewedEmail.classification?.toUpperCase() === 'PROTECTED_USER'"
                class="px-2 py-0.5 rounded bg-purple-950 text-purple-300 border border-purple-800/50 text-[10px]"
              >
                Protégé Manuellement
              </span>
              <span
                v-else-if="store.previewedEmail.classification?.toUpperCase() === 'PROTECTED_PERSONAL'"
                class="px-2 py-0.5 rounded bg-sky-950 text-sky-300 border border-sky-800/50 text-[10px]"
              >
                Messagerie Personnelle (FAI / Webmail)
              </span>
              <span
                v-else-if="store.previewedEmail.classification?.toUpperCase() === 'RECENT'"
                class="px-2 py-0.5 rounded bg-blue-950 text-blue-300 border border-blue-800/50 text-[10px]"
              >
                Récent (&lt; 60j)
              </span>
              <span class="text-xs text-slate-500 font-mono">
                UID: {{ store.previewedEmail.uid }}
              </span>
            </div>
            <h3 class="font-bold text-base text-white leading-snug">
              {{ store.previewedEmail.subject }}
            </h3>
          </div>

          <button
            @click="store.closePreview"
            class="p-1 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white transition"
          >
            <X class="w-5 h-5" />
          </button>
        </div>

        <!-- Modal Meta details -->
        <div class="px-5 py-3 bg-slate-900/60 border-b border-slate-800 text-xs text-slate-300 space-y-1.5">
          <div class="flex items-center justify-between">
            <div>
              <span class="text-slate-500 mr-2">De :</span>
              <span class="font-medium text-slate-200">{{ store.previewedEmail.sender_address }}</span>
            </div>
            <div class="font-mono text-slate-400 text-[11px]">
              {{ formatBytes(store.previewedEmail.size_bytes) }}
            </div>
          </div>

          <div class="flex items-center justify-between text-[11px] text-slate-400">
            <div class="flex items-center space-x-2">
              <span class="text-slate-500">Dossier :</span>
              <span class="text-slate-300 font-medium">{{ store.previewedEmail.folder_path }}</span>
            </div>
            <div>
              {{ formatDate(store.previewedEmail.received_at) }}
            </div>
          </div>

          <!-- Attachments -->
          <div
            v-if="store.previewedEmail.attachment_names.length > 0"
            class="pt-1 flex items-center space-x-2 text-[11px] text-emerald-400"
          >
            <span class="text-slate-500">Pièces jointes :</span>
            <span
              v-for="att in store.previewedEmail.attachment_names"
              :key="att"
              class="px-2 py-0.5 rounded bg-emerald-950/60 border border-emerald-800/40 text-[11px] flex items-center space-x-1"
            >
              <FileText class="w-3 h-3" />
              <span>{{ att }}</span>
            </span>
          </div>

          <!-- Unsubscribe header badge in preview -->
          <div
            v-if="store.previewedEmail.has_unsubscribe"
            class="pt-1 flex items-center space-x-1.5 text-[11px] text-purple-300"
          >
            <BellRing class="w-3 h-3 text-purple-400" />
            <span>Cet email provient d'une liste de distribution (en-tête de désinscription détecté).</span>
          </div>
        </div>

        <!-- Modal Body Content -->
        <div class="flex-1 overflow-y-auto p-5 text-xs leading-relaxed text-slate-200 whitespace-pre-wrap font-sans bg-slate-950/30">
          <div
            v-if="store.previewLoading"
            class="flex items-center justify-center py-12 text-slate-400 space-x-2"
          >
            <Loader2 class="w-4 h-4 animate-spin text-indigo-400" />
            <span>Chargement de l'aperçu du message depuis le serveur...</span>
          </div>
          <div
            v-else-if="store.previewError"
            class="p-4 rounded-xl bg-rose-950/40 border border-rose-800/50 text-rose-300 space-y-3"
          >
            <div class="flex items-center space-x-2">
              <AlertCircle class="w-4 h-4 text-rose-400 shrink-0" />
              <span class="font-medium text-xs">Impossible de charger l'aperçu de l'email</span>
            </div>
            <p class="text-[11px] text-rose-300/80 font-mono break-all leading-normal">
              {{ store.previewError }}
            </p>
            <button
              @click="store.openPreview(store.previewedEmail)"
              class="px-3 py-1.5 rounded-lg bg-rose-900/60 hover:bg-rose-800 text-rose-200 text-xs font-medium flex items-center space-x-1.5 transition"
            >
              <RefreshCw class="w-3.5 h-3.5" />
              <span>Réessayer</span>
            </button>
          </div>
          <template v-else>
            <div
              v-if="store.previewedEmail.body_snippet?.startsWith('⚠️')"
              class="p-4 rounded-xl bg-amber-500/10 border border-amber-500/25 text-amber-300 font-sans whitespace-pre-wrap leading-relaxed text-xs"
            >
              {{ store.previewedEmail.body_snippet }}
            </div>
            <div v-else class="whitespace-pre-wrap font-sans text-xs leading-relaxed text-slate-300">
              {{ store.previewedEmail.body_snippet || "(Aucun aperçu disponible pour ce message)" }}
            </div>
          </template>
        </div>

        <!-- Modal Footer Actions -->
        <div class="p-4 border-t border-slate-800 bg-slate-950/60 flex items-center justify-between">
          <div>
            <button
              @click="store.toggleEmailProtection(store.previewedEmail)"
              :class="[
                'px-3 py-1.5 rounded-lg text-xs font-medium flex items-center space-x-1.5 transition',
                store.previewedEmail.classification?.toUpperCase() === 'PURGEABLE'
                  ? 'bg-emerald-900/60 hover:bg-emerald-800 text-emerald-200 border border-emerald-700/50'
                  : 'bg-rose-900/60 hover:bg-rose-800 text-rose-200 border border-rose-700/50'
              ]"
            >
              <component
                :is="store.previewedEmail.classification?.toUpperCase() === 'PURGEABLE' ? ShieldCheck : Trash2"
                class="w-4 h-4"
              />
              <span>
                {{ store.previewedEmail.classification?.toUpperCase() === 'PURGEABLE' ? 'Basculer en Protégé' : 'Rendre Purgeable' }}
              </span>
            </button>
          </div>

          <button
            @click="store.closePreview"
            class="px-4 py-1.5 rounded-lg text-xs font-medium bg-slate-800 hover:bg-slate-700 text-slate-200 transition"
          >
            Fermer
          </button>
        </div>
      </div>
    </div>

    <!-- Modal d'ajout de compte -->
    <AddAccountModal
      :is-open="isAddAccountModalOpen"
      @close="isAddAccountModalOpen = false"
    />

    <!-- Modal de mise à jour du mot de passe -->
    <div
      v-if="isPasswordModalOpen"
      class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4"
    >
      <div
        class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-md shadow-2xl overflow-hidden p-6 space-y-4"
      >
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2">
            <KeyRound class="w-5 h-5 text-sky-400" />
            <h3 class="text-sm font-semibold text-slate-100">
              Identifiants de connexion
            </h3>
          </div>
          <button
            @click="isPasswordModalOpen = false"
            class="text-slate-400 hover:text-slate-200 p-1 rounded-lg hover:bg-slate-800 transition"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <p class="text-xs text-slate-400 leading-relaxed">
          Entrez le mot de passe pour le compte
          <span class="font-medium text-slate-200">{{ store.currentAccount?.email }}</span>.
          Il sera validé auprès du serveur IMAP puis sauvegardé de manière chiffrée dans votre trousseau OS.
        </p>

        <div
          v-if="passwordError"
          class="p-2.5 bg-rose-500/10 border border-rose-500/20 rounded-xl text-xs text-rose-400"
        >
          {{ passwordError }}
        </div>

        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-slate-300">
            Mot de passe de messagerie
          </label>
          <input
            v-model="newPassword"
            type="password"
            placeholder="Mot de passe ou mot de passe d'application"
            class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500 font-mono"
            @keyup.enter="handlePasswordSubmit"
          />
        </div>

        <div class="flex items-center justify-end gap-2 pt-2">
          <button
            @click="isPasswordModalOpen = false"
            class="px-3 py-2 rounded-xl text-xs font-medium bg-slate-800 hover:bg-slate-700 text-slate-300 transition"
          >
            Annuler
          </button>
          <button
            @click="handlePasswordSubmit"
            :disabled="!newPassword || isSubmittingPassword"
            class="px-4 py-2 bg-sky-500 hover:bg-sky-400 disabled:opacity-50 text-slate-950 font-semibold rounded-xl text-xs flex items-center gap-1.5 transition"
          >
            <Loader2 v-if="isSubmittingPassword" class="w-3.5 h-3.5 animate-spin" />
            <span>{{ isSubmittingPassword ? "Vérification..." : "Enregistrer et tester" }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- Modal Confirmation de suppression de compte -->
    <div
      v-if="accountToDelete"
      class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4"
    >
      <div
        class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-sm shadow-2xl p-6 space-y-4 animate-in fade-in zoom-in-95 duration-150"
      >
        <div class="flex items-center gap-3">
          <div
            class="w-10 h-10 rounded-xl bg-rose-500/10 border border-rose-500/20 flex items-center justify-center text-rose-400 shrink-0"
          >
            <Trash2 class="w-5 h-5" />
          </div>
          <div>
            <h3 class="text-sm font-semibold text-slate-100">
              Supprimer le compte
            </h3>
            <p class="text-xs text-slate-400">
              Cette action est irréversible
            </p>
          </div>
        </div>

        <p class="text-xs text-slate-300 leading-relaxed">
          Voulez-vous supprimer le compte
          <strong class="text-white">{{ accountToDelete.display_name }}</strong> ainsi que toutes ses données et son cache local ?
        </p>

        <div class="flex items-center justify-end gap-2 pt-2">
          <button
            @click="accountToDelete = null"
            class="px-3 py-2 rounded-xl text-xs font-medium bg-slate-800 hover:bg-slate-700 text-slate-300 transition"
          >
            Annuler
          </button>
          <button
            @click="executeDeleteAccount"
            :disabled="isDeletingAccount"
            class="px-4 py-2 bg-rose-600 hover:bg-rose-500 disabled:opacity-50 text-white font-semibold rounded-xl text-xs flex items-center gap-1.5 transition shadow-lg shadow-rose-600/20"
          >
            <Loader2 v-if="isDeletingAccount" class="w-3.5 h-3.5 animate-spin" />
            <span>{{ isDeletingAccount ? "Suppression..." : "Supprimer définitivement" }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- Modal Édition de Compte -->
    <EditAccountModal
      :is-open="isEditAccountModalOpen"
      :account="accountToEdit"
      @close="isEditAccountModalOpen = false"
    />

    <!-- Modal Confirmation Force Purge Domaine (y compris protégés) -->
    <div
      v-if="forcePurgeDomain"
      class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4"
    >
      <div
        class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-md shadow-2xl p-6 space-y-4 animate-in fade-in zoom-in-95 duration-150"
      >
        <div class="flex items-center gap-3">
          <div
            class="w-10 h-10 rounded-xl bg-rose-500/10 border border-rose-500/20 flex items-center justify-center text-rose-400 shrink-0"
          >
            <AlertTriangle class="w-5 h-5" />
          </div>
          <div>
            <h3 class="text-sm font-semibold text-slate-100">
              Purger tout le domaine
            </h3>
            <p class="text-xs text-slate-400 font-mono">
              {{ forcePurgeDomain.root_domain }}
            </p>
          </div>
        </div>

        <p class="text-xs text-slate-300 leading-relaxed">
          Cette action va déplacer <strong>l'intégralité des {{ forcePurgeDomain.total_emails }} email(s)</strong> de ce domaine vers la corbeille.
        </p>

        <div
          v-if="forcePurgeDomain.protected_count > 0"
          class="p-3 rounded-xl bg-rose-950/40 border border-rose-800/50 text-xs text-rose-300 space-y-1"
        >
          <div class="font-semibold flex items-center gap-1.5">
            <ShieldAlert class="w-4 h-4 text-rose-400 shrink-0" />
            <span>Attention : {{ forcePurgeDomain.protected_count }} email(s) actuellement protégé(s)</span>
          </div>
          <p class="text-[11px] text-rose-200/90 leading-relaxed">
            Même les emails contenant des factures, contrats, commandes ou marques de protection seront supprimés.
          </p>
        </div>

        <div class="flex items-center justify-end gap-2 pt-2">
          <button
            @click="forcePurgeDomain = null"
            class="px-3 py-2 rounded-xl text-xs font-medium bg-slate-800 hover:bg-slate-700 text-slate-300 transition"
          >
            Annuler
          </button>
          <button
            @click="handleExecuteForcePurge"
            :disabled="isExecutingForcePurge"
            class="px-4 py-2 bg-rose-600 hover:bg-rose-500 disabled:opacity-50 text-white font-semibold rounded-xl text-xs flex items-center gap-1.5 transition shadow-lg shadow-rose-600/20"
          >
            <Loader2 v-if="isExecutingForcePurge" class="w-3.5 h-3.5 animate-spin" />
            <Trash2 v-else class="w-3.5 h-3.5" />
            <span>{{ isExecutingForcePurge ? "Purge en cours..." : "Confirmer la suppression totale" }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- Modal Confirmation Tout Déprotéger -->
    <div
      v-if="isConfirmUnprotectModalOpen && store.selectedDomain"
      class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4"
    >
      <div
        class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-sm shadow-2xl p-6 space-y-4 animate-in fade-in zoom-in-95 duration-150"
      >
        <div class="flex items-center gap-3">
          <div
            class="w-10 h-10 rounded-xl bg-amber-500/10 border border-amber-500/20 flex items-center justify-center text-amber-400 shrink-0"
          >
            <ShieldAlert class="w-5 h-5" />
          </div>
          <div>
            <h3 class="text-sm font-semibold text-slate-100">
              Tout déprotéger
            </h3>
            <p class="text-xs text-slate-400 font-mono">
              {{ store.selectedDomain }}
            </p>
          </div>
        </div>

        <p class="text-xs text-slate-300 leading-relaxed">
          Voulez-vous déprotéger les <strong>{{ store.protectedEmails.length }} email(s)</strong> de ce domaine ?
          Ils seront tous reclassés comme <em>Candidats à la purge</em>.
        </p>

        <div class="flex items-center justify-end gap-2 pt-2">
          <button
            @click="isConfirmUnprotectModalOpen = false"
            class="px-3 py-2 rounded-xl text-xs font-medium bg-slate-800 hover:bg-slate-700 text-slate-300 transition"
          >
            Annuler
          </button>
          <button
            @click="handleConfirmUnprotect"
            :disabled="store.isLoading"
            class="px-4 py-2 bg-amber-600 hover:bg-amber-500 disabled:opacity-50 text-slate-950 font-semibold rounded-xl text-xs flex items-center gap-1.5 transition shadow"
          >
            <Loader2 v-if="store.isLoading" class="w-3.5 h-3.5 animate-spin" />
            <ShieldAlert v-else class="w-3.5 h-3.5" />
            <span>Déprotéger tous les emails</span>
          </button>
        </div>
      </div>
    </div>

    <!-- Modal : Chasse aux Orphelins & Spams -->
    <OrphanSpamModal
      :is-open="isOrphanSpamModalOpen"
      @close="isOrphanSpamModalOpen = false"
      @open-settings="isSpamRulesModalOpen = true"
    />

    <!-- Modal : Gestion des Règles & Exceptions (ESP Whitelist & TLDs) -->
    <SpamRulesSettingsModal
      :is-open="isSpamRulesModalOpen"
      @close="isSpamRulesModalOpen = false"
    />
  </div>
</template>
