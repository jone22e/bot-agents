<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import { Check, ChevronDown } from "lucide-vue-next";
import type { Contact } from "@dots/shared-types";
import { api } from "@/api";
import Avatar from "@/components/Avatar.vue";
import { useContactsStore } from "@/stores/contacts";
import { useTeamsStore } from "@/stores/teams";

/**
 * Visao geral: uma coluna por equipe (gerente no topo, funcionarios embaixo) com a situacao de cada DOT.
 * Quem gerencia uma equipe aparece so na coluna dela, como na lista de contatos.
 */
const router = useRouter();
const contacts = useContactsStore();
const teams = useTeamsStore();
const activity = ref<{ dot_id: string; started_at: string; finished_at: string }[]>([]);
const now = ref(Date.now());
const loadedTimes = ref(false);
const timeError = ref(false);
const actionCounts = ref<Record<string, number>>({});
const loadedActions = ref(false);
const actionsError = ref(false);
const reportExpanded = ref(false);
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
    api.executions.observedActivityInPeriod(start.toISOString(), end.toISOString()),
    api.executions.toolCallsInPeriod(start.toISOString(), end.toISOString()),
  ]);
  if (times.status === "fulfilled") {
    activity.value = times.value;
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

type TimeInterval = [number, number];
function elapsedWithoutOverlap(intervals: TimeInterval[]) {
  const ordered = [...intervals].sort((a, b) => a[0] - b[0]);
  let total = 0;
  let end = 0;
  for (const [from, to] of ordered) {
    if (from >= end) total += to - from;
    else if (to > end) total += to - end;
    end = Math.max(end, to);
  }
  return total;
}
const intervalsByDot = computed(() => {
  const { start, end } = dayBounds(new Date(now.value));
  const intervals = new Map<string, TimeInterval[]>();
  for (const interval of activity.value) {
    const from = Math.max(Date.parse(interval.started_at), start.getTime());
    const to = Math.min(Date.parse(interval.finished_at), end.getTime(), now.value);
    if (Number.isFinite(from) && Number.isFinite(to) && to > from) {
      const mine = intervals.get(interval.dot_id) ?? [];
      mine.push([from, to]);
      intervals.set(interval.dot_id, mine);
    }
  }
  return intervals;
});
const timeByDot = computed(() => new Map(
  [...intervalsByDot.value].map(([dotId, intervals]) => [dotId, elapsedWithoutOverlap(intervals)]),
));
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
function formatActions(value: number | string) {
  return typeof value === "number" ? value.toLocaleString("pt-BR") : value;
}

type Status = "working" | "done" | "waiting" | "paused" | "idle";
const LEGEND: { key: Status; label: string }[] = [
  { key: "working", label: "Trabalhando" },
  { key: "done", label: "Concluído" },
  { key: "waiting", label: "Aguardando você" },
  { key: "paused", label: "Pausado" },
  { key: "idle", label: "Ocioso" },
];
/** attention = pede algo a voce; busy = trabalhando, na fila ou esperando o gerente; done = ultimo trabalho deu certo. */
const statusOf = (c: Contact): Status => (c.dot.status === "paused" ? "paused" : c.state === "attention" ? "waiting" : c.state === "busy" ? "working" : c.done ? "done" : "idle");
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
    ? formatDuration(elapsedWithoutOverlap(agentsOf(col).flatMap((c) => intervalsByDot.value.get(c.dot.id) ?? [])))
    : "—";
}
function groupActionsToday(col: OverviewColumn) {
  return loadedActions.value && !actionsError.value
    ? agentsOf(col).reduce((sum, c) => sum + (actionCounts.value[c.dot.id] ?? 0), 0)
    : "—";
}
const reportAgents = computed(() => {
  const unique = new Map<string, Contact>();
  for (const col of columns.value) for (const agent of agentsOf(col)) unique.set(agent.dot.id, agent);
  return [...unique.values()];
});
const totalActionsToday = computed(() => loadedActions.value && !actionsError.value
  ? reportAgents.value.reduce((sum, agent) => sum + (actionCounts.value[agent.dot.id] ?? 0), 0)
  : "—");
