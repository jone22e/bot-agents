<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { Plus } from "lucide-vue-next";
import type { Schedule, ScheduleKind } from "@dots/shared-types";
import ErrorBanner from "@/components/ErrorBanner.vue";
import { EVENT_TYPE_LABEL, describeSchedule, describeSubscription } from "@/lib/humanize";
import { useAutomationStore } from "@/stores/automation";
import { useDotsStore } from "@/stores/dots";

const props = defineProps<{ dotId: string }>();
const store = useAutomationStore();
const dots = useDotsStore();
const trigger = reactive({ event_type: "file.changed", path_glob: "", source_dot_id: "", prompt_template: "", retry_safe: false });
const sched = reactive({ kind: "daily" as ScheduleKind, spec: "09:00", prompt: "" });
/** Formularios ficam escondidos ate o usuario pedir para adicionar. */
const addingTrigger = ref(false);
const addingSchedule = ref(false);
const editingSchedule = ref<string | null>(null);
const scheduleDraft = reactive({ kind: "every" as ScheduleKind, spec: "60", prompt: "" });
const dotName = (id: string) => dots.items.find((d) => d.id === id)?.name ?? "um DOT";
const otherDots = computed(() => dots.items.filter((d) => d.id !== props.dotId));
const needsPath = computed(() => trigger.event_type === "file.changed" || trigger.event_type === "git.commit.created");
const needsSource = computed(() => trigger.event_type.startsWith("task.") || trigger.event_type.startsWith("approval."));

onMounted(() => store.load(props.dotId));
async function addTrigger() {
  if (!trigger.prompt_template.trim()) return;
  await store.createSubscription({ dot_id: props.dotId, event_type: trigger.event_type, filter: { path_glob: needsPath.value && trigger.path_glob ? trigger.path_glob : null, source_dot_id: needsSource.value && trigger.source_dot_id ? trigger.source_dot_id : null }, prompt_template: trigger.prompt_template.trim(), retry_safe: trigger.retry_safe, cooldown_seconds: 60 });
  trigger.prompt_template = "";
  addingTrigger.value = false;
}
async function addSchedule() {
  if (!sched.prompt.trim()) return;
  await store.createSchedule({ dot_id: props.dotId, kind: sched.kind, spec: sched.spec.trim(), prompt: sched.prompt.trim() });
  sched.prompt = "";
  addingSchedule.value = false;
}
function editSchedule(s: Schedule) {
  scheduleDraft.kind = s.kind;
  scheduleDraft.spec = s.spec;
  scheduleDraft.prompt = s.prompt;
  editingSchedule.value = s.id;
}
async function saveSchedule() {
  if (!editingSchedule.value || !scheduleDraft.spec.trim() || !scheduleDraft.prompt.trim()) return;
  await store.updateSchedule(editingSchedule.value, { kind: scheduleDraft.kind, spec: scheduleDraft.spec.trim(), prompt: scheduleDraft.prompt.trim() });
  if (!store.error) editingSchedule.value = null;
}
const specHint: Record<ScheduleKind, string> = { every: "minutos, ex.: 30", daily: "HH:MM, ex.: 09:00", weekly: "dia(0=dom) HH:MM, ex.: 1 08:00", cron: "5 campos, ex.: 0 9 * * 1-5" };
</script>

