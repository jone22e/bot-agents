<script setup lang="ts">
import { ref } from "vue";
import type { Approval } from "@dots/shared-types";
import { describeApproval, RISK_LABEL } from "@/lib/humanize";

defineProps<{ approval: Approval; busy: boolean }>();
const emit = defineEmits<{ decide: [approved: boolean, answer?: string] }>();
const answer = ref("");
</script>

<template>
  <div class="flex justify-start">
    <div class="max-w-[72%] rounded-2xl border border-amber-700/50 bg-amber-950/30 px-4 py-3 text-sm text-amber-50">
      <template v-if="approval.kind === 'question'">
        <p>{{ approval.summary }}</p>
        <div v-if="approval.options.length" class="mt-2 flex flex-wrap gap-2">
          <button v-for="o in approval.options" :key="o" class="rounded-full bg-amber-500/20 px-3 py-1 text-xs text-amber-100 hover:bg-amber-500/40" :disabled="busy" @click="emit('decide', true, o)">{{ o }}</button>
        </div>
        <div class="mt-2 flex gap-2">
          <input v-model="answer" class="h-8 flex-1 rounded-full bg-zinc-900 px-3 text-xs text-zinc-100 focus:outline-none" placeholder="Responder com suas palavras" @keyup.enter="answer.trim() && emit('decide', true, answer.trim())" />
          <button class="rounded-full bg-amber-500 px-3 text-xs font-medium text-zinc-950 disabled:opacity-50" :disabled="busy || !answer.trim()" @click="emit('decide', true, answer.trim())">Enviar</button>
        </div>
      </template>
      <template v-else>
        <p>Posso {{ describeApproval(approval).replace(/^quer /, '') }}? <span class="text-amber-300/80">({{ RISK_LABEL[approval.risk] }})</span></p>
        <details class="mt-1 text-[11px] text-amber-200/70"><summary class="cursor-pointer list-none hover:text-amber-100">detalhes</summary><pre class="mt-1 whitespace-pre-wrap break-words font-mono">{{ approval.resources.join("\n") }}\n{{ JSON.stringify(approval.input, null, 1) }}</pre></details>
        <div class="mt-2 flex gap-2">
          <button class="rounded-full bg-zinc-800 px-3 py-1 text-xs text-zinc-100 hover:bg-zinc-700" :disabled="busy" @click="emit('decide', false)">Negar</button>
          <button class="rounded-full bg-amber-500 px-3 py-1 text-xs font-medium text-zinc-950 hover:bg-amber-400" :disabled="busy" @click="emit('decide', true)">Permitir</button>
        </div>
      </template>
    </div>
  </div>
</template>
