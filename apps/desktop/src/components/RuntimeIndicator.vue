<script setup lang="ts">
import { computed } from "vue";
import { useRuntimeStore } from "@/stores/runtime";

const runtime = useRuntimeStore();
const label = computed(() => ({ connected: "Servico ativo", connecting: "Iniciando...", disconnected: "Servico parado", error: "Servico com problema" }[runtime.connection]));
const color = computed(() => ({ connected: "bg-emerald-500", connecting: "bg-amber-400 animate-pulse", disconnected: "bg-zinc-500", error: "bg-red-500" }[runtime.connection]));
</script>

<template>
  <div class="flex items-center gap-2 text-xs text-zinc-400">
    <span class="inline-block h-2 w-2 rounded-full" :class="color"></span>
    <span class="truncate" :title="runtime.error ?? ''">{{ label }}</span>
    <button v-if="runtime.connection === 'disconnected' || runtime.connection === 'error'" class="ml-auto text-indigo-400 hover:underline" :disabled="runtime.busy" @click="runtime.ensure()">ligar</button>
  </div>
</template>
