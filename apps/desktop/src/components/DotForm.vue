<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import type { Dot, DotCreate, DotLimits, DotUpdate, EngineKind } from "@dots/shared-types";
import { ENGINE_LABEL, splitPurpose } from "@/lib/humanize";
import { DOT_TEMPLATES, TRUST_OPTIONS, type DotTemplate } from "@/lib/templates";
import { useEnginesStore } from "@/stores/engines";
import ModelPicker from "@/components/ModelPicker.vue";
import { modelLabel } from "@/lib/models";

const props = defineProps<{ dot?: Dot | null; saving: boolean }>();
const emit = defineEmits<{ create: [input: DotCreate, trust: "read" | "write" | "exec"]; update: [patch: DotUpdate]; cancel: [] }>();

const engines = useEnginesStore();
const ready = computed(() => engines.items.filter((e) => e.enabled && e.last_detection?.installed && e.last_detection.auth_status !== "unauthenticated"));
const defaultLimits: DotLimits = { max_execution_seconds: 1800, max_concurrent_tasks: 1, max_tokens_per_task: null, max_cost_usd_per_task: null };

const form = reactive({
  supervisor: props.dot?.role === "supervisor",
  name: props.dot?.name ?? "",
  purpose: props.dot ? props.dot.instructions || props.dot.objective : "",
  engine: (props.dot?.primary_engine ?? ready.value[0]?.kind ?? "") as EngineKind | "",
  model: props.dot?.model ?? "",
  trust: "read" as "read" | "write" | "exec",
  workspace_path: props.dot?.workspace_path ?? "",
  limits: { ...(props.dot?.limits ?? defaultLimits) } as DotLimits,
});
watch(ready, (r) => { if (!props.dot && form.engine === "" && r[0]) form.engine = r[0].kind; }, { immediate: true });
// Ao trocar de assistente, o modelo do anterior nao serve mais.
watch(() => form.engine, () => { form.model = ""; });
const engineDefaultModel = computed(() => (form.engine ? engines.byKind(form.engine)?.model ?? null : null));
const engineCatalog = computed(() => (form.engine ? engines.byKind(form.engine)?.last_detection?.models ?? [] : []));
const advanced = ref(false);
const error = ref<string | null>(null);

function applyTemplate(t: DotTemplate) {
  if (t.id === "blank") return;
  if (!form.name) form.name = t.name;
  form.purpose = t.purpose;
  form.trust = t.trust;
}

function submit() {
  error.value = null;
  if (!form.name.trim()) { error.value = "De um nome ao DOT."; return; }
  if (!form.purpose.trim()) { error.value = "Descreva o que ele faz."; return; }
  const { objective, instructions } = splitPurpose(form.purpose);
  const engine = form.engine === "" ? null : form.engine;
  const model = form.model.trim() || null;
  if (model && /\s/.test(model)) { error.value = "O nome do modelo nao pode ter espacos."; return; }
  const limits: DotLimits = { ...form.limits, max_tokens_per_task: form.limits.max_tokens_per_task || null, max_cost_usd_per_task: form.limits.max_cost_usd_per_task || null };
  if (props.dot) {
    const patch: DotUpdate = { name: form.name.trim(), objective, instructions, primary_engine: engine, model, limits, role: form.supervisor ? "supervisor" : "worker" };
    if (form.workspace_path !== (props.dot.workspace_path ?? "")) patch.workspace_path = form.workspace_path || null;
    emit("update", patch);
  } else {
    emit("create", { name: form.name.trim(), description: "", objective, instructions, primary_engine: engine, model, workspace_path: form.workspace_path || null, limits, role: form.supervisor ? "supervisor" : "worker" }, form.trust);
  }
}
</script>

