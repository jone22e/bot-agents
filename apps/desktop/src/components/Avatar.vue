<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{ name: string; size?: "sm" | "md" | "lg"; state?: "idle" | "busy" | "attention" | string }>();
const palette = ["bg-indigo-500", "bg-emerald-500", "bg-sky-500", "bg-rose-500", "bg-amber-500", "bg-violet-500", "bg-teal-500"];
const color = computed(() => palette[[...props.name].reduce((n, c) => n + c.charCodeAt(0), 0) % palette.length]);
const initials = computed(() => props.name.split(/\s+/).filter(Boolean).slice(0, 2).map((w) => w[0]!.toUpperCase()).join("") || "?");
const sizeCls = computed(() => ({ sm: "h-7 w-7 text-xs", md: "h-10 w-10 text-sm", lg: "h-14 w-14 text-lg" }[props.size ?? "md"]));
const dot = computed(() => ({ idle: null, busy: "bg-sky-400", attention: "bg-amber-400" } as Record<string, string | null>)[props.state ?? "idle"] ?? null);
</script>

<template>
  <span class="relative inline-flex shrink-0 items-center justify-center rounded-full font-semibold text-white" :class="[color, sizeCls]">
    {{ initials }}
    <span v-if="dot" class="absolute -bottom-0.5 -right-0.5 h-3 w-3 rounded-full border-2 border-zinc-950" :class="dot"></span>
  </span>
</template>
