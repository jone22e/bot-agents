<script setup lang="ts">
import { useRouter } from "vue-router";
import { ArrowLeft } from "lucide-vue-next";

defineProps<{ title: string; subtitle?: string }>();
const router = useRouter();
</script>

<template>
  <header class="flex items-center gap-3 px-6 py-4" data-tauri-drag-region>
    <button class="flex h-8 w-8 items-center justify-center rounded-full bg-zinc-800 text-zinc-300 hover:bg-zinc-700" title="Voltar" @click="router.back()"><ArrowLeft :size="16" /></button>
    <div>
      <h1 class="text-base font-semibold text-zinc-100">{{ title }}</h1>
      <p v-if="subtitle || $slots.subtitle" class="text-xs text-zinc-500"><slot name="subtitle">{{ subtitle }}</slot></p>
    </div>
    <div class="ml-auto"><slot /></div>
  </header>
</template>
