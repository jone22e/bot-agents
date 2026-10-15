<script setup lang="ts">
defineProps<{ title: string; description?: string }>();
</script>

<template>
  <div class="card flex flex-col items-center justify-center py-16 text-center">
    <p class="text-sm font-medium text-zinc-200">{{ title }}</p>
    <p v-if="description" class="mt-1 max-w-md text-xs text-zinc-400">{{ description }}</p>
    <div class="mt-4"><slot /></div>
  </div>
</template>
