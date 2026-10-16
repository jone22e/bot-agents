<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Check, ChevronDown } from "lucide-vue-next";
import ErrorBanner from "@/components/ErrorBanner.vue";
import EventTimeline from "@/components/EventTimeline.vue";
import OpsPanel from "@/components/OpsPanel.vue";
import TaskStatusBadge from "@/components/TaskStatusBadge.vue";
import { api } from "@/api";
import { describeFailure, describeOrigin } from "@/lib/humanize";
import { useDotsStore } from "@/stores/dots";
import { useApprovalsStore } from "@/stores/approvals";
import { useOpsStore } from "@/stores/ops";
import ApprovalCard from "@/components/ApprovalCard.vue";
import PageHeader from "@/components/PageHeader.vue";
import { useRuntimeStore } from "@/stores/runtime";
import { useTasksStore } from "@/stores/tasks";

const tasks = useTasksStore();
const dots = useDotsStore();
const ops = useOpsStore();
const approvals = useApprovalsStore();
const runtime = useRuntimeStore();
const newPrompt = ref("");
const newDot = ref("");
const showOps = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;

onMounted(async () => {
  timer = setInterval(() => { if (runtime.isConnected) ops.load(); }, 5000);
  if (!runtime.isConnected) return;
  if (dots.items.length === 0) await dots.load();
  await Promise.allSettled([tasks.load(), ops.load(), approvals.load()]);
  if (!newDot.value && dots.active[0]) newDot.value = dots.active[0].id;
});
onUnmounted(() => { if (timer) clearInterval(timer); });
function dotName(id: string) { return dots.items.find((d) => d.id === id)?.name ?? "DOT"; }
async function createTask() {
  if (!newDot.value || !newPrompt.value.trim()) return;
  await tasks.create(newDot.value, newPrompt.value.trim());
  newPrompt.value = "";
}
async function togglePause() {
  await api.runtime.pauseAll(!(ops.stats?.paused ?? false));
  await ops.load();
}
function setOnlyActive(v: boolean) {
  if (tasks.onlyActive === v) return;
  tasks.onlyActive = v;
  tasks.load();
}
function openOps() {
  tasks.select(null);
  showOps.value = true;
}
function when(iso: string) {
  const d = new Date(iso);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return d.toDateString() === new Date().toDateString() ? time : `${d.toLocaleDateString([], { day: "2-digit", month: "2-digit" })} ${time}`;
}
const procCpu = computed(() => (ops.stats?.processes ?? []).reduce((n, p) => n + p.cpu_percent, 0));
const procMb = computed(() => (ops.stats?.processes ?? []).reduce((n, p) => n + p.rss_bytes, 0) / 1048576);
const failureKind = (e: string | null) => (e ? describeFailure(e.includes("Not logged in") || e.includes("login") ? "auth" : "engine", e) : "");
</script>

