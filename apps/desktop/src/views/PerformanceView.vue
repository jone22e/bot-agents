<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import Avatar from "@/components/Avatar.vue";
import PageHeader from "@/components/PageHeader.vue";
import { api, RuntimeError } from "@/api";
import { computePerformance, duration, pct, totalPerformance, usd, type DotPerformance } from "@/lib/performance";
import { useDotsStore } from "@/stores/dots";

/**
 * Avaliacao de desempenho: taxa de sucesso, reincidencia, custo por operacao, tempo de resolucao, acoes revertidas
 * e tarefas que exigiram intervencao humana, por DOT e no total, nos ultimos 7 ou 30 dias (ver `lib/performance.ts`).
 */
const router = useRouter();
const dots = useDotsStore();
const days = ref<7 | 30>(7);
const loading = ref(false);
const error = ref<string | null>(null);
const rows = ref<DotPerformance[]>([]);
const total = computed(() => totalPerformance(rows.value));

async function load() {
  loading.value = true;
  const end = new Date();
  const start = new Date(end.getTime() - days.value * 86400000);
  try {
    if (!dots.items.length) await dots.load();
    const [tasks, executions, approvals, audit] = await Promise.all([
      api.tasks.list({ limit: 2000 }),
      api.executions.inPeriod(start.toISOString(), end.toISOString()),
      api.approvals.list(false, 200),
      api.audit.list(2000),
    ]);
    rows.value = computePerformance({ tasks, executions, approvals, audit, start, end, dotNames: new Map(dots.items.map((d) => [d.id, d.name])) });
    error.value = null;
  } catch (e) {
    error.value = e instanceof RuntimeError ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}
onMounted(load);
watch(days, load);

const METRICS: { key: string; label: string; hint: string; value: (r: DotPerformance) => string }[] = [
  { key: "success", label: "Taxa de sucesso", hint: "Tarefas concluídas entre as encerradas no período", value: (r) => pct(r.completed, r.finished) },
  { key: "recurrence", label: "Reincidência", hint: "Tarefas que precisaram de nova tentativa ou repetiram uma falha anterior", value: (r) => pct(r.recurrent, r.finished) },
  { key: "cost", label: "Custo por operação", hint: "Custo das execuções dividido pelas tarefas encerradas", value: (r) => usd(r.costUsd, r.finished) },
  { key: "time", label: "Tempo de resolução", hint: "Mediana entre o pedido e a conclusão", value: (r) => duration(r.resolutionMs) },
  { key: "reverted", label: "Ações revertidas", hint: "Ações negadas pela política, rejeitadas por você ou desfeitas via git", value: (r) => String(r.reverted) },
  { key: "human", label: "Intervenção humana", hint: "Tarefas em que o DOT precisou da sua permissão ou resposta", value: (r) => `${r.humanTasks}${r.finished ? ` (${pct(r.humanTasks, r.finished)})` : ""}` },
];
</script>

<template>
  <div class="flex h-full flex-col overflow-hidden">
    <PageHeader title="Avaliação de desempenho">
      <template #subtitle>Taxa de sucesso, reincidência, custo por operação, tempo de resolução, ações revertidas e tarefas que exigiram intervenção humana.</template>
      <div class="flex items-center gap-1 rounded-full border border-zinc-800 p-0.5 text-xs">
        <button v-for="d in [7, 30] as const" :key="d" type="button" class="rounded-full px-3 py-1" :class="days === d ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-400 hover:text-zinc-200'" :data-days="d" @click="days = d">{{ d }} dias</button>
      </div>
    </PageHeader>
    <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-8">
      <p v-if="error" class="mb-3 rounded-md bg-red-950/40 px-3 py-2 text-xs text-red-200">{{ error }}</p>
      <div class="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6" data-testid="totals">
        <div v-for="m in METRICS" :key="m.key" class="rounded-2xl border border-zinc-800 bg-zinc-900/50 px-4 py-3" :title="m.hint">
          <p class="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">{{ m.label }}</p>
          <p class="mt-1 text-xl font-semibold tabular-nums text-zinc-100">{{ loading && !rows.length ? "…" : m.value(total) }}</p>
        </div>
      </div>
      <p class="mt-2 text-[11px] text-zinc-500">{{ total.finished }} tarefa{{ total.finished === 1 ? "" : "s" }} encerrada{{ total.finished === 1 ? "" : "s" }} e {{ total.executions }} execuç{{ total.executions === 1 ? "ão" : "ões" }} nos últimos {{ days }} dias. Custo soma só o que os assistentes informam.</p>
      <div v-if="!loading && !rows.length" class="mt-6 rounded-2xl border border-zinc-800 bg-zinc-900/40 px-6 py-10 text-center text-sm text-zinc-400">Nenhuma tarefa encerrada no período.</div>
      <div v-else-if="rows.length" class="mt-6 overflow-hidden rounded-2xl border border-zinc-800">
        <table class="w-full text-sm">
          <thead class="bg-zinc-900/70 text-[10px] uppercase tracking-wider text-zinc-500">
            <tr><th class="px-4 py-2.5 text-left font-semibold">Agente</th><th class="px-3 py-2.5 text-right font-semibold">Tarefas</th><th v-for="m in METRICS" :key="m.key" class="px-3 py-2.5 text-right font-semibold" :title="m.hint">{{ m.label }}</th></tr>
          </thead>
          <tbody>
            <tr v-for="r in rows" :key="r.dotId" :data-dot="r.dotId" class="cursor-pointer border-t border-zinc-800 text-zinc-300 hover:bg-zinc-900/60" @click="router.push(`/chat/${r.dotId}`)">
              <td class="px-4 py-2.5"><span class="flex items-center gap-2.5"><Avatar :name="r.dotName" :seed="r.dotId" size="sm" /><span class="truncate font-medium text-zinc-100">{{ r.dotName }}</span></span></td>
              <td class="px-3 py-2.5 text-right tabular-nums">{{ r.finished }}</td>
              <td v-for="m in METRICS" :key="m.key" class="px-3 py-2.5 text-right tabular-nums">{{ m.value(r) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
