<script setup lang="ts">
import { onMounted } from "vue";
import { useMailStore, formatBytes } from "./stores/mailStore";
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
  Inbox,
  Clock,
  Sparkles,
  Eye,
  X,
  ArrowUpDown,
  Shield,
  RotateCcw,
} from "lucide-vue-next";

const store = useMailStore();

onMounted(async () => {
  await store.fetchAccounts();
  if (store.accounts.length === 0) {
    // Automatically load demo data on first launch if empty
    await store.loadDemoData();
  }
});

function formatDate(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleDateString("fr-FR", {
    day: "2-digit",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
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
            <button
              @click="store.loadDemoData"
              class="text-indigo-400 hover:text-indigo-300 flex items-center space-x-1 text-[11px] normal-case"
              title="Recharger le jeu d'essai démo"
            >
              <Sparkles class="w-3 h-3" />
              <span>Démo</span>
            </button>
          </div>

          <div class="space-y-1">
            <button
              v-for="account in store.accounts"
              :key="account.id"
              @click="store.selectAccount(account.id)"
              :class="[
                'w-full text-left p-2.5 rounded-lg transition-all flex flex-col space-y-1',
                store.currentAccountId === account.id
                  ? 'bg-indigo-600/20 text-white border border-indigo-500/40'
                  : 'hover:bg-slate-800/60 text-slate-300'
              ]"
            >
              <div class="flex items-center justify-between w-full">
                <span class="font-medium truncate text-xs">{{ account.display_name }}</span>
                <span class="text-[10px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 font-mono">
                  {{ account.provider_type === 'microsoft365' ? 'M365' : 'IMAP' }}
                </span>
              </div>
              <span class="text-[11px] text-slate-400 truncate">{{ account.email }}</span>
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
          <h2 class="text-base font-semibold text-white">Domaines Détectés</h2>
          <p class="text-xs text-slate-400">Regroupement par domaine racine (eTLD+1)</p>
        </div>

        <div class="flex items-center space-x-2.5">
          <!-- Tri par options -->
          <div class="flex items-center bg-slate-900 border border-slate-700/60 rounded-lg p-0.5 text-xs">
            <button
              @click="store.sortBy = 'size'"
              :class="[
                'px-2.5 py-1 rounded transition text-xs font-medium',
                store.sortBy === 'size'
                  ? 'bg-indigo-600 text-white shadow'
                  : 'text-slate-400 hover:text-slate-200'
              ]"
              title="Trier par volume purgeable décroissant"
            >
              Volume
            </button>
            <button
              @click="store.sortBy = 'count'"
              :class="[
                'px-2.5 py-1 rounded transition text-xs font-medium',
                store.sortBy === 'count'
                  ? 'bg-indigo-600 text-white shadow'
                  : 'text-slate-400 hover:text-slate-200'
              ]"
              title="Trier par nombre total de mails"
            >
              Nb Mails
            </button>
            <button
              @click="store.sortBy = 'alpha'"
              :class="[
                'px-2.5 py-1 rounded transition text-xs font-medium',
                store.sortBy === 'alpha'
                  ? 'bg-indigo-600 text-white shadow'
                  : 'text-slate-400 hover:text-slate-200'
              ]"
              title="Trier par ordre alphabétique"
            >
              A-Z
            </button>
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
        </div>
      </header>

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
                <span class="text-[11px] text-slate-500">({{ domain.total_emails }} mails)</span>
              </div>
              <div class="flex items-center space-x-3 text-xs mt-1">
                <span class="text-slate-400">Total : {{ formatBytes(domain.total_size) }}</span>
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
          </div>
        </div>

        <div v-if="store.filteredDomainStats.length === 0" class="p-12 text-center text-slate-500">
          Aucun domaine trouvé.
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
          <h3 class="font-semibold text-white text-sm">
            {{ store.selectedDomain || "Sélectionnez un domaine" }}
          </h3>
          <p class="text-xs text-slate-400">Inspection détaillée & filet de sécurité</p>
        </div>
      </div>

      <!-- Onglets : Candidats vs Protégés -->
      <div class="flex border-b border-slate-800 text-xs">
        <button
          @click="store.activeTab = 'purgeable'"
          :class="[
            'flex-1 py-2.5 text-center font-medium border-b-2 transition flex items-center justify-center space-x-1.5',
            store.activeTab === 'purgeable'
              ? 'border-indigo-500 text-white bg-slate-800/40'
              : 'border-transparent text-slate-400 hover:text-slate-200'
          ]"
        >
          <span>Candidats Corbeille</span>
          <span class="px-1.5 py-0.2 rounded-full text-[10px] bg-slate-800 font-mono">
            {{ store.purgeableEmails.length }}
          </span>
        </button>

        <button
          @click="store.activeTab = 'protected'"
          :class="[
            'flex-1 py-2.5 text-center font-medium border-b-2 transition flex items-center justify-center space-x-1.5',
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

        <!-- Liste des emails protégés -->
        <div v-if="store.activeTab === 'protected'" class="space-y-2">
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

            <div class="text-[11px] text-slate-400 truncate">
              {{ email.sender_address }}
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
          class="p-2 rounded bg-slate-800/80 border border-slate-700 text-[11px] text-slate-300 flex items-center space-x-1.5"
        >
          <CheckCircle2 class="w-3.5 h-3.5 text-indigo-400 shrink-0" />
          <span class="truncate">{{ store.statusMessage }}</span>
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
        </div>

        <!-- Modal Body Content -->
        <div class="flex-1 overflow-y-auto p-5 text-xs leading-relaxed text-slate-200 whitespace-pre-wrap font-sans bg-slate-950/30">
          {{ store.previewedEmail.body_snippet || "Aucun contenu de prévisualisation disponible pour cet email." }}
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
  </div>
</template>
