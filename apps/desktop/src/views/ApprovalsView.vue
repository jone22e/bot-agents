<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { ChevronDown, ClipboardCheck, MessageSquare } from "lucide-vue-next";
import type { Approval } from "@dots/shared-types";
import Avatar from "@/components/Avatar.vue";
import ApprovalCard from "@/components/ApprovalCard.vue";
import PageHeader from "@/components/PageHeader.vue";
import RequestCard from "@/components/RequestCard.vue";
import { RISK_LABEL, describeApproval } from "@/lib/humanize";
import { useApprovalsStore } from "@/stores/approvals";

/**
 * Central de aprovacoes: uma fila unica com tudo que espera a sua decisao (perguntas e pedidos de permissao de
 * qualquer DOT), da mais antiga para a mais nova. Cada item traz quem pede, a justificativa (o que ele quer fazer
 * e por que), o impacto previsto (risco e recursos afetados), as evidencias (parametros exatos) e os botoes para
 * aprovar, rejeitar ou pedir revisao (dizer o que fazer diferente). O historico recente fica recolhido no fim.
 */
const router = useRouter();
const approvals = useApprovalsStore();
const showHistory = ref(false);
const queue = computed(() => [...approvals.pending].sort((a, b) => a.requested_at.localeCompare(b.requested_at)));
const history = computed(() => approvals.history.slice(0, 30));
onMounted(() => { void approvals.load(); });

const RISK_TONE: Record<string, string> = { read: "bg-zinc-700/60 text-zinc-200", write: "bg-amber-500/20 text-amber-200", exec: "bg-orange-500/20 text-orange-200", critical: "bg-red-500/20 text-red-200" };
/** Justificativa: o que o DOT quer, em uma frase. */
function reason(a: Approval) {
  return a.kind === "question" ? `${a.dot_name} pergunta: ${a.summary}` : `${a.dot_name} ${describeApproval(a)}`;
}
/** Impacto previsto: risco e o que e afetado. */
function impact(a: Approval) {
  if (a.kind === "question") return "Pergunta: nenhuma acao ate a sua resposta.";
  const risk = a.tool === "dots_browser_open" ? "acesso a internet" : RISK_LABEL[a.risk] ?? a.risk;
  return a.resources.length ? `${risk} · ${a.resources.join(", ")}` : risk;
}
function when(iso: string) {
  const d = new Date(iso);
  return d.toDateString() === new Date().toDateString() ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : d.toLocaleString([], { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });
}
</script>

<template>
  <div class="flex h-full flex-col overflow-hidden">
    <PageHeader title="Central de aprovações">
      <template #subtitle>Uma fila única de decisões pendentes, com justificativa, impacto previsto, evidências e botões para aprovar, rejeitar ou solicitar revisão.</template>
      <span v-if="queue.length" class="rounded-full bg-amber-500 px-2 py-0.5 text-xs font-semibold text-zinc-950">{{ queue.length }} pendente{{ queue.length === 1 ? "" : "s" }}</span>
    </PageHeader>
    <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-8">
      <p v-if="approvals.error" class="mb-3 rounded-md bg-red-950/40 px-3 py-2 text-xs text-red-200">{{ approvals.error }}</p>
      <div v-if="!queue.length" class="flex flex-col items-center gap-2 rounded-2xl border border-zinc-800 bg-zinc-900/40 px-6 py-12 text-center">
        <ClipboardCheck :size="28" class="text-emerald-400" />
        <p class="text-sm font-medium text-zinc-100">Nenhuma decisão pendente</p>
        <p class="text-xs text-zinc-500">Quando um DOT precisar da sua permissão ou fizer uma pergunta, o pedido aparece aqui.</p>
      </div>
      <ol v-else class="space-y-4">
        <li v-for="(a, i) in queue" :key="a.id" :data-approval="a.id" class="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/50">
          <div class="flex items-start gap-3 px-4 pt-4">
            <span class="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-zinc-800 text-[11px] font-semibold text-zinc-300">{{ i + 1 }}</span>
            <Avatar :name="a.dot_name" :seed="a.dot_id" size="sm" />
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
                <span class="text-sm font-semibold text-zinc-100">{{ a.dot_name }}</span>
                <span class="text-xs text-zinc-500">{{ a.kind === "question" ? "pergunta" : "pede permissão" }} · {{ when(a.requested_at) }} · expira {{ when(a.expires_at) }}</span>
                <span v-if="a.kind !== 'question'" class="rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide" :class="RISK_TONE[a.risk] ?? RISK_TONE.read">{{ a.tool === 'dots_browser_open' ? 'internet' : RISK_LABEL[a.risk] }}</span>
              </div>
              <dl class="mt-2 grid grid-cols-[7.5rem_1fr] gap-y-1 text-xs">
                <dt class="text-zinc-500">Justificativa</dt><dd class="text-zinc-200">{{ reason(a) }}</dd>
                <dt class="text-zinc-500">Impacto previsto</dt><dd class="text-zinc-200">{{ impact(a) }}</dd>
                <dt class="text-zinc-500">Evidências</dt>
                <dd class="min-w-0">
                  <details>
                    <summary class="cursor-pointer text-violet-300 hover:underline">Ferramenta {{ a.tool }} e parâmetros exatos</summary>
                    <pre class="mt-1 max-h-48 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-zinc-950 p-2 font-mono text-[11px] text-zinc-400">{{ [a.resources.join("\n"), JSON.stringify(a.input, null, 1)].filter(Boolean).join("\n") }}</pre>
                  </details>
                </dd>
              </dl>
            </div>
            <button type="button" class="flex shrink-0 items-center gap-1.5 rounded-full px-2.5 py-1 text-xs text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100" :title="`Abrir conversa com ${a.dot_name}`" @click="router.push(`/chat/${a.dot_id}`)"><MessageSquare :size="13" /> Conversa</button>
          </div>
          <div class="px-4 pb-4 pt-3">
            <RequestCard :approval="a" :busy="approvals.deciding !== null" @decide="(ok, ans) => approvals.decide(a.id, ok, ans)" />
          </div>
        </li>
      </ol>
      <section v-if="history.length" class="mt-8">
        <button type="button" class="flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-zinc-500 hover:text-zinc-300" :aria-expanded="showHistory" @click="showHistory = !showHistory"><ChevronDown :size="14" class="transition-transform" :class="showHistory ? 'rotate-180' : ''" /> Decididas recentemente ({{ history.length }})</button>
        <div v-if="showHistory" class="mt-3 space-y-2">
          <ApprovalCard v-for="a in history" :key="a.id" :approval="a" :busy="false" readonly />
        </div>
      </section>
    </div>
  </div>
</template>
