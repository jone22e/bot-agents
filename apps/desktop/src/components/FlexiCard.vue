<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { Building2, Globe, Plus, RefreshCw, ShieldCheck, Trash2, Users } from "lucide-vue-next";
import flexiIcon from "@/assets/flexi-icon.svg";
import type { AppConnection } from "@dots/shared-types";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import FlexiBrowserLogin from "@/components/FlexiBrowserLogin.vue";
import Toggle from "@/components/Toggle.vue";
import { ACCOUNT_CONNECTION, AGENTS_CONNECTION, useAppsStore } from "@/stores/apps";
import { useSettingsStore } from "@/stores/settings";

defineProps<{ agentSetup?: boolean }>();
const apps = useAppsStore();
const settings = useSettingsStore();
const router = useRouter();
const showForm = ref(false);
const form = reactive({ label: "", email: "", senha: "", remember_password: true, advanced: false, base_url: "" });
const mfaCode = reactive<Record<string, string>>({});
const reauth = reactive<Record<string, string>>({});
const toRemove = ref<AppConnection | null>(null);
const teamFor = ref<AppConnection | null>(null);
const selectionError = ref<string | null>(null);

onMounted(() => { void apps.load(); void apps.loadLink(); });
const statusLabel = computed(() => (c: AppConnection) => ({ connected: "Conectado", needs_login: "Precisa entrar de novo", mfa_pending: "Aguardando codigo 2FA", error: "Com problema" }[c.status]));
const statusCls = computed(() => (c: AppConnection) => ({ connected: "bg-emerald-500/10 text-emerald-400", needs_login: "bg-amber-500/10 text-amber-300", mfa_pending: "bg-amber-500/15 text-amber-200", error: "bg-red-500/10 text-red-300" }[c.status]));
const dotCls = computed(() => (c: AppConnection) => ({ connected: "bg-emerald-400", needs_login: "bg-amber-400", mfa_pending: "bg-amber-300", error: "bg-red-400" }[c.status]));
const displayName = (c: AppConnection) => c.user_name ?? c.email;
const accountId = computed(() => settings.items[ACCOUNT_CONNECTION]);
const agentsId = computed(() => settings.items[AGENTS_CONNECTION]);
const agentConnection = computed(() => apps.flexi.find((c) => c.id === agentsId.value));

async function selectForAgents(id: string) {
  selectionError.value = null;
  try { await settings.set(AGENTS_CONNECTION, id); }
  catch (e) { selectionError.value = e instanceof Error ? e.message : String(e); }
}

async function connect() {
  if (!form.email.trim() || !form.senha) return;
  const c = await apps.connect({ label: form.label || null, base_url: form.advanced && form.base_url ? form.base_url : null, server: null, email: form.email.trim(), senha: form.senha, remember_password: form.remember_password });
  form.senha = "";
  if (c) showForm.value = false;
}
async function createTeam() {
  const c = teamFor.value;
  teamFor.value = null;
  if (!c) return;
  const r = await apps.createTeam(c.id);
  if (r) router.push("/");
}
</script>

