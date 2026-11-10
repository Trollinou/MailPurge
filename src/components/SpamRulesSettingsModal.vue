<template>
  <div
    v-if="isOpen"
    class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4"
  >
    <div
      class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-3xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]"
    >
      <!-- Modal Header -->
      <div
        class="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-900/60 shrink-0"
      >
        <div class="flex items-center gap-3">
          <div
            class="w-10 h-10 rounded-xl bg-sky-500/10 border border-sky-500/20 flex items-center justify-center text-sky-400"
          >
            <ShieldCheck class="w-5 h-5" />
          </div>
          <div>
            <h2 class="text-base font-semibold text-slate-100">
              Règles d'Intelligence & Listes Blanches
            </h2>
            <p class="text-xs text-slate-400">
              Personnalisez les exceptions d'expéditeurs légitimes et les extensions surveillées.
            </p>
          </div>
        </div>
        <button
          @click="emit('close')"
          class="text-slate-400 hover:text-slate-200 transition-colors p-1.5 rounded-lg hover:bg-slate-800"
        >
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Global Synchronisation Banner -->
      <div
        class="px-6 py-3 bg-slate-950/40 border-b border-slate-800 flex items-center justify-between gap-4 shrink-0 text-xs"
      >
        <div class="flex items-center gap-3">
          <Globe class="w-4 h-4 text-sky-400 shrink-0" />
          <div>
            <div class="font-medium text-slate-200">
              Intelligence Communautaire (Spamhaus, HaGeZi, Disposable Lists)
            </div>
            <div class="text-[11px] text-slate-400">
              {{ status?.total_rules_count ?? 0 }} règles actives — Dernière synchro : {{ formatTimestamp(status?.last_updated_at) }}
            </div>
          </div>
        </div>

        <button
          @click="store.updateAllCommunityRules"
          :disabled="store.isCommunityUpdating"
          class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg font-medium transition-all bg-sky-600 hover:bg-sky-500 text-white disabled:opacity-50 shrink-0 shadow-sm"
        >
          <RefreshCw :class="['w-3.5 h-3.5', store.isCommunityUpdating ? 'animate-spin' : '']" />
          {{ store.isCommunityUpdating ? 'Mise à jour...' : 'Mettre à jour les listes' }}
        </button>
      </div>

      <!-- Navigation Tabs -->
      <div class="flex border-b border-slate-800 bg-slate-950/20 px-6 pt-2.5 gap-2 shrink-0">
        <button
          @click="activeTab = 'esp'"
          :class="[
            'flex items-center gap-2 px-3 py-2 text-xs font-medium rounded-t-lg transition-all border-b-2',
            activeTab === 'esp'
              ? 'border-sky-500 text-sky-400 bg-slate-800/40'
              : 'border-transparent text-slate-400 hover:text-slate-200 hover:bg-slate-800/20',
          ]"
        >
          <Send class="w-3.5 h-3.5" />
          Routeurs d'Emailing Autorisés (ESP)
          <span class="ml-1 px-1.5 py-0.2 rounded text-[10px] bg-slate-800 text-slate-300">
            {{ store.espWhitelist.length }}
          </span>
        </button>
        <button
          @click="activeTab = 'tld'"
          :class="[
            'flex items-center gap-2 px-3 py-2 text-xs font-medium rounded-t-lg transition-all border-b-2',
            activeTab === 'tld'
              ? 'border-sky-500 text-sky-400 bg-slate-800/40'
              : 'border-transparent text-slate-400 hover:text-slate-200 hover:bg-slate-800/20',
          ]"
        >
          <AlertTriangle class="w-3.5 h-3.5" />
          Extensions Suspectes (TLDs)
          <span class="ml-1 px-1.5 py-0.2 rounded text-[10px] bg-slate-800 text-slate-300">
            {{ store.suspiciousTlds.length }}
          </span>
        </button>
      </div>

      <!-- Modal Body -->
      <div class="p-6 overflow-y-auto space-y-4 flex-1 text-xs">
        <!-- Tab 1: ESP Whitelist -->
        <div v-if="activeTab === 'esp'" class="space-y-4">
          <p class="text-slate-400 text-xs">
            Les serveurs légitimes comme <strong>SendGrid, Brevo ou Mailchimp</strong> utilisent des domaines techniques dans l'en-tête <code>Return-Path</code> qui diffèrent du domaine de la marque. Cette liste blanche évite de les confondre avec des usurpateurs de domaine.
          </p>

          <!-- Add custom ESP form -->
          <form
            @submit.prevent="onAddEsp"
            class="flex items-center gap-2 p-3 bg-slate-950/60 border border-slate-800 rounded-xl"
          >
            <input
              v-model="newEspDomain"
              type="text"
              placeholder="Domaine technique (ex: smtp-interne.mon-groupe.fr)"
              required
              class="flex-1 bg-slate-900 border border-slate-700/80 rounded-lg px-3 py-1.5 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-sky-500"
            />
            <input
              v-model="newEspName"
              type="text"
              placeholder="Nom du routeur"
              required
              class="w-48 bg-slate-900 border border-slate-700/80 rounded-lg px-3 py-1.5 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-sky-500"
            />
            <button
              type="submit"
              class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg font-medium bg-sky-600 hover:bg-sky-500 text-white transition-colors shrink-0"
            >
              <Plus class="w-3.5 h-3.5" />
              Ajouter
            </button>
          </form>

          <!-- Search ESP -->
          <div class="flex items-center gap-2">
            <input
              v-model="searchEsp"
              type="text"
              placeholder="Filtrer les routeurs..."
              class="w-full bg-slate-950/40 border border-slate-800 rounded-lg px-3 py-1.5 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-sky-500"
            />
          </div>

          <!-- ESP Table -->
          <div class="border border-slate-800 rounded-xl overflow-hidden">
            <table class="w-full text-left text-xs">
              <thead class="bg-slate-950/60 text-slate-400 border-b border-slate-800">
                <tr>
                  <th class="px-4 py-2 font-medium">Domaine Racine</th>
                  <th class="px-4 py-2 font-medium">Fournisseur</th>
                  <th class="px-4 py-2 font-medium">Origine</th>
                  <th class="px-4 py-2 font-medium text-right">Statut</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-slate-800/60">
                <tr
                  v-for="esp in filteredEsps"
                  :key="esp.root_domain"
                  class="hover:bg-slate-800/30 transition-colors"
                >
                  <td class="px-4 py-2.5 font-mono text-slate-200">
                    {{ esp.root_domain }}
                  </td>
                  <td class="px-4 py-2.5 text-slate-300">
                    {{ esp.provider_name }}
                  </td>
                  <td class="px-4 py-2.5">
                    <span
                      v-if="esp.is_user_added"
                      class="px-2 py-0.5 rounded text-[10px] font-medium bg-purple-500/10 text-purple-300 border border-purple-500/20"
                    >
                      Personnalisé
                    </span>
                    <span
                      v-else
                      class="px-2 py-0.5 rounded text-[10px] font-medium bg-slate-800 text-slate-400"
                    >
                      Officiel
                    </span>
                  </td>
                  <td class="px-4 py-2.5 text-right">
                    <button
                      @click="store.toggleEsp(esp.root_domain, !esp.is_enabled)"
                      :class="[
                        'px-2.5 py-1 rounded-md text-[11px] font-medium transition-all border',
                        esp.is_enabled
                          ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30'
                          : 'bg-slate-800 text-slate-400 border-slate-700',
                      ]"
                    >
                      {{ esp.is_enabled ? 'Autorisé' : 'Désactivé' }}
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- Tab 2: Suspicious TLDs -->
        <div v-if="activeTab === 'tld'" class="space-y-4">
          <p class="text-slate-400 text-xs">
            Ces extensions de domaines sont massivement utilisées pour l'envoi de courriels frauduleux ou de phishing jetable. Si vous travaillez régulièrement avec des partenaires utilisant une de ces extensions (ex: <code>.xyz</code>), désactivez simplement la règle correspondante.
          </p>

          <!-- Search TLDs -->
          <div class="flex items-center gap-2">
            <input
              v-model="searchTld"
              type="text"
              placeholder="Filtrer les extensions (ex: .xyz)..."
              class="w-full bg-slate-950/40 border border-slate-800 rounded-lg px-3 py-1.5 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-sky-500"
            />
          </div>

          <!-- TLDs Table -->
          <div class="border border-slate-800 rounded-xl overflow-hidden">
            <table class="w-full text-left text-xs">
              <thead class="bg-slate-950/60 text-slate-400 border-b border-slate-800">
                <tr>
                  <th class="px-4 py-2 font-medium">Extension (TLD)</th>
                  <th class="px-4 py-2 font-medium">Niveau de Risque</th>
                  <th class="px-4 py-2 font-medium text-right">Règle de filtrage</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-slate-800/60">
                <tr
                  v-for="tld in filteredTlds"
                  :key="tld.tld"
                  class="hover:bg-slate-800/30 transition-colors"
                >
                  <td class="px-4 py-2.5 font-mono font-semibold text-slate-200">
                    {{ tld.tld }}
                  </td>
                  <td class="px-4 py-2.5">
                    <span
                      :class="[
                        'px-2 py-0.5 rounded text-[10px] font-bold border',
                        tld.risk_level === 'HIGH'
                          ? 'bg-rose-500/10 text-rose-400 border-rose-500/20'
                          : 'bg-amber-500/10 text-amber-400 border-amber-500/20',
                      ]"
                    >
                      {{ tld.risk_level === 'HIGH' ? 'ÉLEVÉ' : 'MODÉRÉ' }}
                    </span>
                  </td>
                  <td class="px-4 py-2.5 text-right">
                    <button
                      @click="store.toggleTld(tld.tld, !tld.is_enabled)"
                      :class="[
                        'px-2.5 py-1 rounded-md text-[11px] font-medium transition-all border',
                        tld.is_enabled
                          ? 'bg-rose-500/15 text-rose-300 border-rose-500/30'
                          : 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30',
                      ]"
                    >
                      {{ tld.is_enabled ? 'Surveillé (Spam)' : 'Exclu / Autorisé' }}
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <!-- Modal Footer -->
      <div
        class="px-6 py-3.5 border-t border-slate-800 bg-slate-900/80 flex items-center justify-end shrink-0"
      >
        <button
          @click="emit('close')"
          class="px-4 py-2 rounded-xl text-xs font-medium text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 transition-colors"
        >
          Fermer
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from "vue";
import {
  ShieldCheck,
  X,
  Globe,
  RefreshCw,
  Send,
  AlertTriangle,
  Plus,
} from "lucide-vue-next";
import { useMailStore } from "../stores/mailStore";

