<script setup lang="ts">
import { computed, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import MessengerShell from "@/components/MessengerShell.vue";
import { useRuntimeEvents } from "@/composables/useRuntimeEvents";
import { Download, X } from "lucide-vue-next";
import { useRuntimeStore } from "@/stores/runtime";
import { useSettingsStore } from "@/stores/settings";
import { useUpdaterStore } from "@/stores/updater";
import { THEME_SETTING, applyTheme, rememberedTheme } from "@/lib/theme";

useRuntimeEvents();
const route = useRoute();
const router = useRouter();
const runtime = useRuntimeStore();
const settings = useSettingsStore();
const updater = useUpdaterStore();
void updater.init();
const bare = computed(() => route.meta.bare === true);

// Tema (skin): pinta com o ultimo lembrado antes de o servico responder; depois segue a configuracao.
applyTheme(rememberedTheme());
watch(
  () => [settings.loaded, settings.items[THEME_SETTING]] as const,
  ([loaded, id]) => { if (loaded) applyTheme(typeof id === "string" ? id : undefined); },
  { immediate: true },
);

watch(
  () => [settings.loaded, settings.onboardingCompleted, runtime.isConnected] as const,
  ([loaded, done, connected]) => {
    if (connected && loaded && !done && route.name !== "onboarding") router.replace({ name: "onboarding" });
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex h-full flex-col">
    <div v-if="updater.state?.status === 'ready' && !updater.dismissed" class="flex shrink-0 items-center gap-3 border-b border-zinc-800 px-3 py-2 text-[12.5px] text-zinc-100" style="background: var(--accent-soft)">
      <Download :size="15" class="shrink-0 text-violet-300" />
      <span class="flex-1">DOTS {{ updater.state.version }} está pronto para instalar.</span>
      <button class="rounded-md bg-violet-500 px-3 py-1 text-xs font-semibold text-white hover:bg-violet-400" @click="updater.install()">Reiniciar e atualizar</button>
      <button class="rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200" title="Depois (instala ao fechar o app)" @click="updater.dismissed = true"><X :size="14" /></button>
    </div>
    <div class="min-h-0 flex-1">
      <router-view v-if="bare" />
      <MessengerShell v-else>
        <router-view />
      </MessengerShell>
    </div>
  </div>
</template>
