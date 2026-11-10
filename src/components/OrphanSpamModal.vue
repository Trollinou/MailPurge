<template>
  <div
    v-if="isOpen"
    class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4"
  >
    <div
      class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-4xl shadow-2xl overflow-hidden flex flex-col max-h-[92vh]"
    >
      <!-- Modal Header -->
      <div
        class="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-900/60 shrink-0"
      >
        <div class="flex items-center gap-3">
          <div
            class="w-10 h-10 rounded-xl bg-rose-500/10 border border-rose-500/20 flex items-center justify-center text-rose-400"
          >
            <Target class="w-5 h-5" />
          </div>
          <div>
            <h2 class="text-base font-semibold text-slate-100 flex items-center gap-2">
              Chasse aux Orphelins & Spams Éphémères
              <span
                class="px-2 py-0.5 text-[11px] font-medium rounded-full bg-rose-500/10 text-rose-300 border border-rose-500/20"
              >
                One-Hit Wonders
              </span>
            </h2>
            <p class="text-xs text-slate-400">
              Détectez et éliminez les vagues de spammeurs n'ayant envoyé que quelques emails avant d'abandonner leur domaine.
            </p>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <button
            @click="emit('openSettings')"
            class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg text-slate-300 bg-slate-800 hover:bg-slate-700 transition-colors border border-slate-700"
            title="Gérer les routeurs ESP autorisés et les extensions de domaines"
          >
            <Settings class="w-3.5 h-3.5" />
            Règles & Exceptions
          </button>
          <button
            @click="emit('close')"
            class="text-slate-400 hover:text-slate-200 transition-colors p-1.5 rounded-lg hover:bg-slate-800"
          >
            <X class="w-5 h-5" />
          </button>
        </div>
      </div>

      <!-- Controls & Scope Bar -->
      <div
        class="px-6 py-3.5 border-b border-slate-800 bg-slate-950/40 flex flex-wrap items-center justify-between gap-4 shrink-0 text-xs"
      >
        <!-- Scope selector (Current vs All) -->
        <div class="flex items-center gap-2">
          <span class="text-slate-400 font-medium">Périmètre :</span>
          <div class="flex p-0.5 bg-slate-900 border border-slate-800 rounded-lg">
            <button
              @click="changeScope('current')"
              :class="[
                'px-2.5 py-1 rounded-md transition-all font-medium',
                store.orphanScope === 'current'
                  ? 'bg-rose-500/20 text-rose-300 border border-rose-500/30'
                  : 'text-slate-400 hover:text-slate-200',
              ]"
            >
              Compte actif ({{ store.currentAccount?.display_name || 'Sélectionné' }})
            </button>
            <button
              @click="changeScope('all')"
              :class="[
                'px-2.5 py-1 rounded-md transition-all font-medium',
                store.orphanScope === 'all'
                  ? 'bg-rose-500/20 text-rose-300 border border-rose-500/30'
                  : 'text-slate-400 hover:text-slate-200',
              ]"
            >
              Tous les comptes (Multi-comptes)
            </button>
          </div>
        </div>

        <!-- Threshold Slider (1 to 10) -->
        <div class="flex items-center gap-3">
          <div class="flex items-center gap-2">
            <span class="text-slate-400 font-medium">Seuil longue traîne :</span>
            <span
              class="px-2 py-0.5 rounded font-mono font-semibold bg-slate-800 text-sky-400 border border-slate-700"
            >
              ≤ {{ store.orphanThreshold }} emails
            </span>
          </div>
          <input
            type="range"
            min="1"
            max="10"
            step="1"
            :value="store.orphanThreshold"
            @input="onThresholdChange"
            class="w-24 accent-rose-500 cursor-pointer"
          />
        </div>

        <!-- Action buttons (DNS Scan & Community Update) -->
        <div class="flex items-center gap-2">
          <button
            @click="triggerDnsAnalysis"
            :disabled="store.isOrphanAnalyzing"
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all bg-sky-500/15 hover:bg-sky-500/25 text-sky-300 border border-sky-500/30 disabled:opacity-50"
            title="Résout les serveurs DNS/MX asynchrones et stocke le résultat dans le cache partagé multi-comptes"
          >
            <RefreshCw
              :class="['w-3.5 h-3.5', store.isOrphanAnalyzing ? 'animate-spin' : '']"
            />
            {{ store.isOrphanAnalyzing ? 'Analyse DNS...' : 'Analyser les DNS (NXDOMAIN)' }}
          </button>
        </div>
      </div>

      <!-- Shared Cache & Intelligence Summary Bar -->
      <div
        class="px-6 py-2.5 bg-slate-900/40 border-b border-slate-800 flex items-center justify-between text-xs text-slate-400 shrink-0"
      >
        <div class="flex items-center gap-4">
          <span class="flex items-center gap-1.5">
            <Database class="w-3.5 h-3.5 text-slate-500" />
            Cache partagé multi-comptes :
            <strong class="text-slate-200">{{ summary?.dns_analyzed_count ?? 0 }} résolus</strong>
            <span v-if="(summary?.dns_pending_count ?? 0) > 0" class="text-amber-400">
              ({{ summary?.dns_pending_count }} en attente)
            </span>
          </span>
        </div>
        <div class="flex items-center gap-4">
          <span>
            Total orphelins ciblés :
            <strong class="text-rose-300 font-semibold">{{ summary?.total_domains ?? 0 }} domaines</strong>
            ({{ summary?.total_emails ?? 0 }} emails, {{ formatBytes(summary?.total_size ?? 0) }})
          </span>
        </div>
      </div>

      <!-- Action Feedback Banner -->
      <div
        v-if="tagSuccessMessage"
        class="mx-6 mt-4 p-3 bg-emerald-500/15 border border-emerald-500/30 rounded-xl flex items-center justify-between text-xs text-emerald-300 shrink-0"
      >
        <div class="flex items-center gap-2">
          <CheckCircle class="w-4 h-4 text-emerald-400 shrink-0" />
          <span>{{ tagSuccessMessage }}</span>
        </div>
        <button
          @click="viewMarkedInMainView"
          class="flex items-center gap-1.5 px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white font-medium rounded-lg shadow-sm transition-all text-xs"
        >
          <Search class="w-3.5 h-3.5" />
          Vérifier dans la liste principale
        </button>
      </div>

      <!-- Modal Body (Cards of 4 categories) -->
      <div class="p-6 overflow-y-auto space-y-4 flex-1">
        <!-- Zero results state -->
        <div
          v-if="!summary || summary.groups.every((g) => g.domain_count === 0)"
          class="text-center py-12 space-y-3"
        >
          <div
            class="w-12 h-12 rounded-2xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 flex items-center justify-center mx-auto"
          >
            <ShieldCheck class="w-6 h-6" />
          </div>
          <h3 class="text-sm font-semibold text-slate-200">
            Aucun spam orphelin détecté
          </h3>
          <p class="text-xs text-slate-400 max-w-md mx-auto">
            Tous les domaines à faible volumétrie sont soit légitimes, soit leurs DNS sont encore vivants et leurs en-têtes sains. Vous pouvez augmenter le seuil d'emails ou lancer une analyse DNS.
          </p>
        </div>

        <!-- Category Groups Grid -->
        <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div
            v-for="group in summary.groups"
            :key="group.category"
            class="bg-slate-950/50 border border-slate-800 rounded-xl p-4 flex flex-col justify-between hover:border-slate-700 transition-all"
          >
            <div>
              <!-- Card Header -->
              <div class="flex items-start justify-between gap-3 mb-2">
                <div class="flex items-center gap-2.5">
                  <div
                    :class="[
                      'w-8 h-8 rounded-lg flex items-center justify-center shrink-0 border',
                      getCategoryColorClass(group.category),
                    ]"
                  >
                    <component :is="getCategoryIcon(group.category)" class="w-4 h-4" />
                  </div>
                  <div>
                    <h4 class="text-xs font-semibold text-slate-200">
                      {{ group.label }}
                    </h4>
                    <p class="text-[11px] text-slate-400">
                      {{ group.description }}
                    </p>
                  </div>
                </div>
              </div>

              <!-- Metrics -->
              <div class="flex items-center gap-3 py-2 text-xs border-y border-slate-800/80 my-2">
                <div>
                  <span class="text-slate-500">Domaines :</span>
                  <span class="ml-1 font-semibold text-slate-200">{{ group.domain_count }}</span>
                </div>
                <div class="w-px h-3 bg-slate-800" />
                <div>
                  <span class="text-slate-500">Emails :</span>
                  <span class="ml-1 font-semibold text-rose-300">{{ group.email_count }}</span>
                </div>
                <div class="w-px h-3 bg-slate-800" />
                <div>
                  <span class="text-slate-500">Taille :</span>
                  <span class="ml-1 font-semibold text-slate-300">{{ formatBytes(group.total_size) }}</span>
                </div>
              </div>

              <!-- Domain sample pills -->
              <div v-if="group.domains.length > 0" class="space-y-1.5 mt-2">
                <div class="text-[11px] font-medium text-slate-500">
                  Exemples de domaines :
                </div>
                <div class="flex flex-wrap gap-1.5 max-h-24 overflow-y-auto">
                  <span
                    v-for="domain in group.domains.slice(0, 8)"
                    :key="domain"
                    class="px-2 py-0.5 rounded text-[11px] bg-slate-900 border border-slate-800 text-slate-300 font-mono"
                  >
                    {{ domain }}
                  </span>
                  <span
                    v-if="group.domains.length > 8"
                    class="px-1.5 py-0.5 rounded text-[10px] text-slate-500 font-medium"
                  >
                    +{{ group.domains.length - 8 }} autres
                  </span>
                </div>
              </div>
            </div>

            <!-- Card Action Footer -->
            <div class="mt-4 pt-3 border-t border-slate-800/80 flex items-center justify-between">
              <span class="text-[11px] text-slate-500">
                Marquage pour inspection
              </span>
              <button
                @click="tagCategory(group.category)"
                :disabled="group.domain_count === 0 || isMarking"
                :class="[
                  'px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all',
                  markedCategories.has(group.category)
                    ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
                    : 'bg-rose-500/15 hover:bg-rose-500/25 text-rose-300 border border-rose-500/30 disabled:opacity-40'
                ]"
              >
                <BookmarkCheck v-if="markedCategories.has(group.category)" class="w-3.5 h-3.5" />
                <Bookmark v-else class="w-3.5 h-3.5" />
                <span>
                  {{ markedCategories.has(group.category) ? '✓ Marqué' : 'Marquer ce groupe' }} ({{ group.domain_count }})
                </span>
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Modal Footer -->
      <div
        class="px-6 py-4 border-t border-slate-800 bg-slate-900/80 flex items-center justify-between shrink-0"
      >
        <div class="text-xs text-slate-400">
          💡 Les domaines marqués recevront un badge d'alerte et pourront être vérifiés ou exclus d'un clic dans la liste principale.
        </div>

        <!-- Actions -->
        <div class="flex items-center gap-3">
          <button
            @click="emit('close')"
            class="px-4 py-2 rounded-xl text-xs font-medium text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors"
          >
            Fermer
          </button>
          <button
            @click="tagAll"
            :disabled="(summary?.total_domains ?? 0) === 0 || isMarking"
            class="flex items-center gap-2 px-5 py-2 rounded-xl text-xs font-semibold bg-rose-600 hover:bg-rose-500 text-white shadow-lg shadow-rose-600/25 transition-all disabled:opacity-50"
          >
            <Bookmark class="w-3.5 h-3.5" />
            <span v-if="isMarking">Marquage en cours...</span>
            <span v-else>
              Tout Marquer pour Vérification ({{ summary?.total_domains ?? 0 }} domaines — {{ summary?.total_emails ?? 0 }} emails)
            </span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, watch } from "vue";
