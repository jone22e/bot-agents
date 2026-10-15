<script setup lang="ts">
defineProps<{ message: string | null }>();
</script>

<template>
  <div v-if="message" class="mb-4 rounded-md border border-red-900/60 bg-red-950/40 px-3 py-2 text-sm text-red-200">
    {{ message }}
  </div>
</template>
