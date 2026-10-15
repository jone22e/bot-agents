<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { ENGINE_LABEL } from "@/lib/humanize";
import { useOpsStore } from "@/stores/ops";
import { useRuntimeStore } from "@/stores/runtime";

const ops = useOpsStore();
const runtime = useRuntimeStore();
const showLogs = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;
onMounted(async () => {
  if (runtime.isConnected) await ops.load();
  timer = setInterval(() => { if (runtime.isConnected) ops.load(); }, 5000);
});
onUnmounted(() => { if (timer) clearInterval(timer); });
function mb(b: number) { return (b / 1048576).toFixed(0); }
function mins(s: number) { return s < 60 ? `${s}s` : `${Math.floor(s / 60)}min`; }
</script>

<template>
  <div class="card space-y-3">
    <div class="flex items-center justify-between">
      <h3 class="text-sm font-semibold text-zinc-200">Operacoes agora</h3>
      <span v-if="ops.stats?.paused" class="badge bg-amber-500/20 text-amber-200">fila pausada</span>
    </div>
    <div v-if="ops.stats" class="grid grid-cols-4 gap-2 text-xs">
      <div><p class="text-zinc-500">Trabalhando</p><p class="text-lg text-zinc-100">{{ ops.stats.running }}</p></div>
      <div><p class="text-zinc-500">Na fila</p><p class="text-lg text-zinc-100">{{ ops.stats.queued }}</p></div>
      <div><p class="text-zinc-500">Esperando voce</p><p class="text-lg" :class="ops.stats.blocked + ops.stats.questions_pending + ops.stats.approvals_pending > 0 ? 'text-amber-300' : 'text-zinc-100'">{{ ops.stats.blocked }}</p></div>
      <div><p class="text-zinc-500">Nao deram certo (24 h)</p><p class="text-lg text-zinc-100">{{ ops.stats.failures_24h }}</p></div>
    </div>
    <p v-if="ops.stats?.last_error" class="text-xs text-red-300">Ultimo erro: {{ ops.stats.last_error }}</p>
    <div v-if="ops.stats?.processes.length" class="text-xs">
      <p class="mb-1 text-zinc-500">Assistentes em execucao</p>
      <ul class="space-y-0.5"><li v-for="p in ops.stats.processes" :key="p.execution_id" class="text-zinc-300">ha {{ mins(p.elapsed_seconds) }} · CPU {{ p.cpu_percent.toFixed(0) }}% · {{ mb(p.rss_bytes) }} MB</li></ul>
    </div>
    <div v-if="ops.stats?.engine_usage.length" class="text-xs">
      <p class="mb-1 text-zinc-500">Consumo nas ultimas 24 h</p>
      <table class="w-full"><tbody>
        <tr v-for="u in ops.stats.engine_usage" :key="u.engine" class="text-zinc-300">
          <td>{{ ENGINE_LABEL[u.engine as 'codex' | 'claude' | 'agy'] ?? u.engine }}</td><td>{{ u.executions }} execucoes</td><td>{{ u.input_tokens + u.output_tokens }} tokens</td><td>{{ u.cost_usd > 0 ? `US$ ${u.cost_usd.toFixed(2)}` : "custo nao informado" }}</td><td>{{ mins(u.total_seconds) }}</td>
        </tr>
      </tbody></table>
    </div>
    <details class="text-xs" @toggle="showLogs = ($event.target as HTMLDetailsElement).open; showLogs && ops.loadLogs()">
      <summary class="cursor-pointer text-zinc-500 hover:text-zinc-300">Registro do servico (ultimas linhas)</summary>
      <pre class="mt-2 max-h-64 overflow-auto rounded bg-zinc-950 p-2 font-mono text-[10px] text-zinc-400">{{ ops.logs.join("\n") || "(vazio)" }}</pre>
    </details>
  </div>
</template>