<template>
  <div class="max-w-2xl space-y-5">
    <ErrorBanner :message="store.error" />
    <p class="text-xs text-zinc-500">O DOT inicia seu objetivo ao ser criado. Gerentes recebem uma agenda de acompanhamento que você pode editar, pausar ou remover aqui.</p>

    <section class="card space-y-3">
      <div class="flex items-center justify-between gap-3">
        <h3 class="text-sm font-semibold text-zinc-200">Quando acontecer algo</h3>
        <button v-if="!addingTrigger" class="btn-secondary" @click="addingTrigger = true"><Plus :size="14" /> Gatilho</button>
      </div>
      <ul v-if="store.subscriptions.length" class="divide-y divide-zinc-800 text-sm">
        <li v-for="s in store.subscriptions" :key="s.id" class="flex items-center justify-between gap-3 py-2">
          <span :class="s.enabled ? 'text-zinc-200' : 'text-zinc-500 line-through'">{{ describeSubscription(s, dotName) }}</span>
          <span class="flex shrink-0 gap-2 text-xs">
            <button class="text-indigo-400 hover:underline" @click="store.updateSubscription(s.id, { enabled: !s.enabled })">{{ s.enabled ? "pausar" : "ativar" }}</button>
            <button class="text-red-300 hover:underline" @click="store.deleteSubscription(s.id)">remover</button>
          </span>
        </li>
      </ul>
      <p v-else-if="!addingTrigger" class="text-xs text-zinc-500">Nenhum gatilho. Ex.: quando um arquivo mudar, revisar.</p>
      <div v-if="addingTrigger" class="space-y-3 border-t border-zinc-800 pt-3">
        <div>
          <label class="label">Quando</label>
          <select v-model="trigger.event_type" class="input"><option v-for="(label, k) in EVENT_TYPE_LABEL" :key="k" :value="k">{{ label }}</option></select>
        </div>
        <div v-if="needsPath">
          <label class="label">Quais arquivos (opcional)</label>
          <input v-model="trigger.path_glob" class="input font-mono text-xs" placeholder="*.md ou docs/**" />
        </div>
        <div v-if="needsSource">
          <label class="label">De qual DOT (opcional)</label>
          <select v-model="trigger.source_dot_id" class="input"><option value="">qualquer</option><option v-for="d in otherDots" :key="d.id" :value="d.id">{{ d.name }}</option></select>
        </div>
        <div>
          <label class="label">Pedir</label>
          <input v-model="trigger.prompt_template" class="input" placeholder="Ex.: Revise o arquivo {{ '{{' }}path{{ '}}' }} e aponte problemas" @keyup.enter="addTrigger" />
          <p class="mt-1 text-xs text-zinc-500">Pode usar <code v-pre>{{path}}</code>, <code v-pre>{{sha}}</code>, <code v-pre>{{message}}</code>, <code v-pre>{{title}}</code>, <code v-pre>{{result}}</code>. Mesmo evento repetido nunca gera dois trabalhos; ha um intervalo minimo de 1 minuto entre disparos.</p>
        </div>
        <label class="flex items-center gap-2 text-xs text-zinc-400"><input v-model="trigger.retry_safe" type="checkbox" /> pode ser refeito automaticamente se o servico reiniciar</label>
        <div class="flex justify-end gap-2">
          <button class="btn-ghost" @click="addingTrigger = false">Cancelar</button>
          <button class="btn-primary" :disabled="!trigger.prompt_template.trim()" @click="addTrigger">Adicionar gatilho</button>
        </div>
      </div>
    </section>

    <section class="card space-y-3">
      <div class="flex items-center justify-between gap-3">
        <h3 class="text-sm font-semibold text-zinc-200">Agenda de trabalho</h3>
        <button v-if="!addingSchedule" class="btn-secondary" @click="addingSchedule = true"><Plus :size="14" /> Horario</button>
      </div>
      <ul v-if="store.schedules.length" class="divide-y divide-zinc-800 text-sm">
        <li v-for="s in store.schedules" :key="s.id" class="py-2">
          <div class="flex items-center justify-between gap-3">
            <span :class="s.enabled ? 'text-zinc-200' : 'text-zinc-500 line-through'">{{ describeSchedule(s) }}: "{{ s.prompt }}"<span v-if="s.next_run_at" class="ml-2 text-xs text-zinc-500">próximo {{ new Date(s.next_run_at).toLocaleString() }}</span></span>
            <span class="flex shrink-0 gap-2 text-xs">
              <button class="text-indigo-400 hover:underline" @click="editSchedule(s)">editar</button>
              <button class="text-indigo-400 hover:underline" @click="store.updateSchedule(s.id, { enabled: !s.enabled })">{{ s.enabled ? "pausar" : "ativar" }}</button>
              <button class="text-red-300 hover:underline" @click="store.deleteSchedule(s.id)">remover</button>
            </span>
          </div>
          <div v-if="editingSchedule === s.id" class="mt-3 space-y-2 rounded-lg border border-zinc-800 p-3">
            <div class="grid grid-cols-[150px_1fr] gap-3">
              <select v-model="scheduleDraft.kind" class="input"><option value="every">a cada N minutos</option><option value="daily">todo dia</option><option value="weekly">toda semana</option><option value="cron">cron</option></select>
              <input v-model="scheduleDraft.spec" class="input font-mono text-xs" :placeholder="specHint[scheduleDraft.kind]" />
            </div>
            <input v-model="scheduleDraft.prompt" class="input" placeholder="O que o DOT deve fazer" @keyup.enter="saveSchedule" />
            <div class="flex justify-end gap-2">
              <button class="btn-ghost" @click="editingSchedule = null">Cancelar</button>
              <button class="btn-primary" :disabled="!scheduleDraft.spec.trim() || !scheduleDraft.prompt.trim()" @click="saveSchedule">Salvar</button>
            </div>
          </div>
        </li>
      </ul>
      <p v-else-if="!addingSchedule" class="text-xs text-zinc-500">Nenhum horario. Ex.: todo dia as 9h, verificar os testes.</p>
      <div v-if="addingSchedule" class="space-y-3 border-t border-zinc-800 pt-3">
        <div class="grid grid-cols-[150px_1fr] gap-3">
          <div>
            <label class="label">Repetir</label>
            <select v-model="sched.kind" class="input"><option value="every">a cada N minutos</option><option value="daily">todo dia</option><option value="weekly">toda semana</option><option value="cron">cron</option></select>
          </div>
          <div>
            <label class="label">Quando ({{ specHint[sched.kind] }})</label>
            <input v-model="sched.spec" class="input font-mono text-xs" />
          </div>
        </div>
        <div>
          <label class="label">Pedir</label>
          <input v-model="sched.prompt" class="input" placeholder="Ex.: Verifique os testes e me avise se algo quebrou" @keyup.enter="addSchedule" />
        </div>
        <div class="flex justify-end gap-2">
          <button class="btn-ghost" @click="addingSchedule = false">Cancelar</button>
          <button class="btn-primary" :disabled="!sched.prompt.trim()" @click="addSchedule">Agendar</button>
        </div>
      </div>
    </section>
  </div>
</template>
