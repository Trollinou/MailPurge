<template>
  <div
    v-if="isOpen"
    class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4"
  >
    <div
      class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]"
    >
      <!-- Modal Header -->
      <div
        class="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-900/60"
      >
        <div class="flex items-center gap-3">
          <div
            class="w-10 h-10 rounded-xl bg-sky-500/10 border border-sky-500/20 flex items-center justify-center text-sky-400"
          >
            <Mail class="w-5 h-5" />
          </div>
          <div>
            <h2 class="text-base font-semibold text-slate-100">
              Ajouter un compte de messagerie
            </h2>
            <p class="text-xs text-slate-400">
              100 % local — vos identifiants restent chiffrés dans votre trousseau OS
            </p>
          </div>
        </div>
        <button
          @click="close"
          class="text-slate-400 hover:text-slate-200 transition-colors p-1.5 rounded-lg hover:bg-slate-800"
        >
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Provider Tabs -->
      <div class="flex border-b border-slate-800 bg-slate-950/40 px-6 pt-3 gap-2">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          @click="activeTab = tab.id"
          :class="[
            'flex items-center gap-2 px-3 py-2 text-xs font-medium rounded-t-lg transition-all border-b-2',
            activeTab === tab.id
              ? 'border-sky-500 text-sky-400 bg-slate-800/40'
              : 'border-transparent text-slate-400 hover:text-slate-200 hover:bg-slate-800/20',
          ]"
        >
          <component :is="tab.icon" class="w-4 h-4" />
          {{ tab.name }}
        </button>
      </div>

      <!-- Modal Body -->
      <div class="p-6 overflow-y-auto space-y-4 flex-1">
        <!-- Error Alert -->
        <div
          v-if="errorMessage"
          class="p-3 bg-red-500/10 border border-red-500/20 rounded-xl text-xs text-red-400 flex items-start gap-2.5"
        >
          <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
          <div>{{ errorMessage }}</div>
        </div>

        <!-- Google / Gmail Tab -->
        <div v-if="activeTab === 'google'" class="space-y-4">
          <!-- Sub-mode Selector -->
          <div class="grid grid-cols-2 p-1 bg-slate-950 border border-slate-800 rounded-xl text-xs gap-1">
            <button
              @click="gmailMode = 'app_password'"
              :class="[
                'py-1.5 px-3 rounded-lg font-medium transition flex items-center justify-center gap-1.5',
                gmailMode === 'app_password'
                  ? 'bg-sky-500 text-slate-950 shadow'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/40'
              ]"
            >
              <KeyRound class="w-3.5 h-3.5" />
              <span>Mot de passe d'application (Standard)</span>
            </button>
            <button
              @click="gmailMode = 'oauth'"
              :class="[
                'py-1.5 px-3 rounded-lg font-medium transition flex items-center justify-center gap-1.5',
                gmailMode === 'oauth'
                  ? 'bg-sky-500 text-slate-950 shadow'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/40'
              ]"
            >
              <Cloud class="w-3.5 h-3.5" />
              <span>Google Cloud OAuth2 (Entreprise)</span>
            </button>
          </div>

          <!-- Mode A : App Password (Recommandé) -->
          <div v-if="gmailMode === 'app_password'" class="space-y-4">
            <div class="bg-sky-950/30 border border-sky-800/40 rounded-xl p-3.5 space-y-2 text-xs">
              <div class="font-medium text-sky-300 flex items-center gap-1.5">
                <Sparkles class="w-3.5 h-3.5 text-sky-400" />
                Recommandé pour tous les comptes Gmail personnels et TPE
              </div>
              <p class="text-slate-400 leading-relaxed">
                Générez un mot de passe d'application dédié à MailPurge depuis
                <a
                  href="https://myaccount.google.com/apppasswords"
                  target="_blank"
                  class="text-sky-300 underline inline-flex items-center gap-1 hover:text-sky-200"
                >
                  myaccount.google.com/apppasswords
                  <ExternalLink class="w-3 h-3" />
                </a>.
              </p>
              <div class="p-2.5 bg-slate-900/80 rounded-lg border border-slate-800 text-[11px] text-slate-300 space-y-1">
                <div class="font-semibold text-amber-300">💡 Si Google indique "non disponible pour votre compte" :</div>
                <ul class="list-disc list-inside space-y-1 text-slate-400">
                  <li>
                    Activez d'abord la <a href="https://myaccount.google.com/signinoptions/two-step-verification" target="_blank" class="text-sky-400 underline">validation en 2 étapes</a> (prérequis obligatoire Google).
                  </li>
                  <li>
                    Tapez ensuite <strong>« Mots de passe des applications »</strong> dans la barre de recherche tout en haut de votre compte Google.
                  </li>
                  <li>
                    Assurez-vous que l'accès IMAP est activé dans <a href="https://mail.google.com/mail/u/0/#settings/fwdandpop" target="_blank" class="text-sky-400 underline">Gmail &gt; Paramètres &gt; Transfert et POP/IMAP</a>.
                  </li>
                </ul>
              </div>
            </div>

            <div class="space-y-3">
              <div class="space-y-1">
                <label class="block text-xs font-medium text-slate-300">Adresse email Gmail</label>
                <input
                  v-model="gmailEmail"
                  type="email"
                  placeholder="votre-adresse@gmail.com"
                  class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
                />
              </div>
              <div class="space-y-1">
                <label class="block text-xs font-medium text-slate-300">
                  Mot de passe d'application Google (16 lettres)
                </label>
                <input
                  v-model="gmailAppPassword"
                  type="password"
                  placeholder="abcd efgh ijkl mnop"
                  class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500 font-mono"
                />
              </div>
            </div>

            <button
              @click="connectGmailAppPassword"
              :disabled="isSubmitting || !gmailEmail || !gmailAppPassword"
              class="w-full py-2.5 px-4 bg-sky-500 hover:bg-sky-400 disabled:opacity-50 text-slate-950 font-semibold rounded-xl text-xs flex items-center justify-center gap-2 transition-colors shadow-lg shadow-sky-500/10"
            >
              <Loader2 v-if="isSubmitting" class="w-4 h-4 animate-spin" />
              <Check v-else class="w-4 h-4" />
              <span>{{ isSubmitting ? "Test de connexion IMAP en cours..." : "Connecter Gmail" }}</span>
            </button>
          </div>

          <!-- Mode B : Google Cloud OAuth2 -->
          <div v-else class="space-y-4">
            <div class="bg-slate-800/40 border border-slate-800 rounded-xl p-4 space-y-2">
              <div class="text-sm font-medium text-slate-200 flex items-center gap-2">
                <ShieldCheck class="w-4 h-4 text-emerald-400" />
                Connexion OAuth2 Google Mail
              </div>
              <p class="text-xs text-slate-400 leading-relaxed">
                Nécessite une application configurée sur la Google Cloud Console avec les scopes IMAP activés.
              </p>
            </div>

            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-slate-300">
                Client ID Google Cloud
              </label>
              <input
                v-model="googleClientId"
                type="text"
                placeholder="Ex: 812345678901-xxxx.apps.googleusercontent.com"
                class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
              />
            </div>

            <button
              @click="connectGoogleOAuth"
              :disabled="isSubmitting"
              class="w-full py-2.5 px-4 bg-sky-500 hover:bg-sky-400 disabled:opacity-50 text-slate-950 font-semibold rounded-xl text-xs flex items-center justify-center gap-2 transition-colors shadow-lg shadow-sky-500/10"
            >
              <Loader2 v-if="isSubmitting" class="w-4 h-4 animate-spin" />
              <ExternalLink v-else class="w-4 h-4" />
              {{ isSubmitting ? "Connexion en cours dans le navigateur..." : "Se connecter avec Google (Web)" }}
            </button>
          </div>
        </div>

        <!-- Microsoft 365 / Outlook Tab -->
        <div v-if="activeTab === 'microsoft365'" class="space-y-4">
          <!-- Sub-mode Selector -->
          <div class="grid grid-cols-2 p-1 bg-slate-950 border border-slate-800 rounded-xl text-xs gap-1">
            <button
              @click="m365Mode = 'oauth'"
              :class="[
                'py-1.5 px-3 rounded-lg font-medium transition flex items-center justify-center gap-1.5',
                m365Mode === 'oauth'
                  ? 'bg-sky-500 text-slate-950 shadow'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/40'
              ]"
            >
              <ShieldCheck class="w-3.5 h-3.5" />
              <span>Connexion Web OAuth2 (Pro & Perso)</span>
            </button>
            <button
              @click="m365Mode = 'app_password'"
              :class="[
                'py-1.5 px-3 rounded-lg font-medium transition flex items-center justify-center gap-1.5',
                m365Mode === 'app_password'
                  ? 'bg-sky-500 text-slate-950 shadow'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/40'
              ]"
            >
              <KeyRound class="w-3.5 h-3.5" />
              <span>Mot de passe d'app (Outlook.com)</span>
            </button>
          </div>

          <!-- Mode A : OAuth2 Web (Pro & Perso) -->
          <div v-if="m365Mode === 'oauth'" class="space-y-4">
            <div class="bg-slate-800/40 border border-slate-800 rounded-xl p-4 space-y-2">
              <div class="text-sm font-medium text-slate-200 flex items-center gap-2">
                <ShieldCheck class="w-4 h-4 text-emerald-400" />
                Authentification Sécurisée Microsoft (SSO / MFA)
              </div>
              <p class="text-xs text-slate-400 leading-relaxed">
                Valide directement votre compte professionnel (Entra ID) ou personnel (@outlook.com, @hotmail.com) dans votre navigateur par défaut sans exposer votre mot de passe.
              </p>
            </div>

            <div class="space-y-1.5">
              <label class="block text-xs font-medium text-slate-300">
                Client ID Azure AD (Optionnel pour tenant d'entreprise strict)
              </label>
              <input
                v-model="m365ClientId"
                type="text"
                placeholder="d3590ed6-52b3-4102-aeff-aad2292ab01c (Par défaut Office Desktop)"
                class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
              />
            </div>

            <button
              @click="connectMicrosoftOAuth"
              :disabled="isSubmitting"
              class="w-full py-2.5 px-4 bg-sky-500 hover:bg-sky-400 disabled:opacity-50 text-slate-950 font-semibold rounded-xl text-xs flex items-center justify-center gap-2 transition-colors shadow-lg shadow-sky-500/10"
            >
              <Loader2 v-if="isSubmitting" class="w-4 h-4 animate-spin" />
              <ExternalLink v-else class="w-4 h-4" />
              {{ isSubmitting ? "Connexion en cours dans le navigateur..." : "Se connecter avec Microsoft" }}
            </button>
          </div>

          <!-- Mode B : App Password (Particuliers) -->
          <div v-else class="space-y-4">
            <div class="bg-sky-950/30 border border-sky-800/40 rounded-xl p-3.5 space-y-1.5 text-xs">
              <div class="font-medium text-sky-300 flex items-center gap-1.5">
                <KeyRound class="w-3.5 h-3.5" />
                Pour les comptes personnels Outlook / Hotmail / Live
              </div>
              <p class="text-slate-400 leading-relaxed">
                Générez un mot de passe d'application depuis
                <a
                  href="https://account.live.com/proofs/manage"
                  target="_blank"
                  class="text-sky-300 underline inline-flex items-center gap-1 hover:text-sky-200"
                >
                  account.live.com/proofs/manage
                  <ExternalLink class="w-3 h-3" />
                </a>.
              </p>
            </div>

            <div class="space-y-3">
              <div class="space-y-1">
                <label class="block text-xs font-medium text-slate-300">Adresse email Outlook / Hotmail</label>
                <input
                  v-model="outlookEmail"
                  type="email"
                  placeholder="nom@outlook.com ou @hotmail.com"
                  class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
                />
              </div>
              <div class="space-y-1">
                <label class="block text-xs font-medium text-slate-300">
                  Mot de passe d'application Microsoft
                </label>
                <input
                  v-model="outlookAppPassword"
                  type="password"
                  placeholder="Mot de passe d'application"
                  class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500 font-mono"
                />
              </div>
            </div>

            <button
              @click="connectOutlookAppPassword"
              :disabled="isSubmitting || !outlookEmail || !outlookAppPassword"
              class="w-full py-2.5 px-4 bg-sky-500 hover:bg-sky-400 disabled:opacity-50 text-slate-950 font-semibold rounded-xl text-xs flex items-center justify-center gap-2 transition-colors shadow-lg shadow-sky-500/10"
            >
              <Loader2 v-if="isSubmitting" class="w-4 h-4 animate-spin" />
              <Check v-else class="w-4 h-4" />
              <span>{{ isSubmitting ? "Test de connexion IMAP en cours..." : "Connecter Outlook" }}</span>
            </button>
          </div>
        </div>

        <!-- iCloud Tab -->
        <div v-if="activeTab === 'icloud'" class="space-y-4">
          <div class="bg-amber-500/10 border border-amber-500/20 rounded-xl p-3.5 space-y-1 text-xs">
            <div class="font-medium text-amber-300 flex items-center gap-1.5">
              <KeyRound class="w-3.5 h-3.5" />
              Mot de passe d'application requis pour iCloud
            </div>
            <p class="text-slate-400 leading-relaxed">
              Apple exige un mot de passe spécifique généré depuis
              <a
                href="https://appleid.apple.com"
                target="_blank"
                class="text-amber-200 underline inline-flex items-center gap-1"
              >
                appleid.apple.com
                <ExternalLink class="w-3 h-3" />
              </a>
              &gt; Mots de passe pour les apps.
            </p>
          </div>

          <div class="space-y-3">
            <div class="space-y-1">
              <label class="block text-xs font-medium text-slate-300">Adresse email iCloud</label>
              <input
                v-model="icloudEmail"
                type="email"
                placeholder="nom.prenom@icloud.com"
                class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
              />
            </div>
            <div class="space-y-1">
              <label class="block text-xs font-medium text-slate-300">
                Mot de passe d'application Apple
              </label>
              <input
                v-model="icloudAppPassword"
                type="password"
                placeholder="xxxx-xxxx-xxxx-xxxx"
                class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500 font-mono"
              />
            </div>
          </div>

          <button
            @click="connectIcloud"
            :disabled="isSubmitting || !icloudEmail || !icloudAppPassword"
            class="w-full py-2.5 px-4 bg-sky-500 hover:bg-sky-400 disabled:opacity-50 text-slate-950 font-semibold rounded-xl text-xs flex items-center justify-center gap-2 transition-colors shadow-lg shadow-sky-500/10"
          >
            <Loader2 v-if="isSubmitting" class="w-4 h-4 animate-spin" />
            <Check v-else class="w-4 h-4" />
            <span>{{ isSubmitting ? "Test de connexion IMAP en cours..." : "Connecter iCloud" }}</span>
          </button>
        </div>

        <!-- Custom IMAP Tab -->
        <div v-if="activeTab === 'imap'" class="space-y-4">
          <div class="space-y-3">
            <div class="space-y-1">
              <label class="block text-xs font-medium text-slate-300">Adresse email</label>
              <input
                v-model="imapEmail"
                type="email"
                placeholder="contact@monentreprise.fr"
                class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
              />
            </div>

            <div class="grid grid-cols-3 gap-2">
              <div class="col-span-2 space-y-1">
                <label class="block text-xs font-medium text-slate-300">Serveur IMAP</label>
                <input
                  v-model="imapServer"
                  type="text"
                  placeholder="mail.monentreprise.fr"
                  class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
                />
              </div>
              <div class="space-y-1">
                <label class="block text-xs font-medium text-slate-300">Port (SSL/TLS)</label>
                <input
                  v-model.number="imapPort"
                  type="number"
                  placeholder="993"
                  class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
                />
              </div>
            </div>

            <div class="space-y-1">
              <label class="block text-xs font-medium text-slate-300">Mot de passe</label>
              <input
                v-model="imapPassword"
                type="password"
                placeholder="••••••••••••"
                class="w-full px-3 py-2 bg-slate-950 border border-slate-800 rounded-xl text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-sky-500"
              />
            </div>
          </div>

          <button
            @click="connectImap"
            :disabled="isSubmitting || !imapEmail || !imapServer || !imapPassword"
            class="w-full py-2.5 px-4 bg-sky-500 hover:bg-sky-400 disabled:opacity-50 text-slate-950 font-semibold rounded-xl text-xs flex items-center justify-center gap-2 transition-colors shadow-lg shadow-sky-500/10"
          >
            <Loader2 v-if="isSubmitting" class="w-4 h-4 animate-spin" />
            <Check v-else class="w-4 h-4" />
            <span>{{ isSubmitting ? "Test de connexion IMAP en cours..." : "Connecter le compte IMAP" }}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import {
  Mail,
  X,
  ShieldCheck,
  KeyRound,
  ExternalLink,
  Check,
  AlertCircle,
  Loader2,
  Cloud,
  Server,
  Sparkles,
} from "lucide-vue-next";
import { useMailStore } from "../stores/mailStore";

const props = defineProps<{
  isOpen: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

const mailStore = useMailStore();

const activeTab = ref<"google" | "microsoft365" | "icloud" | "imap">("google");
const isSubmitting = ref(false);
const errorMessage = ref("");

// Google modes
const gmailMode = ref<"app_password" | "oauth">("app_password");
const gmailEmail = ref("");
const gmailAppPassword = ref("");
const googleClientId = ref("");

// Microsoft modes
const m365Mode = ref<"oauth" | "app_password">("oauth");
const m365ClientId = ref("");
const outlookEmail = ref("");
const outlookAppPassword = ref("");

// iCloud
const icloudEmail = ref("");
const icloudAppPassword = ref("");

// Custom IMAP
const imapEmail = ref("");
const imapServer = ref("");
const imapPort = ref(993);
const imapPassword = ref("");

const tabs = [
  { id: "google" as const, name: "Google / Gmail", icon: Cloud },
  { id: "microsoft365" as const, name: "Microsoft 365", icon: ShieldCheck },
  { id: "icloud" as const, name: "iCloud", icon: KeyRound },
  { id: "imap" as const, name: "IMAP Custom", icon: Server },
];

function close() {
  errorMessage.value = "";
  emit("close");
}

async function connectGmailAppPassword() {
  try {
    errorMessage.value = "";
    isSubmitting.value = true;
    await mailStore.addImapAccount({
      email: gmailEmail.value.trim(),
      password: gmailAppPassword.value.trim().replace(/\s+/g, ""),
      server: "imap.gmail.com",
      port: 993,
      providerType: "gmail",
    });
    close();
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}

async function connectGoogleOAuth() {
  try {
    errorMessage.value = "";
    isSubmitting.value = true;
    await mailStore.startOAuthFlow("google", googleClientId.value.trim() || undefined);
    close();
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}

async function connectMicrosoftOAuth() {
  try {
    errorMessage.value = "";
    isSubmitting.value = true;
    await mailStore.startOAuthFlow("microsoft365", m365ClientId.value.trim() || undefined);
    close();
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}

async function connectOutlookAppPassword() {
  try {
    errorMessage.value = "";
    isSubmitting.value = true;
    await mailStore.addImapAccount({
      email: outlookEmail.value.trim(),
      password: outlookAppPassword.value.trim().replace(/\s+/g, ""),
      server: "outlook.office365.com",
      port: 993,
      providerType: "outlook",
    });
    close();
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}

async function connectIcloud() {
  try {
    errorMessage.value = "";
    isSubmitting.value = true;
    await mailStore.addImapAccount({
      email: icloudEmail.value.trim(),
      password: icloudAppPassword.value.trim().replace(/\s+/g, ""),
      server: "imap.mail.me.com",
      port: 993,
      providerType: "icloud",
    });
    close();
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}

async function connectImap() {
  try {
    errorMessage.value = "";
    isSubmitting.value = true;
    await mailStore.addImapAccount({
      email: imapEmail.value.trim(),
      password: imapPassword.value.trim(),
      server: imapServer.value.trim(),
      port: imapPort.value || 993,
      providerType: "imap",
    });
    close();
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    isSubmitting.value = false;
  }
}
</script>