const totalWorkedToday = computed(() => loadedTimes.value && !timeError.value
  ? formatDuration(elapsedWithoutOverlap(reportAgents.value.flatMap((agent) => intervalsByDot.value.get(agent.dot.id) ?? [])))
  : "—");
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
      <p v-else class="mb-2 text-xs text-zinc-500" title="Tempo entre chamada e retorno de ferramentas, descontadas esperas por aprovação. Raciocínio sem eventos e ferramentas ainda em andamento não entram.">Tempo ativo observado hoje por DOT</p>
      <div v-if="columns.length" class="flex min-w-full w-max rounded-2xl border border-zinc-800 bg-zinc-900/60">
        <section v-for="col in columns" :key="col.key" class="flex w-60 flex-1 flex-col border-r border-zinc-800 last:border-r-0">
          <div class="flex h-[72px] items-center border-b border-zinc-800 px-5">
            <button v-if="col.manager" class="row" :title="`${col.manager.dot.name} · ${label(statusOf(col.manager))} · Hoje: ${workedToday(col.manager.dot.id)}`" @click="open(col.manager)">
              <span class="ball" :class="statusOf(col.manager)"><Avatar :name="col.manager.dot.name" :seed="col.manager.dot.id" size="sm" /><span v-if="statusOf(col.manager) === 'done'" class="status-marker"><Check :size="9" :stroke-width="3" /></span></span>
              <span class="min-w-0 text-left">
                <span class="block truncate text-sm font-semibold text-zinc-100">{{ col.manager.dot.name }}</span>
                <span class="block truncate text-[10px] uppercase tracking-[0.16em] text-zinc-500">{{ col.title }}</span>
              </span>
              <span v-if="col.manager.dot.status === 'paused'" class="shrink-0 text-[10px] font-medium text-amber-300">Pausado</span>
              <span class="ml-auto shrink-0 text-[11px] tabular-nums text-zinc-400">{{ workedToday(col.manager.dot.id) }}</span>
            </button>
            <span v-else class="truncate text-[10px] uppercase tracking-[0.16em] text-zinc-500">{{ col.title }}</span>
          </div>
          <div class="flex flex-col gap-1 px-5 py-3">
            <button v-for="c in col.members" :key="c.dot.id" class="row py-2" :title="`${c.dot.name} · ${label(statusOf(c))} · Hoje: ${workedToday(c.dot.id)}`" @click="open(c)">
              <span class="ball" :class="statusOf(c)"><Avatar :name="c.dot.name" :seed="c.dot.id" size="sm" /><span v-if="statusOf(c) === 'done'" class="status-marker"><Check :size="9" :stroke-width="3" /></span></span>
              <span class="truncate text-sm text-zinc-200">{{ c.dot.name }}</span>
              <span v-if="c.dot.status === 'paused'" class="shrink-0 text-[10px] font-medium text-amber-300">Pausado</span>
              <span class="ml-auto shrink-0 text-[11px] tabular-nums text-zinc-400">{{ workedToday(c.dot.id) }}</span>
            </button>
          </div>
        </section>
      </div>
      <section v-if="columns.length" class="mt-5 overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/60">
        <div class="flex flex-wrap items-center justify-between gap-4 px-5 py-4">
          <div class="min-w-0">
            <p class="text-[10px] font-semibold uppercase tracking-[0.18em] text-violet-400">Atividade · hoje</p>
            <h2 class="mt-1 text-base font-semibold text-zinc-100">Relatório de hoje</h2>
          </div>
          <div class="flex flex-wrap items-center gap-3 sm:gap-5">
            <div class="text-right"><span class="block text-[10px] uppercase tracking-wider text-zinc-500">Ações</span><strong class="text-sm font-semibold tabular-nums text-zinc-100">{{ formatActions(totalActionsToday) }}</strong></div>
            <div class="text-right"><span class="block text-[10px] uppercase tracking-wider text-zinc-500">Tempo observado</span><strong class="text-sm font-semibold tabular-nums text-zinc-100">{{ totalWorkedToday }}</strong></div>
            <button type="button" class="flex h-9 items-center gap-1.5 rounded-lg border border-zinc-700 px-3 text-xs font-medium text-zinc-300 transition-colors hover:border-zinc-500 hover:bg-zinc-800 hover:text-zinc-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-violet-400" :aria-expanded="reportExpanded" aria-controls="today-report-details" @click="reportExpanded = !reportExpanded">
              <span>{{ reportExpanded ? "Recolher" : "Expandir" }}</span><ChevronDown :size="15" class="transition-transform" :class="reportExpanded ? 'rotate-180' : ''" />
            </button>
          </div>
        </div>
        <div v-if="reportExpanded" id="today-report-details" class="border-t border-zinc-800 px-5 py-5">
          <div class="flex flex-col gap-4">
            <section v-for="col in columns" :key="col.key" :data-report-group="col.key" class="overflow-hidden rounded-xl border border-zinc-800 bg-zinc-950/35">
              <header class="flex items-center justify-between gap-4 border-b border-zinc-800 px-4 py-4">
                <div class="min-w-0"><h3 class="truncate text-sm font-semibold text-zinc-100">{{ col.title }}</h3><p class="mt-0.5 text-[11px] text-zinc-500">{{ agentsOf(col).length }} {{ agentsOf(col).length === 1 ? "agente" : "agentes" }}</p></div>
                <div class="flex shrink-0 items-center gap-4 sm:gap-6">
                  <div class="text-right"><span class="block text-[10px] uppercase tracking-wide text-zinc-500">Ações</span><strong class="text-sm font-semibold tabular-nums text-zinc-200">{{ formatActions(groupActionsToday(col)) }}</strong></div>
                  <div class="text-right"><span class="block text-[10px] uppercase tracking-wide text-zinc-500">Tempo</span><strong class="text-sm font-semibold tabular-nums text-violet-300">{{ groupWorkedToday(col) }}</strong></div>
                </div>
              </header>
              <div class="grid grid-cols-[minmax(0,1fr)_4.5rem_6.5rem] gap-2 px-4 pb-1 pt-3 text-[10px] uppercase tracking-wider text-zinc-500"><span>Agente</span><span class="text-right">Ações</span><span class="text-right">Tempo</span></div>
              <div v-for="c in agentsOf(col)" :key="c.dot.id" :data-report-agent="c.dot.id" class="grid grid-cols-[minmax(0,1fr)_4.5rem_6.5rem] items-center gap-2 px-4 py-2.5 text-sm text-zinc-300 odd:bg-zinc-800/20">
                <span class="flex min-w-0 items-center gap-2.5"><Avatar :name="c.dot.name" :seed="c.dot.id" size="sm" /><span class="truncate">{{ c.dot.name }}</span></span>
                <span class="text-right tabular-nums text-zinc-400">{{ formatActions(actionsToday(c.dot.id)) }}</span>
                <span class="text-right font-medium tabular-nums text-zinc-200">{{ workedToday(c.dot.id) }}</span>
              </div>
            </section>
          </div>
          <p class="mt-4 text-[11px] leading-relaxed text-zinc-500">Ações: chamadas de ferramenta registradas. Tempo: chamadas com retorno, sem esperas por aprovação. Períodos simultâneos contam uma vez; raciocínio sem eventos fica fora.</p>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.row { display: flex; width: 100%; min-width: 0; align-items: center; gap: 0.75rem; border-radius: 0.75rem; text-align: left; }