<template>
  <form class="max-w-2xl space-y-5" @submit.prevent="submit">
    <div v-if="error" class="rounded-md border border-red-900/60 bg-red-950/40 px-3 py-2 text-sm text-red-200">{{ error }}</div>

    <div v-if="!dot">
      <label class="label">Comecar com um modelo (opcional)</label>
      <div class="flex flex-wrap gap-2">
        <button v-for="t in DOT_TEMPLATES.filter((x) => x.id !== 'blank')" :key="t.id" type="button" class="btn-secondary" @click="applyTemplate(t)">{{ t.name }}</button>
      </div>
    </div>

    <div>
      <label class="label" for="name">Nome</label>
      <input id="name" v-model="form.name" class="input" maxlength="80" placeholder="Ex.: Revisor de codigo" required />
    </div>

    <div>
      <label class="label" for="purpose">O que ele faz</label>
      <textarea id="purpose" v-model="form.purpose" class="input" rows="6" placeholder="Descreva em portugues o trabalho deste DOT, como faria com uma pessoa nova na equipe."></textarea>
      <p class="mt-1 text-xs text-zinc-500">Isso vira a identidade permanente do DOT. Voce pode mudar depois.</p>
    </div>

    <div>
      <label class="label" for="engine">Assistente de IA que ele usa</label>
      <select id="engine" v-model="form.engine" class="input">
        <option v-for="e in ready" :key="e.kind" :value="e.kind">{{ ENGINE_LABEL[e.kind] }}</option>
        <option value="">Escolher depois</option>
      </select>
      <p v-if="!ready.length" class="mt-1 text-xs text-amber-300">Nenhum assistente pronto. Veja Configuracoes > Assistentes de IA.</p>
    </div>

    <label class="flex cursor-pointer items-start gap-3 rounded-md border border-zinc-800 p-3 hover:bg-zinc-900/60" :class="form.supervisor ? 'border-indigo-600 bg-zinc-900/60' : ''">
      <input v-model="form.supervisor" type="checkbox" class="mt-1" />
      <span><span class="block text-sm text-zinc-100">Este DOT coordena outros (Supervisor)</span><span class="block text-xs text-zinc-500">Divide objetivos, pede trabalho a DOTs da equipe, acompanha e consolida. A equipe e definida na aba Equipe.</span></span>
    </label>

    <div v-if="!dot">
      <label class="label">Quanto ele pode fazer sozinho</label>
      <div class="space-y-2">
        <label v-for="o in TRUST_OPTIONS" :key="o.value" class="flex cursor-pointer items-start gap-3 rounded-md border border-zinc-800 p-3 hover:bg-zinc-900/60" :class="form.trust === o.value ? 'border-indigo-600 bg-zinc-900/60' : ''">
          <input v-model="form.trust" type="radio" :value="o.value" class="mt-1" />
          <span><span class="block text-sm text-zinc-100">{{ o.title }}</span><span class="block text-xs text-zinc-500">{{ o.description }}</span></span>
        </label>
      </div>
      <p class="mt-1 text-xs text-zinc-500">Pastas e comandos liberados ficam na aba Permissoes depois de criar.</p>
    </div>

    <details class="card" :open="advanced" @toggle="advanced = ($event.target as HTMLDetailsElement).open">
      <summary class="cursor-pointer text-sm text-zinc-300">Avancado</summary>
      <div class="mt-4 grid grid-cols-2 gap-4">
        <div>
          <label class="label" for="workspace">Pasta de trabalho</label>
          <input id="workspace" v-model="form.workspace_path" class="input font-mono text-xs" placeholder="Vazio = pasta propria criada pelo DOTS" />
        </div>
        <div>
          <label class="label" for="maxsec">Tempo maximo por trabalho (minutos)</label>
          <input id="maxsec" :value="Math.round(form.limits.max_execution_seconds / 60)" type="number" min="1" class="input" @input="form.limits.max_execution_seconds = Math.max(60, Number(($event.target as HTMLInputElement).value) * 60)" />
        </div>
        <div>
          <label class="label" for="maxcost">Gasto maximo por trabalho (US$)</label>
          <input id="maxcost" v-model.number="form.limits.max_cost_usd_per_task" type="number" min="0" step="0.5" class="input" placeholder="sem limite" />
          <p class="mt-1 text-xs text-zinc-500">So o Claude informa custo; os outros informam apenas tokens.</p>
        </div>
        <div>
          <label class="label" for="maxconc">Trabalhos ao mesmo tempo</label>
          <input id="maxconc" v-model.number="form.limits.max_concurrent_tasks" type="number" min="1" max="4" class="input" />
        </div>
        <div class="col-span-2">
          <label class="label" for="model">Modelo do assistente</label>
          <ModelPicker id="model" v-model="form.model" :engine="form.engine" :disabled="form.engine === ''" :empty-label="engineDefaultModel ? `${modelLabel(form.engine, engineDefaultModel, engineCatalog)} (padrao em Configuracoes)` : 'Modelo padrao do assistente'" />
          <p class="mt-1 text-xs text-zinc-500">So para este DOT. O padrao de cada assistente fica em Configuracoes > Assistentes de IA.</p>
        </div>
      </div>
    </details>

    <div class="flex justify-end gap-2">
      <button type="button" class="btn-secondary" @click="emit('cancel')">Cancelar</button>
      <button type="submit" class="btn-primary" :disabled="saving">{{ saving ? "Salvando..." : dot ? "Salvar" : "Criar DOT" }}</button>
    </div>
  </form>
</template>
