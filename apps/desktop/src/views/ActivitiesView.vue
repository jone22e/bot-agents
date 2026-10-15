<script setup lang="ts">
import { onMounted, ref } from "vue";
import EmptyState from "@/components/EmptyState.vue";
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

onMounted(async () => {
  if (!runtime.isConnected) return;
  if (dots.items.length === 0) await dots.load();
  await Promise.allSettled([tasks.load(), ops.load(), approvals.load()]);
  if (!newDot.value && dots.active[0]) newDot.value = dots.active[0].id;
});
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
const failureKind = (e: string | null) => (e ? describeFailure(e.includes("Not logged in") || e.includes("login") ? "auth" : "engine", e) : "");
</script>

<template>
  <div class="flex h-full flex-col">
  <PageHeader title="Atividades" subtitle="Tudo o que os DOTs fizeram, estao fazendo ou esperam de voce" />
  <div v-if="approvals.pending.length" class="mx-6 mb-3 space-y-2">
    <p class="text-xs font-semibold uppercase tracking-wide text-amber-300">Esperando voce ({{ approvals.pending.length }})</p>
    <ApprovalCard v-for="a in approvals.pending" :key="a.id" :approval="a" :busy="approvals.deciding !== null" @approve="approvals.decide(a.id, true)" @reject="approvals.decide(a.id, false)" @answer="(t) => approvals.decide(a.id, true, t)" />
  </div>
  <div class="flex min-h-0 flex-1 gap-4 px-6 pb-6">
    <section class="flex w-1/2 min-w-0 flex-col gap-3">
      <div class="card">
        <p class="mb-2 text-xs font-medium text-zinc-400">Pedir um trabalho</p>
        <div class="flex gap-2">
          <select v-model="newDot" class="input w-44"><option value="" disabled>Para qual DOT</option><option v-for="d in dots.active" :key="d.id" :value="d.id">{{ d.name }}</option></select>
          <input v-model="newPrompt" class="input" placeholder="O que ele deve fazer" @keyup.enter="createTask" />
          <button class="btn-primary" :disabled="!newDot || !newPrompt.trim()" @click="createTask">Enviar</button>
        </div>
      </div>
      <div class="flex items-center justify-between text-xs text-zinc-400">
        <div class="flex items-center gap-3">
          <label class="flex items-center gap-1"><input v-model="tasks.onlyActive" type="checkbox" @change="tasks.load()" /> so em andamento</label>
          <select v-model="tasks.filterDot" class="input w-40" @change="tasks.load()"><option :value="null">todos os DOTs</option><option v-for="d in dots.items" :key="d.id" :value="d.id">{{ d.name }}</option></select>
        </div>
        <button class="btn-ghost" @click="togglePause">{{ ops.stats?.paused ? "Retomar a fila" : "Pausar tudo" }}</button>
      </div>
      <ErrorBanner :message="tasks.error" />
      <EmptyState v-if="!tasks.loading && tasks.items.length === 0" title="Nenhum trabalho ainda" description="Peca um trabalho acima ou converse com um DOT. Tudo o que ele fizer aparece aqui." />
      <ul v-else class="flex-1 space-y-1 overflow-y-auto">
        <li v-for="t in tasks.items" :key="t.id">
          <button class="w-full rounded-md border px-3 py-2 text-left hover:bg-zinc-900" :class="t.id === tasks.selectedId ? 'border-indigo-700 bg-zinc-900' : 'border-zinc-800'" @click="tasks.select(t.id)">
            <div class="flex items-center justify-between gap-2">
              <span class="truncate text-sm text-zinc-100">{{ t.title }}</span>
              <TaskStatusBadge :status="t.status" />
            </div>
            <p class="mt-0.5 text-xs text-zinc-500">{{ dotName(t.dot_id) }} · {{ describeOrigin(t, dotName) }} · {{ new Date(t.created_at).toLocaleString() }}</p>
          </button>
        </li>
      </ul>
    </section>

    <section class="flex w-1/2 min-w-0 flex-col overflow-y-auto">
      <template v-if="!tasks.selected">
        <OpsPanel />
        <EmptyState class="mt-3" title="Escolha um trabalho" description="Voce vera o que o DOT fez, passo a passo, e o resultado." />
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
  </div>
</template>