.row:hover .ball { filter: brightness(1.15); }
.ball { position: relative; display: inline-flex; height: 2.25rem; width: 2.25rem; flex-shrink: 0; align-items: center; justify-content: center; border: 2px solid #52525b; border-radius: 9999px; background: #27272a; }
.ball.working { border-color: #8b5cf6; }
.ball.working::after { content: ""; position: absolute; inset: -4px; border-radius: 9999px; border: 3px solid rgb(139 92 246 / 0.25); border-top-color: rgb(139 92 246 / 0.8); animation: spin 1.1s linear infinite; }
.ball.done { border-color: #22c55e; }
.ball.waiting { border-color: #f97316; }
.ball.paused { border-color: #fbbf24; }
.status-marker { position: absolute; right: -4px; bottom: -4px; display: flex; height: 14px; width: 14px; align-items: center; justify-content: center; border: 2px solid #202027; border-radius: 9999px; background: #22c55e; color: white; }
.legend { display: inline-flex; height: 0.75rem; width: 0.75rem; align-items: center; justify-content: center; border-radius: 9999px; color: white; }
.legend.working { background: #8b5cf6; box-shadow: 0 0 0 2px rgb(139 92 246 / 0.35); }
.legend.done { background: #22c55e; }
.legend.waiting { background: #f97316; }
.legend.paused { background: #fbbf24; }
.legend.idle { background: #3f3f46; }
@keyframes spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .ball.working::after { animation: none; } }
</style>
