<script setup lang="ts">
import { computed, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import MessengerShell from "@/components/MessengerShell.vue";
import ManagerRequestAlert from "@/components/ManagerRequestAlert.vue";
import { useRuntimeEvents } from "@/composables/useRuntimeEvents";
import { Download, X } from "lucide-vue-next";
import { useRuntimeStore } from "@/stores/runtime";
import { useSettingsStore } from "@/stores/settings";
import { ACCOUNT_CONNECTION, useAppsStore } from "@/stores/apps";
import { useUpdaterStore } from "@/stores/updater";
import { THEME_SETTING, applyTheme, rememberedTheme } from "@/lib/theme";

useRuntimeEvents();
const route = useRoute();
const router = useRouter();
const runtime = useRuntimeStore();
const settings = useSettingsStore();
const apps = useAppsStore();
const updater = useUpdaterStore();
void updater.init();
const bare = computed(() => route.meta.bare === true);
const ready = computed(() => runtime.isConnected && settings.loaded && apps.loaded);
const accountConnected = computed(() => apps.flexi.some((c) => c.id === settings.items[ACCOUNT_CONNECTION] && c.status === "connected"));
const needsOnboarding = computed(() => !settings.onboardingCompleted || !accountConnected.value);

// Tema (skin): pinta com o ultimo lembrado antes de o servico responder; depois segue a configuracao.
applyTheme(rememberedTheme());
watch(
  () => [settings.loaded, settings.items[THEME_SETTING]] as const,
  ([loaded, id]) => { if (loaded) applyTheme(typeof id === "string" ? id : undefined); },
  { immediate: true },
);

watch(
  () => [ready.value, needsOnboarding.value, route.name] as const,
  ([loaded, required, name]) => {
    if (loaded && required && name !== "onboarding") void router.replace({ name: "onboarding" });
  },
  { immediate: true },
);

async function retry() {
  if (!runtime.isConnected) await runtime.ensure();
  if (runtime.isConnected) await Promise.allSettled([settings.load(), apps.load()]);
}
</script>

<template>
  <div class="flex h-full flex-col">
    <div v-if="updater.state?.status === 'ready' && !updater.dismissed" class="flex shrink-0 items-center gap-3 border-b border-zinc-800 px-3 py-2 text-[12.5px] text-zinc-100" style="background: var(--accent-soft)">
      <Download :size="15" class="shrink-0 text-violet-300" />
      <span class="flex-1">DOTS {{ updater.state.version }} está pronto para instalar.</span>
      <button class="rounded-md bg-violet-500 px-3 py-1 text-xs font-semibold text-white hover:bg-violet-400" @click="updater.install()">Reiniciar e atualizar</button>
      <button class="rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200" title="Depois (instala ao fechar o app)" @click="updater.dismissed = true"><X :size="14" /></button>
    </div>
    <ManagerRequestAlert v-if="ready && !needsOnboarding" />
    <div class="min-h-0 flex-1">
      <div v-if="!ready || (needsOnboarding && !bare)" class="flex h-full items-center justify-center bg-zinc-950 p-8">
        <div class="card w-full max-w-sm text-center">
          <h1 class="text-lg font-semibold">Iniciando DOTS</h1>
          <p class="mt-2 text-sm text-zinc-400">{{ runtime.error ?? apps.error ?? "Carregando sua conta..." }}</p>
          <button v-if="!ready && (runtime.connection === 'error' || runtime.connection === 'disconnected' || apps.error)" class="btn-secondary mt-4" :disabled="runtime.busy" @click="retry">Tentar novamente</button>
        </div>
      </div>
      <router-view v-else-if="bare" />
      <MessengerShell v-else>
        <router-view />
      </MessengerShell>
    </div>
  </div>
</template>
