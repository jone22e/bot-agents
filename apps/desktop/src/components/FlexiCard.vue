<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { Building2, KeyRound, Link2, RefreshCw, ShieldCheck, Trash2, Users } from "lucide-vue-next";
import flexiIcon from "@/assets/flexi-icon.svg";
import type { AppConnection } from "@dots/shared-types";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import Toggle from "@/components/Toggle.vue";
import { useAppsStore } from "@/stores/apps";

const apps = useAppsStore();
const router = useRouter();
const showForm = ref(false);
const form = reactive({ label: "", email: "", senha: "", remember_password: true, advanced: false, base_url: "" });
const mfaCode = reactive<Record<string, string>>({});
const reauth = reactive<Record<string, string>>({});
const toRemove = ref<AppConnection | null>(null);
const teamFor = ref<AppConnection | null>(null);

onMounted(() => apps.load());
const statusLabel = computed(() => (c: AppConnection) => ({ connected: "Conectado", needs_login: "Precisa entrar de novo", mfa_pending: "Aguardando codigo 2FA", error: "Com problema" }[c.status]));
const statusCls = computed(() => (c: AppConnection) => ({ connected: "bg-emerald-500/15 text-emerald-300", needs_login: "bg-amber-500/15 text-amber-300", mfa_pending: "bg-amber-500/25 text-amber-200", error: "bg-red-500/15 text-red-300" }[c.status]));

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
  <div class="card">
    <div class="flex items-start justify-between gap-4">
      <div class="flex items-start gap-3">
        <img :src="flexiIcon" alt="" class="h-10 w-10 shrink-0" />
        <div>
          <h3 class="text-sm font-semibold text-zinc-100">Flexi</h3>
          <p class="text-xs text-zinc-500">Deixa os DOTs trabalharem com anuncios, perguntas, analises e Google Ads do Flexi, no nome do usuario que voce escolher. A empresa e a base vem da conta desse usuario, como no login do Flexi.</p>
        </div>
      </div>
      <button class="btn-primary" @click="showForm = !showForm">{{ apps.flexi.length ? "Conectar outro usuario" : "Conectar" }}</button>
    </div>
    <ErrorBanner :message="apps.error" />

    <div v-if="showForm" class="mt-4 rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
      <div class="grid grid-cols-2 gap-3">
        <div><label class="label">E-mail do usuario</label><input v-model="form.email" class="input" type="email" placeholder="pode ser outro usuario, nao o seu" /></div>
        <div><label class="label">Senha</label><input v-model="form.senha" class="input" type="password" @keyup.enter="connect" /></div>
        <div class="col-span-2"><label class="label">Nome desta conexao (opcional)</label><input v-model="form.label" class="input" placeholder="ex.: Flexi · Maria (marketplaces)" /></div>
      </div>
      <div class="mt-3 space-y-2">
        <Toggle v-model="form.remember_password" label="Guardar a senha no Keychain do Mac para reconectar sozinho" description="A sessao do Flexi expira em um ou dois dias. Sem a senha guardada, voce precisa entrar de novo a cada expiracao. O codigo 2FA sempre e pedido a voce." />
        <details class="text-xs" @toggle="form.advanced = ($event.target as HTMLDetailsElement).open"><summary class="cursor-pointer text-zinc-500 hover:text-zinc-300">Avancado</summary>
          <label class="label mt-2">Endereco do Flexi</label><input v-model="form.base_url" class="input font-mono text-xs" placeholder="https://flexi.c2s-business.com (padrao) ou http://127.0.0.1:2012" />
        </details>
      </div>
      <div class="mt-3 flex justify-end gap-2"><button class="btn-secondary" @click="showForm = false">Cancelar</button><button class="btn-primary" :disabled="apps.busy || !form.email.trim() || !form.senha" @click="connect">{{ apps.busy ? "Entrando..." : "Entrar no Flexi" }}</button></div>
    </div>

    <ul v-if="apps.flexi.length" class="mt-4 divide-y divide-zinc-800">
      <li v-for="c in apps.flexi" :key="c.id" class="py-3">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <p class="truncate text-sm font-medium text-zinc-100">{{ c.label }}</p>
            <p class="flex flex-wrap items-center gap-x-3 text-xs text-zinc-500">
              <span class="flex items-center gap-1"><KeyRound :size="12" /> {{ c.user_name ?? c.email }}</span>
              <span v-if="c.empresa" class="flex items-center gap-1"><Building2 :size="12" /> {{ c.empresa }}</span>
              <span class="flex items-center gap-1"><Link2 :size="12" /> {{ c.base_url.replace(/^https?:\/\//, '') }}</span>
              <span v-if="c.remember_password" class="flex items-center gap-1"><ShieldCheck :size="12" /> reconecta sozinho</span>
            </p>
            <p v-if="c.status_detail && c.status !== 'connected'" class="mt-0.5 text-xs text-amber-300">{{ c.status_detail }}</p>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            <span class="badge" :class="statusCls(c)">{{ statusLabel(c) }}</span>
            <button class="btn-ghost" title="Verificar sessao" :disabled="apps.busy" @click="apps.check(c.id)"><RefreshCw :size="14" /></button>
            <button class="btn-ghost" title="Montar equipe de marketplaces" :disabled="apps.busy || c.status !== 'connected'" @click="teamFor = c"><Users :size="14" /></button>
            <button class="btn-ghost text-red-300" title="Remover" @click="toRemove = c"><Trash2 :size="14" /></button>
          </div>
        </div>
        <div v-if="c.status === 'mfa_pending'" class="mt-2 flex items-center gap-2">
          <input v-model="mfaCode[c.id]" class="input w-40" placeholder="codigo 2FA" maxlength="8" @keyup.enter="apps.mfa(c.id, mfaCode[c.id] ?? '')" />
          <button class="btn-primary" :disabled="apps.busy || !(mfaCode[c.id] ?? '').trim()" @click="apps.mfa(c.id, mfaCode[c.id] ?? '')">Confirmar</button>
        </div>
        <div v-else-if="c.status !== 'connected'" class="mt-2 flex items-center gap-2">
          <input v-model="reauth[c.id]" class="input w-56" type="password" placeholder="senha para entrar de novo" @keyup.enter="apps.reconnect(c.id, reauth[c.id])" />
          <button class="btn-primary" :disabled="apps.busy" @click="apps.reconnect(c.id, reauth[c.id])">Entrar de novo</button>
        </div>
      </li>
    </ul>
    <p v-else-if="!showForm" class="mt-3 text-xs text-zinc-500">Nenhuma conexao ainda. Ao conectar, os modelos de DOT para marketplaces ficam disponiveis.</p>

    <ConfirmDialog :open="toRemove !== null" title="Remover conexao com o Flexi" :message="`Remover “${toRemove?.label ?? ''}”? Os DOTs ligados a ela perdem o acesso ao Flexi. A senha e a sessao guardadas sao apagadas do Keychain.`" confirm-label="Remover" danger @confirm="apps.remove(toRemove!.id); toRemove = null" @cancel="toRemove = null" />
    <ConfirmDialog :open="teamFor !== null" title="Montar equipe de marketplaces" message="Cria 11 DOTs prontos (Gerente de Marketplaces, subgerentes e agentes de Mercado Livre, Shopee, Google Ads e Publicacao), ligados a esta conexao e com a delegacao configurada. Todos comecam no nivel 'so le e pergunta antes de mudar'. Voce pode renomear, ajustar ou excluir depois." confirm-label="Montar equipe" @confirm="createTeam" @cancel="teamFor = null" />
  </div>
</template>
