<script setup lang="ts">
import type { Approval } from "@dots/shared-types";
import { describeApproval, RISK_LABEL } from "@/lib/humanize";

defineProps<{ approval: Approval; busy: boolean; readonly?: boolean }>();
const emit = defineEmits<{ approve: []; reject: []; answer: [text: string] }>();
import { ref } from "vue";
const answer = ref("");
const statusLabel: Record<string, string> = { pending: "aguardando", approved: "permitido", rejected: "negado", expired: "expirou" };
</script>

<template>
  <div class="card">
    <div class="flex items-start justify-between gap-3">
      <p v-if="approval.kind === 'question'" class="text-sm text-zinc-100"><strong>{{ approval.dot_name }}</strong> pergunta: {{ approval.summary }}</p>
      <p v-else class="text-sm text-zinc-100"><strong>{{ approval.dot_name }}</strong> {{ describeApproval(approval) }} <span class="text-zinc-500">· {{ approval.tool === 'dots_browser_open' ? 'acesso à internet' : RISK_LABEL[approval.risk] }}</span></p>
      <span class="badge" :class="approval.status === 'pending' ? 'bg-amber-500/20 text-amber-200' : approval.status === 'approved' ? 'bg-emerald-500/15 text-emerald-300' : 'bg-zinc-700/50 text-zinc-300'">{{ statusLabel[approval.status] }}</span>
    </div>
    <p class="mt-1 text-xs text-zinc-500">{{ new Date(approval.requested_at).toLocaleString() }}<template v-if="approval.status === 'pending'"> · expira {{ new Date(approval.expires_at).toLocaleTimeString() }}</template><template v-if="approval.reason"> · {{ approval.reason }}</template></p>
    <details class="mt-2 text-xs">
      <summary class="cursor-pointer text-zinc-500 hover:text-zinc-300">Detalhes tecnicos</summary>
      <dl class="mt-2 grid grid-cols-[100px_1fr] gap-y-1">
        <dt class="text-zinc-500">Ferramenta</dt><dd class="font-mono text-zinc-300">{{ approval.tool }}</dd>
        <dt class="text-zinc-500">Recursos</dt><dd class="font-mono text-zinc-300"><div v-for="r in approval.resources" :key="r" class="truncate">{{ r }}</div></dd>
        <dt class="text-zinc-500">Parametros</dt><dd><pre class="max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] text-zinc-400">{{ JSON.stringify(approval.input, null, 1) }}</pre></dd>
      </dl>
    </details>
    <div v-if="!readonly && approval.status === 'pending' && approval.kind === 'question'" class="mt-3 space-y-2">
      <div v-if="approval.options.length" class="flex flex-wrap gap-2">
        <button v-for="o in approval.options" :key="o" class="btn-secondary" :disabled="busy" @click="emit('answer', o)">{{ o }}</button>
      </div>
      <div class="flex gap-2">
        <input v-model="answer" class="input" placeholder="Ou responda com suas palavras" @keyup.enter="answer.trim() && emit('answer', answer.trim())" />
        <button class="btn-primary" :disabled="busy || !answer.trim()" @click="emit('answer', answer.trim())">Responder</button>
      </div>
    </div>
    <div v-else-if="!readonly && approval.status === 'pending'" class="mt-3 flex justify-end gap-2">
      <button class="btn-secondary" :disabled="busy" @click="emit('reject')">Negar</button>
      <button class="btn-primary" :disabled="busy" @click="emit('approve')">Permitir</button>
    </div>
    <p v-else-if="approval.kind === 'question' && approval.reason" class="mt-2 text-xs text-emerald-300">Resposta: {{ approval.reason }}</p>
  </div>
</template>
