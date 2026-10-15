<script setup lang="ts">
import { onMounted, onUnmounted, watch } from "vue";
import { Globe, X } from "lucide-vue-next";
import type { Dot } from "@dots/shared-types";
import { useBrowserStore } from "@/stores/browser";

const props = defineProps<{ dot: Dot }>();
const emit = defineEmits<{ close: [] }>();
const browser = useBrowserStore();

onMounted(() => void browser.open(props.dot.id));
watch(() => props.dot.id, (id) => void browser.open(id));
onUnmounted(() => void browser.close());
</script>

<template>
  <aside class="flex w-[520px] shrink-0 flex-col overflow-hidden border-l border-zinc-800/80 bg-zinc-950">
    <div class="flex items-center gap-3 border-b border-zinc-800/80 px-4 py-3">
      <div class="min-w-0 flex-1">
        <p class="truncate text-sm font-semibold text-zinc-100">Computador de {{ dot.name }}</p>
        <p class="truncate text-xs text-zinc-500">
          <template v-if="browser.phase === 'live'">Ao vivo · {{ browser.status?.binary ?? 'navegador' }}</template>
          <template v-else-if="browser.phase === 'connecting'">Conectando ao computador de {{ dot.name }}…</template>
          <template v-else-if="browser.phase === 'error'">Sem navegador</template>
          <template v-else>Navegador parado</template>
        </p>
      </div>
      <button class="btn-ghost -mr-2 px-2" title="Fechar" @click="emit('close')"><X :size="18" /></button>
    </div>

    <div class="flex flex-1 flex-col items-center justify-center overflow-hidden bg-gradient-to-b from-sky-900/40 to-zinc-950 p-5">
      <div class="flex w-full max-w-full flex-col overflow-hidden rounded-lg border border-zinc-700/70 bg-zinc-900 shadow-2xl">
        <div class="flex items-center gap-2 border-b border-zinc-800 bg-zinc-900 px-3 py-1.5">
          <span class="flex gap-1"><i class="h-2.5 w-2.5 rounded-full bg-red-500/80"></i><i class="h-2.5 w-2.5 rounded-full bg-amber-400/80"></i><i class="h-2.5 w-2.5 rounded-full bg-emerald-500/80"></i></span>
          <span class="ml-1 flex min-w-0 flex-1 items-center gap-1.5 rounded-md bg-zinc-800 px-2 py-0.5 text-[11px] text-zinc-400">
            <Globe :size="11" class="shrink-0" />
            <span class="truncate">{{ browser.status?.url || 'about:blank' }}</span>
          </span>
        </div>
        <div class="relative flex aspect-[1280/900] w-full items-center justify-center bg-zinc-950">
          <img v-if="browser.frame" :src="browser.frame" class="h-full w-full object-contain" alt="" draggable="false" />
          <div v-else class="flex flex-col items-center gap-3 text-sm text-zinc-400">
            <template v-if="browser.phase === 'error'">
              <p class="max-w-xs text-center">{{ browser.error }}</p>
              <p class="max-w-xs text-center text-xs text-zinc-500">Instale o Google Chrome ou informe o caminho do navegador nas configuracoes.</p>
            </template>
            <template v-else>
              <span class="h-6 w-6 animate-spin rounded-full border-2 border-zinc-600 border-t-zinc-200"></span>
              <p>Conectando ao computador de {{ dot.name }}…</p>
            </template>
          </div>
        </div>
      </div>
      <p v-if="browser.status?.title" class="mt-3 max-w-full truncate text-xs text-zinc-400">{{ browser.status.title }}</p>
      <p class="mt-1 max-w-sm text-center text-[11px] text-zinc-500">Voce ve o que {{ dot.name }} ve. O navegador e dele: cookies e logins ficam guardados so para este DOT.</p>
    </div>
  </aside>
</template>