import {
  Target,
  X,
  Settings,
  RefreshCw,
  Database,
  Trash2,
  ShieldCheck,
  Skull,
  ShieldAlert,
  AlertTriangle,
  Flame,
  Bookmark,
  BookmarkCheck,
  CheckCircle,
  Search,
} from "lucide-vue-next";
import { useMailStore, formatBytes } from "../stores/mailStore";
import type { OrphanCategory } from "../types";

const props = defineProps<{
  isOpen: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "openSettings"): void;
}>();

const store = useMailStore();
const summary = computed(() => store.orphanSummary);

const tagSuccessMessage = ref("");
const markedCategories = ref(new Set<string>());
const isMarking = ref(false);

watch(
  () => props.isOpen,
  async (open) => {
    if (open) {
      tagSuccessMessage.value = "";
      await store.fetchOrphanSummary(store.orphanScope, store.orphanThreshold);
    }
  },
  { immediate: true }
);

onMounted(async () => {
  if (props.isOpen) {
    await store.fetchOrphanSummary(store.orphanScope, store.orphanThreshold);
  }
});

function changeScope(scope: "current" | "all") {
  tagSuccessMessage.value = "";
  store.fetchOrphanSummary(scope, store.orphanThreshold);
}

function onThresholdChange(e: Event) {
  tagSuccessMessage.value = "";
  const val = Number((e.target as HTMLInputElement).value);
  store.orphanThreshold = val;
  store.fetchOrphanSummary(store.orphanScope, val);
}

