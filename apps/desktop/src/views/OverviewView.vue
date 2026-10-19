<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import { Check } from "lucide-vue-next";
import type { Contact, Execution } from "@dots/shared-types";
import { api } from "@/api";
import { useContactsStore } from "@/stores/contacts";
import { useTeamsStore } from "@/stores/teams";

/**
 * Visao geral: uma coluna por equipe (gerente no topo, funcionarios embaixo) com a situacao de cada DOT.
 * Quem gerencia uma equipe aparece so na coluna dela, como na lista de contatos.
 */
const router = useRouter();
const contacts = useContactsStore();
const teams = useTeamsStore();
const executions = ref<Execution[]>([]);
const now = ref(Date.now());
const loadedTimes = ref(false);
const timeError = ref(false);
const actionCounts = ref<Record<string, number>>({});
const loadedActions = ref(false);
const actionsError = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;

function dayBounds(date: Date) {
  const y = date.getFullYear();
  const m = date.getMonth();
  const d = date.getDate();
  return { start: new Date(y, m, d), end: new Date(y, m, d + 1) };
}

async function loadReport() {
  now.value = Date.now();
  const { start, end } = dayBounds(new Date(now.value));
  const [times, actions] = await Promise.allSettled([
    api.executions.inPeriod(start.toISOString(), end.toISOString()),
    api.executions.toolCallsInPeriod(start.toISOString(), end.toISOString()),
  ]);
  if (times.status === "fulfilled") {
    executions.value = times.value;
    loadedTimes.value = true;
    timeError.value = false;
  } else {
    timeError.value = true;
  }
  if (actions.status === "fulfilled") {
    actionCounts.value = actions.value;
    loadedActions.value = true;
    actionsError.value = false;
  } else {
    actionsError.value = true;
  }
}

onMounted(() => {
  void loadReport();
  timer = setInterval(() => { void loadReport(); }, 30_000);
});
onUnmounted(() => { if (timer) clearInterval(timer); });

const timeByDot = computed(() => {
  const { start, end } = dayBounds(new Date(now.value));
  const totals = new Map<string, number>();
  for (const execution of executions.value) {
    const from = Math.max(Date.parse(execution.started_at), start.getTime());
    const to = Math.min(execution.finished_at ? Date.parse(execution.finished_at) : now.value, end.getTime(), now.value);
    if (Number.isFinite(from) && Number.isFinite(to) && to > from) {
      totals.set(execution.dot_id, (totals.get(execution.dot_id) ?? 0) + to - from);
    }
  }
  return totals;
});
function formatDuration(milliseconds: number) {
  const minutes = Math.floor(milliseconds / 60_000);
  return `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}min`;
}
function workedToday(dotId: string) {
  return loadedTimes.value && !timeError.value ? formatDuration(timeByDot.value.get(dotId) ?? 0) : "—";
}
function actionsToday(dotId: string) {
  return loadedActions.value && !actionsError.value ? actionCounts.value[dotId] ?? 0 : "—";
}

type Status = "working" | "done" | "waiting" | "idle";
const LEGEND: { key: Status; label: string }[] = [
  { key: "working", label: "Trabalhando" },
  { key: "done", label: "Concluído" },
  { key: "waiting", label: "Aguardando você" },
  { key: "idle", label: "Ocioso" },
];
/** attention = pede algo a voce; busy = trabalhando, na fila ou esperando o gerente; done = ultimo trabalho deu certo. */
const statusOf = (c: Contact): Status => (c.state === "attention" ? "waiting" : c.state === "busy" ? "working" : c.done ? "done" : "idle");
const label = (s: Status) => LEGEND.find((l) => l.key === s)!.label;

type OverviewColumn = { key: string; title: string; manager: Contact | null; members: Contact[] };
const columns = computed(() => {
  const byId = new Map(contacts.items.map((c) => [c.dot.id, c]));
  const managers = new Set(teams.items.map((t) => t.manager_dot_id).filter((x): x is string => !!x));
  const placed = new Set<string>();
  const out: OverviewColumn[] = [];
  for (const t of teams.items) {
    const manager = t.manager_dot_id ? byId.get(t.manager_dot_id) ?? null : null;
    if (manager) placed.add(manager.dot.id);
    const members = t.member_ids.map((id) => byId.get(id)).filter((c): c is Contact => !!c && !managers.has(c.dot.id) && !placed.has(c.dot.id));
    members.forEach((c) => placed.add(c.dot.id));
    if (manager || members.length) out.push({ key: t.id, title: t.name, manager, members });
  }
  const rest = contacts.items.filter((c) => !placed.has(c.dot.id));
  if (rest.length) out.push({ key: "none", title: "Sem equipe", manager: null, members: rest });
  return out;
});
function agentsOf(col: OverviewColumn) {
  return col.manager ? [col.manager, ...col.members] : col.members;
}
function groupWorkedToday(col: OverviewColumn) {
  return loadedTimes.value && !timeError.value
    ? formatDuration(agentsOf(col).reduce((sum, c) => sum + (timeByDot.value.get(c.dot.id) ?? 0), 0))
    : "—";
}
function groupActionsToday(col: OverviewColumn) {
  return loadedActions.value && !actionsError.value
    ? agentsOf(col).reduce((sum, c) => sum + (actionCounts.value[c.dot.id] ?? 0), 0)
    : "—";
}
const open = (c: Contact) => router.push(`/chat/${c.dot.id}`);
</script>