<template>
  <div class="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/60">
    <div class="flex items-start gap-4 p-5">
      <img :src="flexiIcon" alt="" class="h-11 w-11 shrink-0 rounded-xl" />
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <h3 class="text-base font-semibold text-zinc-100">Flexi</h3>
          <span v-if="apps.flexi.length" class="rounded-full border border-zinc-700 px-2 py-0.5 text-[11px] text-zinc-400">{{ apps.flexi.length }} {{ apps.flexi.length === 1 ? "usuário" : "usuários" }}</span>
        </div>
        <p class="mt-1 max-w-3xl text-[13px] leading-relaxed text-zinc-400">Deixa os DOTs trabalharem com anúncios, perguntas, análises e Google Ads do Flexi, no nome do usuário que você escolher. A empresa e a base vêm da conta desse usuário, como no login do Flexi.</p>
      </div>
    </div>
    <div v-if="apps.error || selectionError" class="px-5 pb-4"><ErrorBanner :message="apps.error ?? selectionError" /></div>

    <ul v-if="apps.flexi.length" class="divide-y divide-zinc-800 border-t border-zinc-800">
      <li v-for="c in apps.flexi" :key="c.id" class="px-5 py-3.5">
        <div class="flex items-center justify-between gap-3">
          <div class="flex min-w-0 items-center gap-3">
            <span class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-violet-500/20 text-sm font-semibold text-violet-300">{{ displayName(c).charAt(0).toUpperCase() }}</span>
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <p class="truncate text-sm font-semibold text-zinc-100" :title="`${c.label} · ${c.email}`">{{ displayName(c) }}</p>
                <span class="inline-flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-medium" :class="statusCls(c)"><span class="h-1.5 w-1.5 rounded-full" :class="dotCls(c)"></span>{{ statusLabel(c) }}</span>
                <span v-if="c.id === accountId" class="shrink-0 rounded-full border border-zinc-700 px-2 py-0.5 text-[11px] text-zinc-300">Sua conta</span>
                <span v-if="c.id === agentsId" class="shrink-0 rounded-full bg-violet-500/15 px-2 py-0.5 text-[11px] text-violet-300">Conta dos agentes</span>
              </div>
              <p class="mt-0.5 flex flex-wrap items-center gap-x-4 text-xs text-zinc-500">
                <span v-if="c.empresa" class="flex items-center gap-1"><Building2 :size="12" /> {{ c.empresa }}</span>
                <span class="flex items-center gap-1 font-mono"><Globe :size="12" /> {{ c.base_url.replace(/^https?:\/\//, '') }}</span>
                <span v-if="c.remember_password" class="flex items-center gap-1"><ShieldCheck :size="12" /> Reconecta sozinho</span>
              </p>
              <p v-if="c.status_detail && c.status !== 'connected'" class="mt-0.5 text-xs text-amber-300">{{ c.status_detail }}</p>
            </div>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            <button class="inline-flex h-8 items-center gap-1.5 rounded-lg border border-zinc-700 px-3 text-xs font-medium text-zinc-200 hover:bg-zinc-800 disabled:opacity-50" title="Verificar a sessão e reconectar com a senha guardada" :disabled="apps.busy" @click="apps.check(c.id)"><RefreshCw :size="13" /> Reconectar</button>
            <button v-if="!agentSetup" class="flex h-8 w-8 items-center justify-center rounded-lg border border-zinc-700 text-zinc-300 hover:bg-zinc-800 disabled:opacity-50" title="Montar equipe de marketplaces com a conta dos agentes" :disabled="apps.busy || c.status !== 'connected' || c.id !== agentsId" @click="teamFor = c"><Users :size="14" /></button>
            <button class="flex h-8 w-8 items-center justify-center rounded-lg border border-red-900/60 text-red-400 hover:bg-red-500/10" title="Remover" @click="toRemove = c"><Trash2 :size="14" /></button>
          </div>
        </div>
        <div v-if="c.status === 'mfa_pending'" class="ml-12 mt-2 flex items-center gap-2">
          <input v-model="mfaCode[c.id]" class="input w-40" placeholder="código 2FA" maxlength="8" @keyup.enter="apps.mfa(c.id, mfaCode[c.id] ?? '')" />
          <button class="btn-primary" :disabled="apps.busy || !(mfaCode[c.id] ?? '').trim()" @click="apps.mfa(c.id, mfaCode[c.id] ?? '')">Confirmar</button>
        </div>
        <div v-else-if="c.status !== 'connected'" class="ml-12 mt-2 flex items-center gap-2">
          <input v-model="reauth[c.id]" class="input w-56" type="password" placeholder="senha para entrar de novo" @keyup.enter="apps.reconnect(c.id, reauth[c.id])" />
          <button class="btn-primary" :disabled="apps.busy" @click="apps.reconnect(c.id, reauth[c.id])">Entrar de novo</button>
          <span v-if="!agentSetup" class="text-xs text-zinc-500">ou use Logar com Flexi, abaixo</span>
        </div>
        <button v-if="c.id !== agentsId" class="ml-12 mt-2 text-xs font-medium text-violet-300 hover:text-violet-200" @click="selectForAgents(c.id)">Usar esta conta nos agentes</button>
      </li>
    </ul>

    <div v-if="apps.flexi.length" class="border-t border-zinc-800 px-5 py-3 text-xs text-zinc-400">
      <p v-if="accountId && apps.link" class="mb-1.5 flex items-center gap-1.5" :class="apps.link.connected ? 'text-emerald-400' : 'text-zinc-500'">
        <span class="h-1.5 w-1.5 rounded-full" :class="apps.link.connected ? 'bg-emerald-400' : 'bg-zinc-600'"></span>
        <span v-if="apps.link.connected">Conectado ao Flexi web</span>
        <span v-else>Sem conexão com o Flexi web{{ apps.link.last_error ? `: ${apps.link.last_error}` : '' }}</span>
      </p>
      <p v-if="agentConnection">Os agentes acessam o Flexi como <strong class="text-zinc-200">{{ displayName(agentConnection) }}</strong>. Sua conta continua sendo usada para entrar no DOTS.</p>
      <p v-else>Escolha acima uma conta para os agentes acessarem o Flexi. Sua conta de login não é usada automaticamente por eles.</p>
    </div>

    <div class="border-t border-zinc-800 px-5 py-3.5">
      <div v-if="showForm" class="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
        <div class="grid grid-cols-2 gap-3">
          <div><label class="label">E-mail do usuário</label><input v-model="form.email" class="input" type="email" placeholder="pode ser outro usuário, não o seu" /></div>
          <div><label class="label">Senha</label><input v-model="form.senha" class="input" type="password" @keyup.enter="connect" /></div>
          <div class="col-span-2"><label class="label">Nome desta conexão (opcional)</label><input v-model="form.label" class="input" placeholder="ex.: Flexi · Maria (marketplaces)" /></div>
        </div>
        <div class="mt-3 space-y-2">
          <Toggle v-model="form.remember_password" label="Guardar a senha no Keychain do Mac para reconectar sozinho" description="A sessão do Flexi expira em um ou dois dias. Sem a senha guardada, você precisa entrar de novo a cada expiração. O código 2FA sempre é pedido a você." />
          <details class="text-xs" @toggle="form.advanced = ($event.target as HTMLDetailsElement).open"><summary class="cursor-pointer text-zinc-500 hover:text-zinc-300">Avançado</summary>
            <label class="label mt-2">Endereço do Flexi</label><input v-model="form.base_url" class="input font-mono text-xs" placeholder="https://flexi.c2s-business.com (padrão) ou http://127.0.0.1:2012" />
          </details>
        </div>
        <div class="mt-3 flex justify-end gap-2"><button class="btn-secondary" @click="showForm = false">Cancelar</button><button class="btn-primary" :disabled="apps.busy || !form.email.trim() || !form.senha" @click="connect">{{ apps.busy ? "Entrando..." : "Entrar no Flexi" }}</button></div>
      </div>
      <template v-else>
        <p v-if="!apps.flexi.length" class="mb-3 text-xs text-zinc-500">Nenhuma conexão ainda. Ao conectar, os modelos de DOT para marketplaces ficam disponíveis.</p>
        <div v-if="!agentSetup" class="mb-3">
          <FlexiBrowserLogin :base-url="form.advanced && form.base_url ? form.base_url : null" />
          <p v-if="!apps.browserLogin" class="mt-1.5 text-xs text-zinc-500">Abre o Flexi no navegador, onde você já está logado, e liga a sua conta ao app. Se a sessão expirar, use de novo.</p>
        </div>
        <button class="inline-flex items-center gap-2 rounded-lg border border-dashed border-violet-500/50 bg-violet-500/10 px-4 py-2 text-[13px] font-medium text-violet-300 hover:bg-violet-500/15" @click="showForm = true"><Plus :size="14" /> {{ apps.flexi.length ? "Conectar outro usuário" : "Conectar usuário" }}</button>
      </template>
    </div>

    <ConfirmDialog :open="toRemove !== null" title="Remover conexao com o Flexi" :message="`Remover “${toRemove?.label ?? ''}”? Os DOTs ligados a ela perdem o acesso ao Flexi. A senha e a sessao guardadas sao apagadas do Keychain.`" confirm-label="Remover" danger @confirm="apps.remove(toRemove!.id); toRemove = null" @cancel="toRemove = null" />
    <ConfirmDialog :open="teamFor !== null" title="Montar equipe de marketplaces" message="Cria 11 DOTs prontos (Gerente de Marketplaces, subgerentes e agentes de Mercado Livre, Shopee, Google Ads e Publicacao), ligados a esta conexao e com a delegacao configurada. Todos comecam no nivel 'so le e pergunta antes de mudar'. Voce pode renomear, ajustar ou excluir depois." confirm-label="Montar equipe" @confirm="createTeam" @cancel="teamFor = null" />
  </div>
</template>