async function triggerDnsAnalysis() {
  tagSuccessMessage.value = "";
  await store.analyzeOrphanDns(store.orphanScope, store.orphanThreshold);
}

async function tagCategory(cat: OrphanCategory) {
  isMarking.value = true;
  try {
    const count = await store.tagOrphanGroup(cat, store.orphanScope);
    markedCategories.value.add(cat);
    const grp = summary.value?.groups.find((g) => g.category === cat);
    tagSuccessMessage.value = `${count} domaine(s) de la catégorie "${grp?.label || cat}" ont été marqués avec succès pour vérification.`;
  } catch (err) {
    tagSuccessMessage.value = `Erreur lors du marquage : ${String(err)}`;
  } finally {
    isMarking.value = false;
  }
}

async function tagAll() {
  isMarking.value = true;
  try {
    const count = await store.tagOrphanGroup(null, store.orphanScope);
    if (summary.value) {
      for (const g of summary.value.groups) {
        markedCategories.value.add(g.category);
      }
    }
    tagSuccessMessage.value = `Tous les domaines orphelins ciblés (${count}) ont été marqués pour vérification dans la liste principale.`;
  } catch (err) {
    tagSuccessMessage.value = `Erreur lors du marquage : ${String(err)}`;
  } finally {
    isMarking.value = false;
  }
}