<template>
  <div class="flex h-full flex-col overflow-hidden">
    <header class="flex shrink-0 items-center justify-between gap-4 px-6 pb-4 pt-5">
      <h1 class="text-lg font-semibold text-zinc-100">Visão Geral</h1>
      <div class="flex flex-wrap items-center gap-4 text-xs text-zinc-400">
        <span v-for="l in LEGEND" :key="l.key" class="flex items-center gap-1.5"><span class="legend" :class="l.key"><Check v-if="l.key === 'done'" :size="8" :stroke-width="4" /></span>{{ l.label }}</span>
      </div>
    </header>
    <div class="min-h-0 flex-1 overflow-auto px-6 pb-6">
      <p v-if="!columns.length" class="text-sm text-zinc-500">Nenhum DOT ainda.</p>
      <p v-else class="mb-2 text-xs text-zinc-500" title="Soma da duração das execuções no dia, inclusive períodos aguardando aprovação.">Tempo trabalhado hoje por DOT</p>
      <div v-if="columns.length" class="flex min-w-full w-max rounded-2xl border border-zinc-800 bg-zinc-900/60">
        <section v-for="col in columns" :key="col.key" class="flex w-60 flex-1 flex-col border-r border-zinc-800 last:border-r-0">
          <div class="flex h-[72px] items-center border-b border-zinc-800 px-5">
            <button v-if="col.manager" class="row" :title="`${col.manager.dot.name} · ${label(statusOf(col.manager))} · Hoje: ${workedToday(col.manager.dot.id)}`" @click="open(col.manager)">
              <span class="ball" :class="statusOf(col.manager)"><Check v-if="statusOf(col.manager) === 'done'" :size="14" :stroke-width="3" /></span>
              <span class="min-w-0 text-left">
                <span class="block truncate text-sm font-semibold text-zinc-100">{{ col.manager.dot.name }}</span>
                <span class="block truncate text-[10px] uppercase tracking-[0.16em] text-zinc-500">{{ col.title }}</span>
              </span>
              <span class="ml-auto shrink-0 text-[11px] tabular-nums text-zinc-400">{{ workedToday(col.manager.dot.id) }}</span>
            </button>
            <span v-else class="truncate text-[10px] uppercase tracking-[0.16em] text-zinc-500">{{ col.title }}</span>
          </div>
          <div class="flex flex-col gap-1 px-5 py-3">
            <button v-for="c in col.members" :key="c.dot.id" class="row py-2" :title="`${c.dot.name} · ${label(statusOf(c))} · Hoje: ${workedToday(c.dot.id)}`" @click="open(c)">
              <span class="ball" :class="statusOf(c)"><Check v-if="statusOf(c) === 'done'" :size="14" :stroke-width="3" /></span>
              <span class="truncate text-sm text-zinc-200">{{ c.dot.name }}</span>
              <span class="ml-auto shrink-0 text-[11px] tabular-nums text-zinc-400">{{ workedToday(c.dot.id) }}</span>
            </button>
          </div>
        </section>
      </div>
      <section v-if="columns.length" class="mt-5 min-w-[28rem] rounded-2xl border border-zinc-800 bg-zinc-900/60">
        <div class="border-b border-zinc-800 px-5 py-4">
          <h2 class="text-sm font-semibold text-zinc-100">Relatório de hoje</h2>
          <p class="mt-1 text-xs text-zinc-500">Ações são chamadas de ferramenta registradas. O tempo soma as execuções, inclusive esperas por aprovação.</p>
        </div>
        <div class="overflow-x-auto">
          <table class="w-full min-w-[28rem] text-sm">
            <thead class="text-[11px] uppercase tracking-wide text-zinc-500">
              <tr><th scope="col" class="px-5 py-2 text-left font-medium">Grupo / agente</th><th scope="col" class="w-20 py-2 text-right font-medium">Ações</th><th scope="col" class="w-32 px-5 py-2 text-right font-medium">Tempo</th></tr>
            </thead>
            <tbody v-for="col in columns" :key="col.key" class="border-t border-zinc-800">
              <tr class="bg-zinc-800/40 font-semibold text-zinc-100"><th scope="row" class="px-5 py-2.5 text-left">{{ col.title }}</th><td class="py-2.5 text-right tabular-nums">{{ groupActionsToday(col) }}</td><td class="px-5 py-2.5 text-right tabular-nums">{{ groupWorkedToday(col) }}</td></tr>
              <tr v-for="c in agentsOf(col)" :key="c.dot.id" class="text-zinc-300"><th scope="row" class="py-2 pl-9 pr-5 text-left font-normal">{{ c.dot.name }}</th><td class="py-2 text-right tabular-nums">{{ actionsToday(c.dot.id) }}</td><td class="px-5 py-2 text-right tabular-nums">{{ workedToday(c.dot.id) }}</td></tr>
            </tbody>
          </table>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.row { display: flex; width: 100%; min-width: 0; align-items: center; gap: 0.75rem; border-radius: 0.75rem; text-align: left; }
.row:hover .ball { filter: brightness(1.15); }
.ball { position: relative; display: inline-flex; height: 2rem; width: 2rem; flex-shrink: 0; align-items: center; justify-content: center; border-radius: 9999px; color: white; }
.ball.working { background: #8b5cf6; }
.ball.working::after { content: ""; position: absolute; inset: -4px; border-radius: 9999px; border: 3px solid rgb(139 92 246 / 0.25); border-top-color: rgb(139 92 246 / 0.8); animation: spin 1.1s linear infinite; }
.ball.done { background: #22c55e; }
.ball.waiting { background: #f97316; }
.ball.idle { background: #3f3f46; }
.legend { display: inline-flex; height: 0.75rem; width: 0.75rem; align-items: center; justify-content: center; border-radius: 9999px; color: white; }
.legend.working { background: #8b5cf6; box-shadow: 0 0 0 2px rgb(139 92 246 / 0.35); }
.legend.done { background: #22c55e; }
.legend.waiting { background: #f97316; }
.legend.idle { background: #3f3f46; }
@keyframes spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .ball.working::after { animation: none; } }
</style>
