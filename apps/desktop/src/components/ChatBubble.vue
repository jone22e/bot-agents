<script setup lang="ts">
import { computed } from "vue";
import type { Message } from "@dots/shared-types";
import { describeFailure } from "@/lib/humanize";

const props = defineProps<{ message: Message }>();
const meta = computed(() => (props.message.metadata ?? {}) as { tools?: string[]; error?: string | null; streaming?: boolean; origin?: string; prompt?: string; status?: string });
const tools = computed(() => (meta.value.tools ?? []).map((t) => t.replace(/^mcp__dots__/, "").replace(/^dots\//, "").replace(/^dots_/, "")));
const time = computed(() => new Date(props.message.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }));
</script>

<template>
  <div v-if="message.role === 'system'" class="my-3 flex justify-center">
    <details class="max-w-[80%] rounded-full bg-zinc-900/80 px-3 py-1 text-center text-[11px] text-zinc-400">
      <summary class="cursor-pointer list-none">{{ message.content }}</summary>
      <p v-if="meta.prompt" class="mt-1 whitespace-pre-wrap text-left text-zinc-500">{{ meta.prompt }}</p>
    </details>
  </div>
  <div v-else class="flex" :class="message.role === 'user' ? 'justify-end' : 'justify-start'">
    <div class="max-w-[72%] rounded-2xl px-4 py-2.5 text-[15px] leading-relaxed" :class="message.role === 'user' ? 'bg-zinc-700/90 text-zinc-50' : 'bg-zinc-800/90 text-zinc-100'">
      <pre class="whitespace-pre-wrap break-words font-sans">{{ message.content || (meta.streaming ? "…" : "") }}</pre>
      <p v-if="meta.error" class="mt-1.5 text-xs text-red-300">{{ describeFailure(meta.status === 'TIMEOUT' ? 'timeout' : meta.status === 'CANCELLED' ? 'cancelled' : 'engine', meta.error) }}</p>
      <details v-if="tools.length" class="mt-1.5 text-[11px] text-zinc-400">
        <summary class="cursor-pointer list-none hover:text-zinc-300">{{ tools.length }} acao(oes) · {{ time }}</summary>
        <ul class="mt-1 space-y-0.5"><li v-for="(t, i) in tools" :key="i">· {{ t }}</li></ul>
      </details>
      <p v-else class="mt-1 text-right text-[10px] text-zinc-500">{{ time }}</p>
    </div>
  </div>
</template>