function viewMarkedInMainView() {
  // If specific categories were marked, filter by them, otherwise filter by ALL_ORPHANS
  const tags: string[] = [];
  if (markedCategories.value.size > 0) {
    for (const cat of markedCategories.value) {
      if (cat === "DEAD_DOMAIN") tags.push("DEAD_DOMAIN");
      else if (cat === "SERVER_SPAM" || cat === "SERVER_SPAM_VERDICT") tags.push("SERVER_SPAM");
      else if (cat === "STRUCTURAL_ANOMALY") tags.push("STRUCTURAL_ANOMALY");
      else if (cat === "DISPOSABLE" || cat === "DISPOSABLE_DOMAIN") tags.push("DISPOSABLE");
    }
  } else {
    tags.push("ALL_ORPHANS");
  }
  store.selectedTags = tags;
  emit("close");
}

function getCategoryColorClass(cat: OrphanCategory): string {
  switch (cat) {
    case "DEAD_DOMAIN":
      return "bg-slate-500/10 border-slate-500/20 text-slate-400";
    case "SERVER_SPAM":
      return "bg-rose-500/10 border-rose-500/20 text-rose-400";
    case "STRUCTURAL_ANOMALY":
      return "bg-amber-500/10 border-amber-500/20 text-amber-400";
    case "DISPOSABLE":
      return "bg-orange-500/10 border-orange-500/20 text-orange-400";
    default:
      return "bg-slate-500/10 border-slate-500/20 text-slate-400";
  }
}

function getCategoryIcon(cat: OrphanCategory) {
  switch (cat) {
    case "DEAD_DOMAIN":
      return Skull;
    case "SERVER_SPAM":
      return ShieldAlert;
    case "STRUCTURAL_ANOMALY":
      return AlertTriangle;
    case "DISPOSABLE":
      return Flame;
    default:
      return Target;
  }
}

async function purgeCategory(cat: OrphanCategory, count: number) {
  const modeText = store.dryRunEnabled ? "simuler la purge de" : "déplacer vers la corbeille";
  const ok = window.confirm(
    `Êtes-vous certain de vouloir ${modeText} ${count} email(s) de cette catégorie ?`
  );
  if (!ok) return;

  await store.purgeOrphanGroup(cat, store.orphanScope, store.dryRunEnabled);
}

async function purgeAll() {
  const count = summary.value?.total_emails ?? 0;
  const modeText = store.dryRunEnabled ? "simuler la suppression de" : "déplacer vers la corbeille";
  const ok = window.confirm(
    `Êtes-vous certain de vouloir ${modeText} TOUS les ${count} emails orphelins détectés ?`
  );
  if (!ok) return;

  await store.purgeOrphanGroup(null, store.orphanScope, store.dryRunEnabled);
}
</script>