const props = defineProps<{
  isOpen: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

const store = useMailStore();
const activeTab = ref<"esp" | "tld">("esp");
const searchEsp = ref("");
const searchTld = ref("");
const newEspDomain = ref("");
const newEspName = ref("");

const status = computed(() => store.communityRulesStatus);

watch(
  () => props.isOpen,
  async (open) => {
    if (open) {
      await store.fetchCommunityRules();
    }
  },
  { immediate: true }
);

onMounted(async () => {
  if (props.isOpen) {
    await store.fetchCommunityRules();
  }
});

const filteredEsps = computed(() => {
  const q = searchEsp.value.trim().toLowerCase();
  if (!q) return store.espWhitelist;
  return store.espWhitelist.filter(
    (e) =>
      e.root_domain.toLowerCase().includes(q) ||
      e.provider_name.toLowerCase().includes(q)
  );
});

const filteredTlds = computed(() => {
  const q = searchTld.value.trim().toLowerCase();
  if (!q) return store.suspiciousTlds;
  return store.suspiciousTlds.filter((t) => t.tld.toLowerCase().includes(q));
});

async function onAddEsp() {
  if (!newEspDomain.value.trim() || !newEspName.value.trim()) return;
  await store.addCustomEsp(newEspDomain.value, newEspName.value);
  newEspDomain.value = "";
  newEspName.value = "";
}

function formatTimestamp(ts?: number): string {
  if (!ts) return "Inconnu";
  return new Date(ts * 1000).toLocaleString("fr-FR", {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
</script>
