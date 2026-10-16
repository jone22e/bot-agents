<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { ArrowLeft, ArrowRight, Check, Dices, UserRound, Users } from "lucide-vue-next";
import type { EngineKind, FlexiRoleTemplate } from "@dots/shared-types";
import Avatar from "@/components/Avatar.vue";
import { api, RuntimeError } from "@/api";
import { ENGINE_LABEL, splitPurpose } from "@/lib/humanize";
import { randomName } from "@/lib/names";
import { TRUST_OPTIONS } from "@/lib/templates";
import { useAppsStore } from "@/stores/apps";
import { useDotsStore } from "@/stores/dots";
import { useEnginesStore } from "@/stores/engines";
import { useTeamsStore } from "@/stores/teams";

const router = useRouter();
const dots = useDotsStore();
const engines = useEnginesStore();
const apps = useAppsStore();
const teams = useTeamsStore();
type Step = 1 | 2 | 3;
const step = ref<Step>(1);
const saving = ref(false);
const error = ref<string | null>(null);
const form = reactive({ name: "", purpose: "", trust: "read" as "read" | "write" | "exec", engine: "" as EngineKind | "", tipo: "funcionario" as "funcionario" | "gerente", teamId: "", newTeamName: "" });
/** Sorteia um nome comum que nenhum DOT usa. */
const genName = () => (form.name = randomName(dots.items.map((d) => d.name), form.name));
const showEngine = ref(false);
const showTrust = ref(false);
/** Papel do Flexi escolhido no passo 2 (preenche nome e descricao; vai para o Flexi como template_id). */
const picked = ref<FlexiRoleTemplate | null>(null);
/** Especialidades do Flexi: sempre de gerente (o gerente contrata os funcionarios e define o que cada um faz). */
const flexiRoles = computed<FlexiRoleTemplate[]>(() => (apps.flexiConnected && form.tipo === "gerente" ? apps.templates.filter((t) => t.role === "supervisor") : []));
function pickRole(t: FlexiRoleTemplate) {
  picked.value = t;
  form.name = t.name;
  form.purpose = t.purpose;
  form.trust = t.trust === "none" ? "read" : t.trust;
}

const ready = computed(() => engines.items.filter((e) => e.enabled && e.last_detection?.installed && e.last_detection.auth_status !== "unauthenticated"));
const engineLabel = computed(() => (form.engine ? ENGINE_LABEL[form.engine] : "nenhum assistente pronto"));
const trustLabel = computed(() => TRUST_OPTIONS.find((o) => o.value === form.trust)?.title ?? "");
/** Com o Flexi conectado, as permissoes sao sobre o Flexi, nao sobre arquivos. */
const FLEXI_TRUST: { value: "read" | "write"; title: string; description: string }[] = [
  { value: "read", title: "Somente consulta no Flexi", description: "Le anuncios, perguntas, analises e campanhas. Qualquer alteracao pede sua permissao." },
  { value: "write", title: "Pode editar no Flexi", description: "Altera precos, conteudo, estoque, respostas e campanhas sem perguntar. Criar, publicar, finalizar e copiar anuncios sempre pedem permissao." },
];
const TIPOS = [
  { id: "gerente" as const, title: "Gerente", text: "Recebe objetivos, divide o trabalho e delega aos funcionários da equipe dele. Acompanha e consolida.", icon: Users },
  { id: "funcionario" as const, title: "Funcionário", text: "Executa o que você ou o gerente dele pedem. Não delega.", icon: UserRound },
];
/** Cor da inicial fixa por DOT (a mesma da tela de Equipes). */
const SOFT = ["bg-violet-500/20 text-violet-300", "bg-emerald-500/20 text-emerald-300", "bg-amber-500/20 text-amber-300", "bg-sky-500/20 text-sky-300", "bg-rose-500/20 text-rose-300", "bg-teal-500/20 text-teal-300"];
const avatarCls = (id: string) => SOFT[[...id].reduce((n, c) => n + c.charCodeAt(0), 0) % SOFT.length];
const dotName = (id: string | null | undefined) => (id ? dots.items.find((d) => d.id === id)?.name ?? "?" : null);
/** Equipes que um gerente novo pode assumir: as que ainda nao tem gerente. */
const openTeams = computed(() => teams.withoutManager);
/** Resumo da escolha de equipe para o passo final. */
const teamSummary = computed(() => {
  if (form.tipo === "gerente") {
    if (form.teamId) return `Gerente da equipe ${teams.byId(form.teamId)?.name ?? ""}`;
    if (form.newTeamName.trim()) return `Gerente da nova equipe ${form.newTeamName.trim()}`;
    return "Gerente sem equipe ainda";
  }
  const t = form.teamId ? teams.byId(form.teamId) : null;
  if (!t) return "Sem equipe por enquanto";
  const m = dotName(t.manager_dot_id);
  return m ? `Funcionario de ${m} (equipe ${t.name})` : `Funcionario da equipe ${t.name} (ainda sem gerente)`;
});