<template>
  <div class="flex h-full flex-col">
  <PageHeader title="Atividades">
    <template #subtitle>
      <span v-if="ops.stats" class="flex flex-wrap items-center gap-x-4">
        <span class="flex items-center gap-1.5 text-zinc-200"><span class="h-1.5 w-1.5 rounded-full bg-violet-500"></span>{{ ops.stats.running }} trabalhando</span>
        <span>{{ ops.stats.queued }} na fila</span>
        <span :class="ops.stats.blocked > 0 ? 'text-amber-300' : ''">{{ ops.stats.blocked }} esperando você</span>
        <span :class="ops.stats.failures_24h > 0 ? 'text-red-300' : ''">{{ ops.stats.failures_24h }} não deram certo em 24 h</span>
        <span v-if="ops.stats.paused" class="text-amber-300">fila pausada</span>
      </span>
      <template v-else>Tudo o que os DOTs fizeram, estão fazendo ou esperam de você</template>
    </template>
    <button class="rounded-lg border border-zinc-700 px-4 py-2 text-sm font-medium text-zinc-100 hover:bg-zinc-800" @click="togglePause">{{ ops.stats?.paused ? "Retomar a fila" : "Pausar tudo" }}</button>
  </PageHeader>

  <div class="mx-6 mb-4 flex items-center gap-2 rounded-xl border border-zinc-800 bg-zinc-900/60 p-1.5">
    <div class="relative shrink-0">
      <select v-model="newDot" class="h-9 cursor-pointer appearance-none rounded-lg bg-zinc-800 pl-3 pr-8 text-sm font-medium text-zinc-100 focus:outline-none"><option value="" disabled>Para qual DOT</option><option v-for="d in dots.active" :key="d.id" :value="d.id">{{ d.name }}</option></select>
      <ChevronDown :size="14" class="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-400" />
    </div>
    <input v-model="newPrompt" class="h-9 min-w-0 flex-1 bg-transparent px-2 text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none" placeholder="Peça um trabalho: o que ele deve fazer?" @keyup.enter="createTask" />
    <button class="h-9 shrink-0 rounded-lg bg-violet-500 px-5 text-sm font-semibold text-white hover:bg-violet-400 disabled:cursor-not-allowed disabled:opacity-50" :disabled="!newDot || !newPrompt.trim()" @click="createTask">Enviar</button>
  </div>

  <div v-if="approvals.pending.length" class="mx-6 mb-3 space-y-2">
    <p class="text-xs font-semibold uppercase tracking-wide text-amber-300">Esperando você ({{ approvals.pending.length }})</p>
    <ApprovalCard v-for="a in approvals.pending" :key="a.id" :approval="a" :busy="approvals.deciding !== null" @approve="approvals.decide(a.id, true)" @reject="approvals.decide(a.id, false)" @answer="(t) => approvals.decide(a.id, true, t)" />
  </div>

  <div class="flex min-h-0 flex-1 gap-6 px-6">
    <section class="flex min-w-0 basis-3/5 flex-col">
      <div class="mb-2 flex items-center justify-between">
        <div class="flex gap-5 text-sm">
          <button class="border-b-2 pb-1.5 font-semibold" :class="!tasks.onlyActive ? 'border-violet-500 text-zinc-100' : 'border-transparent text-zinc-500 hover:text-zinc-300'" @click="setOnlyActive(false)">Todos</button>
          <button class="border-b-2 pb-1.5 font-semibold" :class="tasks.onlyActive ? 'border-violet-500 text-zinc-100' : 'border-transparent text-zinc-500 hover:text-zinc-300'" @click="setOnlyActive(true)">Em andamento</button>
        </div>
        <select v-model="tasks.filterDot" class="cursor-pointer bg-transparent text-xs text-zinc-500 hover:text-zinc-300 focus:outline-none" @change="tasks.load()"><option :value="null">todos os DOTs</option><option v-for="d in dots.items" :key="d.id" :value="d.id">{{ d.name }}</option></select>
      </div>
      <ErrorBanner :message="tasks.error" />
      <div v-if="!tasks.loading && tasks.items.length === 0" class="py-16 text-center">
        <p class="text-sm font-medium text-zinc-200">Nenhum trabalho ainda</p>
        <p class="mt-1 text-xs text-zinc-500">Peça um trabalho acima ou converse com um DOT. Tudo o que ele fizer aparece aqui.</p>
      </div>
      <ul v-else class="flex-1 overflow-y-auto pb-4 pr-2">
        <li v-for="t in tasks.items" :key="t.id">
          <button class="flex w-full items-center justify-between gap-3 rounded-lg px-3.5 py-2.5 text-left hover:bg-zinc-900" :class="t.id === tasks.selectedId ? 'bg-zinc-900' : ''" @click="tasks.select(t.id); showOps = false">
            <div class="min-w-0">
              <p class="truncate text-sm text-zinc-100">{{ t.title }}</p>
              <p class="mt-0.5 text-xs text-zinc-500" :title="describeOrigin(t, dotName)">{{ dotName(t.dot_id) }} · {{ when(t.created_at) }}</p>
            </div>
            <span v-if="t.status === 'RUNNING'" class="inline-flex shrink-0 items-center gap-1.5 rounded-full bg-violet-500/15 px-2.5 py-0.5 text-xs font-medium text-violet-300"><span class="h-1.5 w-1.5 rounded-full bg-violet-400"></span>Trabalhando</span>
            <Check v-else-if="t.status === 'COMPLETED'" :size="15" class="mr-1 shrink-0 text-emerald-400" />
            <TaskStatusBadge v-else class="shrink-0" :status="t.status" />
          </button>
        </li>
      </ul>
    </section>

    <section class="flex min-w-0 basis-2/5 flex-col overflow-y-auto pb-4">
      <template v-if="!tasks.selected">
        <OpsPanel v-if="showOps" />
        <div v-else class="flex flex-1 flex-col items-center justify-center rounded-xl border border-zinc-800 bg-zinc-900/40 px-8 text-center">
          <p class="text-sm font-semibold text-zinc-100">Escolha um trabalho</p>
          <p class="mt-1 max-w-xs text-xs leading-relaxed text-zinc-500">Você verá o que o DOT fez, passo a passo, e o resultado.</p>
        </div>
      </template>
      <template v-else>
        <div class="card">
          <div class="flex items-start justify-between gap-2">
            <div class="min-w-0">
              <h2 class="truncate text-sm font-semibold text-zinc-100">{{ tasks.selected.task.title }}</h2>
              <p class="text-xs text-zinc-500">{{ dotName(tasks.selected.task.dot_id) }} · {{ describeOrigin(tasks.selected.task, dotName) }}<template v-if="tasks.selected.task.attempts > 1"> · {{ tasks.selected.task.attempts }}ª tentativa</template></p>
              <button v-if="tasks.selected.task.parent_task_id" class="text-xs text-indigo-400 hover:underline" @click="tasks.select(tasks.selected!.task.parent_task_id!)">ver o pedido que originou</button>
            </div>
            <div class="flex items-center gap-2">
              <TaskStatusBadge :status="tasks.selected.task.status" />
              <button v-if="['QUEUED','RUNNING','WAITING_APPROVAL'].includes(tasks.selected.task.status)" class="btn-danger" @click="tasks.cancel(tasks.selected.task.id)">Parar</button>
              <button v-else-if="['FAILED','CANCELLED','WAITING_USER'].includes(tasks.selected.task.status)" class="btn-secondary" @click="tasks.retry(tasks.selected.task.id)">Tentar de novo</button>
            </div>
          </div>
          <p v-if="tasks.selected.task.error" class="mt-2 rounded bg-red-950/30 p-2 text-xs text-red-200">{{ failureKind(tasks.selected.task.error) }}</p>
          <div v-if="tasks.selected.task.result_text" class="mt-2">
            <p class="text-xs font-medium text-zinc-400">Resultado</p>
            <pre class="mt-1 max-h-60 overflow-auto whitespace-pre-wrap rounded bg-emerald-950/20 p-2 font-sans text-sm text-emerald-50">{{ tasks.selected.task.result_text }}</pre>
          </div>
          <details class="mt-2 text-xs"><summary class="cursor-pointer text-zinc-500 hover:text-zinc-300">Pedido original</summary><pre class="mt-1 whitespace-pre-wrap font-sans text-zinc-300">{{ tasks.selected.task.prompt }}</pre></details>
        </div>
        <div class="mt-3">
          <h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-400">O que aconteceu</h3>
          <EventTimeline :events="tasks.events" :live-text="tasks.liveText" />
        </div>
        <div v-if="tasks.delegations.length" class="mt-3 card">
          <h3 class="text-xs font-semibold uppercase tracking-wide text-zinc-400">Pedidos a outros DOTs</h3>
          <ul class="mt-2 space-y-1 text-xs">
            <li v-for="d in tasks.delegations" :key="d.id" class="flex items-center justify-between gap-2">
              <span class="min-w-0 truncate text-zinc-300">→ <strong>{{ d.to_dot_name }}</strong>: {{ d.request }}</span>
              <span class="flex shrink-0 items-center gap-2">
                <span class="badge" :class="d.status === 'completed' ? 'bg-emerald-500/15 text-emerald-300' : d.status === 'failed' || d.status === 'cancelled' ? 'bg-red-500/15 text-red-300' : 'bg-indigo-500/20 text-indigo-200'">{{ { pending: 'na fila', running: 'trabalhando', completed: 'concluido', failed: 'falhou', cancelled: 'cancelado' }[d.status] }}</span>
                <button v-if="d.child_task_id" class="text-indigo-400 hover:underline" @click="tasks.select(d.child_task_id!)">abrir</button>
              </span>
            </li>
          </ul>
        </div>
        <div v-if="tasks.checkpoints.length > 1" class="mt-3 card">
          <h3 class="text-xs font-semibold uppercase tracking-wide text-zinc-400">Pontos de controle</h3>
          <ul class="mt-2 space-y-1 text-xs text-zinc-300"><li v-for="c in tasks.checkpoints" :key="c.id">{{ new Date(c.created_at).toLocaleTimeString() }} · {{ c.step }}<span v-if="c.next_action" class="text-zinc-500"> → {{ c.next_action }}</span></li></ul>
        </div>
        <div v-if="tasks.changes.length" class="mt-3 card">
          <h3 class="text-xs font-semibold uppercase tracking-wide text-zinc-400">Arquivos tocados</h3>
          <ul class="mt-2 space-y-1 text-xs">
            <li v-for="c in tasks.changes" :key="c.id"><span class="text-zinc-400">{{ { create: 'criou', modify: 'alterou', delete: 'excluiu', command: 'rodou comando em', git: 'git em' }[c.kind] ?? c.kind }}</span> <span class="font-mono text-zinc-300">{{ c.path.split('/').slice(-2).join('/') }}</span></li>
          </ul>
        </div>
      </template>
    </section>
  </div>

  <footer class="flex items-center justify-end gap-4 px-6 py-3 text-xs text-zinc-500">
    <span :title="ops.stats?.processes.length ? 'CPU e memória dos assistentes em execução' : undefined">{{ runtime.isConnected ? "Serviço ativo" : "Serviço desconectado" }}<template v-if="ops.stats?.processes.length"> · CPU {{ procCpu.toFixed(0) }}% · {{ procMb.toFixed(0) }} MB</template></span>
    <button class="font-medium text-violet-400 hover:text-violet-300" @click="showOps ? (showOps = false) : openOps()">{{ showOps ? "Ocultar detalhes" : "Ver detalhes" }}</button>
  </footer>
  </div>
</template>
