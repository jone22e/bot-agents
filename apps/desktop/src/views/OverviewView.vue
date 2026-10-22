<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import { Check } from "lucide-vue-next";
import type { Contact } from "@dots/shared-types";
import { api } from "@/api";
import Avatar from "@/components/Avatar.vue";
import { useDotMenu } from "@/composables/useDotMenu";
import { useDragReorder } from "@/composables/useDragReorder";
import { useContactsStore } from "@/stores/contacts";
import { useTeamsStore } from "@/stores/teams";

/**
 * Visao geral: uma coluna por equipe (gerente no topo, funcionarios embaixo) com a situacao de cada DOT.
 * Quem gerencia uma equipe aparece so na coluna dela, como na lista de contatos.
 */
const router = useRouter();
const contacts = useContactsStore();
const teams = useTeamsStore();
/** Botao direito sobre um DOT: acoes rapidas (pausar, retomar, duplicar, arquivar). */
const dotMenu = useDotMenu();
/** Arrastar o cabecalho de uma coluna para o lado reordena as equipes (mesma ordem da lista de contatos). */
const drag = useDragReorder("x", (from, to, before) => { void teams.reorder(from, to, before); });
const activity = ref<{ dot_id: string; started_at: string; finished_at: string }[]>([]);
const now = ref(Date.now());
const loadedTimes = ref(false);
const timeError = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;

function dayBounds(date: Date) {
  const y = date.getFullYear();
  const m = date.getMonth();
  const d = date.getDate();
  return { start: new Date(y, m, d), end: new Date(y, m, d + 1) };
}

/** Tempo ativo observado hoje por DOT (mostrado ao lado de cada nome). */
async function loadReport() {
  now.value = Date.now();
  const { start, end } = dayBounds(new Date(now.value));
  try {
    activity.value = await api.executions.observedActivityInPeriod(start.toISOString(), end.toISOString());
    loadedTimes.value = true;
    timeError.value = false;
  } catch {
    timeError.value = true;
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
  for (const t of teams.ordered) {
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
        <section v-for="col in columns" :key="col.key" class="team-column flex w-60 flex-1 flex-col border-r border-zinc-800 last:border-r-0" :class="drag.classOf(col.key)" :data-drag-id="col.key !== 'none' ? col.key : undefined">
          <div class="flex h-[72px] items-center border-b border-zinc-800 px-5" :class="col.key !== 'none' ? 'cursor-grab' : ''" :title="col.key !== 'none' ? 'Arraste para reordenar as equipes' : undefined" @pointerdown="col.key !== 'none' && drag.start($event, col.key)">
            <button v-if="col.manager" class="row" :title="`${col.manager.dot.name} · ${label(statusOf(col.manager))} · Hoje: ${workedToday(col.manager.dot.id)}`" @click="open(col.manager)" @contextmenu="dotMenu.open($event, col.manager.dot)">
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
            <button v-for="c in col.members" :key="c.dot.id" class="row py-2" :title="`${c.dot.name} · ${label(statusOf(c))} · Hoje: ${workedToday(c.dot.id)}`" @click="open(c)" @contextmenu="dotMenu.open($event, c.dot)">
              <span class="ball" :class="statusOf(c)"><Avatar :name="c.dot.name" :seed="c.dot.id" size="sm" /><span v-if="statusOf(c) === 'done'" class="status-marker"><Check :size="9" :stroke-width="3" /></span></span>
              <span class="truncate text-sm text-zinc-200">{{ c.dot.name }}</span>
              <span v-if="c.dot.status === 'paused'" class="shrink-0 text-[10px] font-medium text-amber-300">Pausado</span>
              <span class="ml-auto shrink-0 text-[11px] tabular-nums text-zinc-400">{{ workedToday(c.dot.id) }}</span>
            </button>
          </div>
        </section>
      </div>
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
.team-column.drag-source { opacity: 0.45; }
.team-column.drop-before { box-shadow: inset 3px 0 0 #8b5cf6; }
.team-column.drop-after { box-shadow: inset -3px 0 0 #8b5cf6; }
@keyframes spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .ball.working::after { animation: none; } }
</style>
