<script setup lang="ts">
defineProps<{ modelValue: boolean; disabled?: boolean; label?: string; description?: string }>();
const emit = defineEmits<{ "update:modelValue": [v: boolean] }>();
</script>

<template>
  <label class="flex cursor-pointer items-start justify-between gap-4" :class="disabled ? 'opacity-50' : ''">
    <span v-if="label" class="min-w-0">
      <span class="block text-sm text-zinc-100">{{ label }}</span>
      <span v-if="description" class="block text-xs text-zinc-500">{{ description }}</span>
    </span>
    <button type="button" role="switch" :aria-checked="modelValue" class="relative mt-0.5 h-6 w-11 shrink-0 rounded-full transition-colors" :class="modelValue ? 'bg-emerald-500' : 'bg-zinc-700'" :disabled="disabled" @click="emit('update:modelValue', !modelValue)">
      <span class="absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-white shadow transition-transform" :class="modelValue ? 'translate-x-5' : 'translate-x-0'"></span>
    </button>
  </label>
</template>