onMounted(async () => {
  if (engines.items.length === 0) await engines.load();
  if (!form.engine && ready.value[0]) form.engine = ready.value[0].kind;
  await Promise.all([apps.load(), teams.load(), dots.items.length ? Promise.resolve() : dots.load()]);
  if (apps.flexiConnected) await apps.loadTemplates();
});

function next() {
  error.value = null;
  if (step.value === 1) {
    // Trocou tipo ou setor: o papel escolhido antes pode nao valer mais.
    if (picked.value && !flexiRoles.value.some((t) => t.id === picked.value!.id)) picked.value = null;
    step.value = 2;
  } else if (step.value === 2) {
    if (!form.name.trim()) { error.value = "De um nome a ele."; return; }
    if (!form.purpose.trim()) { error.value = "Diga em uma frase o que ele faz."; return; }
    step.value = 3;
  }
}
async function create() {
  saving.value = true;
  error.value = null;
  try {
    const { objective, instructions } = splitPurpose(form.purpose);
    const d = await dots.create({ name: form.name.trim(), description: "", objective, instructions, primary_engine: form.engine || null, model: null, workspace_path: null, limits: null, role: form.tipo === "gerente" ? "supervisor" : "worker" });
    await api.policies.set(d.id, { auto_approve_level: form.trust });
    const flexi = apps.flexi.find((c) => c.status === "connected");
    // Liga o DOT ao Flexi; gerente com especialidade leva o setor dela.
    if (apps.flexiConnected && flexi) await api.apps.assignRole(d.id, flexi.id, { template_id: picked.value?.id ?? null, group: picked.value?.group ?? null, role: form.tipo === "gerente" ? "supervisor" : "worker" });
    // Equipe: gerente assume uma equipe sem gerente ou cria uma nova; funcionario entra na equipe escolhida.
    if (form.tipo === "gerente") {
      if (form.teamId) await api.teams.setManager(form.teamId, d.id);
      else if (form.newTeamName.trim()) await api.teams.create({ name: form.newTeamName.trim(), description: "", manager_dot_id: d.id, member_ids: [] });
    } else if (form.teamId) {
      await api.teams.addMember(form.teamId, d.id);
    }
    await teams.load();
    router.replace(`/chat/${d.id}`);
  } catch (e) {
    error.value = e instanceof RuntimeError ? e.message : String(e);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="flex items-center gap-3 px-6 py-4" data-tauri-drag-region>
      <button class="flex h-8 w-8 items-center justify-center rounded-full bg-zinc-800 text-zinc-300 hover:bg-zinc-700" @click="step > 1 ? (step = (step - 1) as Step) : router.back()"><ArrowLeft :size="16" /></button>
      <div class="flex gap-1.5"><span v-for="i in 3" :key="i" class="h-1.5 w-8 rounded-full" :class="i <= step ? 'bg-indigo-500' : 'bg-zinc-800'"></span></div>
    </header>

    <div class="flex flex-1 items-start justify-center overflow-y-auto px-6 pb-10 pt-6">
      <div class="w-full" :class="step === 1 ? 'max-w-[600px]' : 'max-w-lg'">
        <p v-if="error" class="mb-4 rounded-md bg-red-950/40 px-3 py-2 text-sm text-red-200">{{ error }}</p>

        <!-- Passo 1: tipo e equipe -->
        <template v-if="step === 1">
          <h1 class="text-[28px] font-semibold tracking-tight text-zinc-50">Seu novo DOT é gerente ou funcionário?</h1>
          <p class="mt-1.5 text-sm text-zinc-400">Primeiro o papel dele, depois o setor.</p>
          <div class="mt-7 grid grid-cols-2 gap-3">
            <button v-for="o in TIPOS" :key="o.id" class="rounded-2xl border p-4 text-left transition-colors" :class="form.tipo === o.id ? 'border-violet-500 bg-violet-500/10' : 'border-zinc-800 bg-zinc-900/60 hover:border-zinc-700'" @click="form.tipo = o.id; form.teamId = ''">
              <span class="flex items-start justify-between">
                <span class="flex h-9 w-9 items-center justify-center rounded-lg" :class="form.tipo === o.id ? 'bg-violet-500/20 text-violet-300' : 'bg-zinc-800 text-zinc-400'"><component :is="o.icon" :size="16" /></span>
                <span class="radio" :class="{ on: form.tipo === o.id }" />
              </span>
              <span class="mt-3 block text-sm font-semibold text-zinc-100">{{ o.title }}</span>
              <span class="mt-1 block text-[13px] leading-relaxed text-zinc-400">{{ o.text }}</span>
            </button>
          </div>

          <template v-if="form.tipo === 'gerente'">
            <p class="mb-2 mt-7 text-[13px] font-semibold text-zinc-300">Gerente do quê?</p>
            <div class="divide-y divide-zinc-800 overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900/60">
              <label v-for="t in openTeams" :key="t.id" class="row" :class="{ on: form.teamId === t.id }">
                <input v-model="form.teamId" type="radio" :value="t.id" class="sr-only" @change="form.newTeamName = ''" />
                <span class="avatar bg-zinc-800 text-zinc-400">{{ t.name.charAt(0).toUpperCase() }}</span>
                <span class="min-w-0 flex-1"><span class="block text-sm font-semibold text-zinc-100">{{ t.name }}</span><span class="block text-xs text-zinc-500">{{ t.member_ids.length ? `${t.member_ids.length} ${t.member_ids.length === 1 ? "funcionário" : "funcionários"}, sem gerente` : "Equipe vazia, sem gerente" }}</span></span>
                <span class="radio" :class="{ on: form.teamId === t.id }" />
              </label>
              <label class="row" :class="{ on: form.teamId === '' }">
                <input v-model="form.teamId" type="radio" value="" class="sr-only" />
                <span class="avatar bg-zinc-800 text-zinc-400">+</span>
                <span class="min-w-0 flex-1"><span class="block text-sm font-semibold text-zinc-100">Nova equipe</span><input v-model="form.newTeamName" class="input mt-1.5 h-9" placeholder="nome da equipe, ex.: Mercado Livre" :disabled="form.teamId !== ''" /></span>
                <span class="radio" :class="{ on: form.teamId === '' }" />
              </label>
            </div>
            <p class="mt-2 text-xs text-zinc-500">Equipes que já têm gerente não aparecem. Para trocar o gerente de uma equipe, use Equipes no menu.</p>
          </template>
          <template v-else>
            <p class="mb-2 mt-7 text-[13px] font-semibold text-zinc-300">Funcionário de quem?</p>
            <div class="divide-y divide-zinc-800 overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900/60">
              <label v-for="t in teams.items" :key="t.id" class="row" :class="{ on: form.teamId === t.id }">
                <input v-model="form.teamId" type="radio" :value="t.id" class="sr-only" />
                <span v-if="t.manager_dot_id" class="avatar" :class="avatarCls(t.manager_dot_id)">{{ (dotName(t.manager_dot_id) ?? "?").charAt(0).toUpperCase() }}</span>
                <span v-else class="avatar border border-zinc-700 text-zinc-500">–</span>
                <span class="min-w-0 flex-1"><span class="block text-sm font-semibold text-zinc-100">{{ dotName(t.manager_dot_id) ?? "Sem gerente ainda" }}</span><span class="block text-xs text-zinc-500">Equipe {{ t.name }}{{ t.member_ids.length ? ` · ${t.member_ids.length} ${t.member_ids.length === 1 ? "funcionário" : "funcionários"}` : "" }}</span></span>
                <span class="radio" :class="{ on: form.teamId === t.id }" />
              </label>
              <label class="row" :class="{ on: form.teamId === '' }">
                <input v-model="form.teamId" type="radio" value="" class="sr-only" />
                <span class="avatar border border-zinc-700 text-zinc-500">–</span>
                <span class="min-w-0 flex-1"><span class="block text-sm font-semibold text-zinc-100">Sem equipe por enquanto</span><span class="block text-xs text-zinc-500">Você escolhe depois em Equipes</span></span>
                <span class="radio" :class="{ on: form.teamId === '' }" />
              </label>
            </div>
          </template>
          <div class="mt-10 flex items-center justify-between">
            <button class="text-[13px] font-medium text-zinc-400 hover:text-zinc-200" @click="router.back()">Cancelar</button>
            <button class="flex h-11 items-center gap-2 rounded-xl bg-violet-500 px-5 text-sm font-semibold text-white hover:bg-violet-400" @click="next">Continuar <ArrowRight :size="15" /></button>
          </div>
        </template>

        <!-- Passo 2: nome e o que faz -->
        <template v-else-if="step === 2">
          <div class="flex items-center gap-3"><Avatar :name="form.name || '?'" size="lg" /><h1 class="text-2xl font-semibold text-zinc-50">Como ele se chama e o que faz?</h1></div>
          <template v-if="flexiRoles.length">
            <p class="mt-5 text-xs font-medium uppercase tracking-wide text-zinc-500">Especialidade do gerente no Flexi</p>
            <ul class="mt-2 divide-y divide-zinc-800 overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/60">
              <li v-for="t in flexiRoles" :key="t.id">
                <button class="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-zinc-800/70" :class="picked?.id === t.id ? 'bg-indigo-500/10' : ''" @click="pickRole(t)">
                  <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border" :class="picked?.id === t.id ? 'border-indigo-500 bg-indigo-500 text-white' : 'border-zinc-700 text-transparent'"><Check :size="14" /></span>
                  <span class="min-w-0 flex-1"><span class="block truncate text-sm font-medium text-zinc-100">{{ t.name }}</span><span class="block truncate text-xs text-zinc-500">{{ t.group_name }} · {{ t.purpose }}</span></span>
                </button>
              </li>
            </ul>
            <div class="mt-6 flex items-center gap-3">
              <span class="h-px flex-1 bg-zinc-800"></span><span class="text-xs text-zinc-500">ou do zero</span><span class="h-px flex-1 bg-zinc-800"></span>
            </div>
          </template>
          <div class="mt-5 flex gap-2">
            <input v-model="form.name" class="h-11 min-w-0 flex-1 rounded-xl border border-zinc-800 bg-zinc-900 px-4 text-base text-zinc-100 placeholder-zinc-500 focus:border-indigo-500 focus:outline-none" placeholder="Nome do DOT" maxlength="80" :autofocus="!flexiRoles.length" />
            <button type="button" class="flex h-11 shrink-0 items-center gap-2 rounded-xl border border-zinc-800 bg-zinc-900 px-4 text-sm text-zinc-200 hover:bg-zinc-800" title="Gerar um nome comum" @click="genName"><Dices :size="16" /> Gerar nome</button>
          </div>
          <p class="mt-4 text-sm text-zinc-400">Explique o que ele faz como faria para uma pessoa nova na equipe. Isso vira a identidade dele; da para mudar depois.</p>
          <textarea v-model="form.purpose" class="mt-2 w-full rounded-xl border border-zinc-800 bg-zinc-900 px-4 py-3 text-base text-zinc-100 placeholder-zinc-500 focus:border-indigo-500 focus:outline-none" rows="6" placeholder="Ex.: Revisa o codigo da pasta de trabalho e aponta bugs com o arquivo e a linha. Nunca altera arquivos, so recomenda."></textarea>
          <div class="mt-4 flex justify-end gap-2">
            <button class="btn-secondary h-11" @click="step = 1">Voltar</button>
            <button class="btn-primary h-11 px-5" :disabled="!form.name.trim() || !form.purpose.trim()" @click="next">Continuar</button>
          </div>
        </template>

        <!-- Passo 3: confirmar -->
        <template v-else>
          <div class="flex items-center gap-3"><Avatar :name="form.name" size="lg" /><div><h1 class="text-2xl font-semibold text-zinc-50">{{ form.name }}</h1><p class="text-sm text-zinc-400">Pronto para conversar.</p></div></div>
          <div class="mt-6 rounded-2xl border border-zinc-800 bg-zinc-900/60 p-4">
            <p class="text-xs font-medium uppercase tracking-wide text-zinc-500">O que ele faz</p>
            <textarea v-model="form.purpose" class="mt-1 w-full resize-none bg-transparent text-sm leading-relaxed text-zinc-200 focus:outline-none" rows="4"></textarea>
          </div>
          <div class="mt-3 space-y-2 text-sm">
            <div class="flex items-center justify-between rounded-xl bg-zinc-900/60 px-4 py-3">
              <span class="text-zinc-300">Usa <strong class="text-zinc-100">{{ engineLabel }}</strong></span>
              <button v-if="ready.length > 1" class="text-xs text-indigo-400 hover:underline" @click="showEngine = !showEngine">trocar</button>
            </div>
            <div v-if="showEngine" class="flex flex-wrap gap-2 px-1"><button v-for="e in ready" :key="e.kind" class="rounded-full px-3 py-1 text-xs" :class="form.engine === e.kind ? 'bg-indigo-600 text-white' : 'bg-zinc-800 text-zinc-300'" @click="form.engine = e.kind; showEngine = false">{{ ENGINE_LABEL[e.kind] }}</button></div>
            <p v-if="!ready.length" class="px-1 text-xs text-amber-300">Nenhum assistente pronto. Voce pode criar agora e ligar um em Configuracoes > Assistentes de IA.</p>
            <div class="flex items-center justify-between rounded-xl bg-zinc-900/60 px-4 py-3">
              <span class="text-zinc-300">{{ teamSummary }}</span>
              <button class="text-xs text-indigo-400 hover:underline" @click="step = 1">mudar</button>
            </div>
            <template v-if="apps.flexiConnected">
              <div class="rounded-xl bg-zinc-900/60 px-4 py-3">
                <p class="text-xs font-medium uppercase tracking-wide text-zinc-500">No Flexi</p>
                <div class="mt-2 space-y-1">
                  <label v-for="o in FLEXI_TRUST" :key="o.value" class="flex cursor-pointer items-start gap-2 rounded-lg px-2 py-1.5 hover:bg-zinc-800/60"><input v-model="form.trust" type="radio" :value="o.value" class="mt-1" /><span><span class="block text-sm text-zinc-200">{{ o.title }}</span><span class="block text-xs text-zinc-500">{{ o.description }}</span></span></label>
                </div>
              </div>
            </template>
            <template v-else>
              <div class="flex items-center justify-between rounded-xl bg-zinc-900/60 px-4 py-3">
                <span class="text-zinc-300">{{ trustLabel }}</span>
                <button class="text-xs text-indigo-400 hover:underline" @click="showTrust = !showTrust">mudar</button>
              </div>
              <div v-if="showTrust" class="space-y-1 px-1">
                <label v-for="o in TRUST_OPTIONS" :key="o.value" class="flex cursor-pointer items-start gap-2 rounded-lg px-2 py-1.5 hover:bg-zinc-900"><input v-model="form.trust" type="radio" :value="o.value" class="mt-1" /><span><span class="block text-sm text-zinc-200">{{ o.title }}</span><span class="block text-xs text-zinc-500">{{ o.description }}</span></span></label>
              </div>
            </template>
            <p class="px-1 text-xs text-zinc-500">Pastas, gatilhos, equipe e memoria ficam no perfil dele, pelo nome no topo da conversa.</p>
          </div>
          <div class="mt-6 flex justify-end gap-2">
            <button class="btn-secondary h-11" @click="step = 2">Voltar</button>
            <button class="btn-primary h-11 px-6" :disabled="saving || !form.name.trim() || !form.purpose.trim()" @click="create">{{ saving ? "Criando..." : "Criar e conversar" }}</button>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Passo 1: linhas de escolha e o radio redondo do desenho */
.row { display: flex; cursor: pointer; align-items: center; gap: 12px; padding: 12px 16px; }
.row:hover { background: rgb(39 39 42 / 0.5); }
.row.on { background: rgb(139 92 246 / 0.08); }
.avatar { display: flex; height: 32px; width: 32px; flex: none; align-items: center; justify-content: center; border-radius: 9999px; font-size: 12px; font-weight: 600; }
.radio { position: relative; height: 18px; width: 18px; flex: none; border-radius: 9999px; border: 1.5px solid var(--color-zinc-600); }
.radio.on { border-color: var(--color-violet-400); }
.radio.on::after { content: ""; position: absolute; inset: 3px; border-radius: 9999px; background: var(--color-violet-400); }
</style>
