<template>
  <div
    v-if="isOpen && account"
    class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4"
  >
    <div
      class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-md shadow-2xl overflow-hidden flex flex-col"
    >
      <!-- Modal Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-900/60">
        <div class="flex items-center gap-3">
          <div class="w-9 h-9 rounded-xl bg-sky-500/10 border border-sky-500/20 flex items-center justify-center text-sky-400">
            <Edit3 class="w-4 h-4" />
          </div>
          <div>
            <h2 class="text-sm font-semibold text-slate-100">
              Modifier le compte
            </h2>
            <p class="text-[11px] text-slate-400">
              Personnaliser le nom d'affichage du compte
            </p>
          </div>
        </div>
        <button
          @click="close"
          class="text-slate-400 hover:text-slate-200 transition-colors p-1.5 rounded-lg hover:bg-slate-800"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Modal Body -->
      <div class="p-6 space-y-4">
        <!-- Error Alert -->
        <div
          v-if="errorMessage"
          class="p-3 bg-red-500/10 border border-red-500/20 rounded-xl text-xs text-red-400 flex items-start gap-2.5"
        >
          <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
          <div>{{ errorMessage }}</div>
        </div>

        <!-- Account Info (Read-only) -->
        <div class="p-3 bg-slate-950 border border-slate-800 rounded-xl space-y-1.5 text-xs">
          <div class="flex justify-between items-center text-slate-400">
            <span>Adresse email</span>
            <span class="font-mono text-slate-200 font-medium">{{ account.email }}</span>
          </div>
          <div v-if="account.server_host" class="flex justify-between items-center text-slate-400">
            <span>Serveur IMAP</span>
            <span class="font-mono text-slate-300">{{ account.server_host }}:{{ account.server_port || 993 }}</span>
          </div>
          <div class="flex justify-between items-center text-slate-400">
            <span>Type de fournisseur</span>
            <span class="capitalize text-slate-300">{{ account.provider_type }}</span>
          </div>
        </div>

        <!-- Name Input -->
        <div class="space-y-1.5">
          <label class="block text-xs font-medium text-slate-200">
            Nom personnalisé du compte
          </label>
          <input
            v-model="accountName"
            type="text"
            placeholder="Ex: Pro, Personnel, E-commerce..."
            class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
            @keydown.enter="handleSave"
          />
          <p class="text-[11px] text-slate-400 leading-relaxed">
            Ce nom s'affichera dans l'application au lieu de l'adresse et du serveur. Si vous le laissez vide, l'email sera affiché par défaut.
          </p>
        </div>
      </div>

      <!-- Modal Footer -->
      <div class="px-6 py-3.5 bg-slate-950/60 border-t border-slate-800 flex items-center justify-end gap-2.5">
        <button
          @click="close"
          class="px-3.5 py-1.5 text-xs text-slate-400 hover:text-slate-200 rounded-lg transition hover:bg-slate-800"
        >
          Annuler
        </button>
        <button
          @click="handleSave"
          :disabled="isSubmitting"
          class="px-4 py-1.5 bg-sky-500 hover:bg-sky-400 disabled:opacity-50 text-slate-950 font-semibold rounded-lg text-xs flex items-center gap-1.5 transition shadow"
        >
          <Loader2 v-if="isSubmitting" class="w-3.5 h-3.5 animate-spin" />
          <Check v-else class="w-3.5 h-3.5" />
          <span>{{ isSubmitting ? "Enregistrement..." : "Enregistrer" }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { Edit3, X, AlertCircle, Check, Loader2 } from "lucide-vue-next";
import type { AccountRecord } from "../types";
import { useMailStore } from "../stores/mailStore";

const props = defineProps<{
  isOpen: boolean;
  account: AccountRecord | null;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

const mailStore = useMailStore();

const accountName = ref("");
const isSubmitting = ref(false);
const errorMessage = ref("");

watch(
  () => props.account,
  (newAcc) => {
    if (newAcc) {
      accountName.value = newAcc.account_name || "";
      errorMessage.value = "";
    }
  },
  { immediate: true }
);

function close() {
  errorMessage.value = "";
  emit("close");
}

async function handleSave() {
  if (!props.account) return;
  try {
    isSubmitting.value = true;
    errorMessage.value = "";
    await mailStore.updateAccountName(props.account.id, accountName.value);
    close();
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}
</script>
