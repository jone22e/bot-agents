<script setup lang="ts">
import { computed } from "vue";
import { TASK_STATUS_LABEL } from "@/lib/humanize";

const props = defineProps<{ status: string }>();
const cls: Record<string, string> = {
  QUEUED: "bg-zinc-700/50 text-zinc-200", RUNNING: "bg-indigo-500/20 text-indigo-200", WAITING_USER: "bg-amber-500/15 text-amber-300",
  WAITING_APPROVAL: "bg-amber-500/25 text-amber-200", WAITING_EXTERNAL: "bg-amber-500/15 text-amber-300", PAUSED: "bg-zinc-700/50 text-zinc-300",
  COMPLETED: "bg-emerald-500/15 text-emerald-300", FAILED: "bg-red-500/15 text-red-300", CANCELLED: "bg-zinc-700/50 text-zinc-400", CANCELLING: "bg-zinc-700/50 text-zinc-400",
};
const label = computed(() => (TASK_STATUS_LABEL as Record<string, string>)[props.status] ?? props.status);
</script>
<template><span class="badge" :class="cls[status] ?? 'bg-zinc-700/50 text-zinc-300'">{{ label }}</span></template>
