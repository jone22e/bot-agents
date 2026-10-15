<script setup lang="ts">
import { computed, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import MessengerShell from "@/components/MessengerShell.vue";
import { useRuntimeEvents } from "@/composables/useRuntimeEvents";
import { useRuntimeStore } from "@/stores/runtime";
import { useSettingsStore } from "@/stores/settings";

useRuntimeEvents();
const route = useRoute();
const router = useRouter();
const runtime = useRuntimeStore();
const settings = useSettingsStore();
const bare = computed(() => route.meta.bare === true);

watch(
  () => [settings.loaded, settings.onboardingCompleted, runtime.isConnected] as const,
  ([loaded, done, connected]) => {
    if (connected && loaded && !done && route.name !== "onboarding") router.replace({ name: "onboarding" });
  },
  { immediate: true },
);
</script>

<template>
  <router-view v-if="bare" />
  <MessengerShell v-else>
    <router-view />
  </MessengerShell>
</template>
