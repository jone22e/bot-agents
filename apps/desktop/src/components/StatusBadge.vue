<script setup lang="ts">
import { computed } from "vue";
import type { DotStatus } from "@dots/shared-types";

const props = defineProps<{ status: DotStatus }>();
const map: Record<DotStatus, { label: string; cls: string }> = {
  active: { label: "Ativo", cls: "bg-emerald-500/15 text-emerald-300" },
  paused: { label: "Pausado", cls: "bg-amber-500/15 text-amber-300" },
  archived: { label: "Arquivado", cls: "bg-zinc-700/50 text-zinc-300" },
};
const item = computed(() => map[props.status]);
</script>

<template>
  <span class="badge" :class="item.cls">{{ item.label }}</span>
</template>
