<script setup lang="ts">
defineProps<{ open: boolean; title: string; message: string; confirmLabel?: string; danger?: boolean }>();
const emit = defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60" @click.self="emit('cancel')">
      <div class="w-full max-w-md rounded-lg border border-zinc-800 bg-zinc-900 p-5 shadow-xl">
        <h2 class="text-sm font-semibold text-zinc-100">{{ title }}</h2>
        <p class="mt-2 text-sm text-zinc-300">{{ message }}</p>
        <div class="mt-5 flex justify-end gap-2">
          <button class="btn-secondary" @click="emit('cancel')">Cancelar</button>
          <button :class="danger ? 'btn-danger' : 'btn-primary'" @click="emit('confirm')">{{ confirmLabel ?? "Confirmar" }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
