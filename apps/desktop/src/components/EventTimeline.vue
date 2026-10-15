<script setup lang="ts">
import { computed } from "vue";
import type { ExecutionEventRecord } from "@dots/shared-types";
import { describeEvent } from "@/lib/humanize";

const props = defineProps<{ events: ExecutionEventRecord[]; liveText?: string }>();
const lines = computed(() => props.events.map((e) => ({ id: e.id, at: e.created_at, d: describeEvent(e.event) })).filter((x) => x.d !== null));
const toneCls: Record<string, string> = { normal: "text-zinc-300", ok: "text-emerald-300", warn: "text-amber-300", error: "text-red-300" };
</script>

<template>
  <div>
    <ol class="space-y-1 text-sm">
      <li v-for="l in lines" :key="l.id" class="flex gap-3">
        <span class="w-16 shrink-0 text-xs text-zinc-600">{{ new Date(l.at).toLocaleTimeString() }}</span>
        <span :class="toneCls[l.d!.tone]">{{ l.d!.text }}</span>
      </li>
      <li v-if="!lines.length && !liveText" class="text-xs text-zinc-500">Nada aconteceu ainda.</li>
    </ol>
    <pre v-if="liveText" class="mt-3 whitespace-pre-wrap break-words rounded-md border border-indigo-900/50 bg-indigo-950/20 p-2 font-sans text-sm text-zinc-200">{{ liveText }}</pre>
    <details v-if="events.length" class="mt-3 text-xs">
      <summary class="cursor-pointer text-zinc-500 hover:text-zinc-300">Detalhes tecnicos ({{ events.length }} eventos)</summary>
      <ol class="mt-2 space-y-1">
        <li v-for="ev in events" :key="ev.id" class="rounded border border-zinc-800 p-1.5 font-mono text-[11px] text-zinc-400">
          <span class="text-zinc-500">#{{ ev.seq }} {{ ev.event.type }}</span>
          <pre class="whitespace-pre-wrap break-words">{{ JSON.stringify(ev.event).slice(0, 600) }}</pre>
        </li>
      </ol>
    </details>
  </div>
</template>
