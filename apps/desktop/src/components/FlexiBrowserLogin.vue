<script setup lang="ts">
import { ExternalLink, Loader2 } from "lucide-vue-next";
import type { AppConnection } from "@dots/shared-types";
import flexiIcon from "@/assets/flexi-icon.svg";
import { useAppsStore } from "@/stores/apps";

/** Botao "Logar com Flexi": abre o Flexi no navegador e espera o usuario autorizar la. */
const props = defineProps<{ baseUrl?: string | null; label?: string; large?: boolean }>();
const emit = defineEmits<{ done: [connection: AppConnection] }>();
const apps = useAppsStore();

async function start() {
  const c = await apps.loginWithBrowser(props.baseUrl);
  if (c) emit("done", c);
}
</script>

<template>
  <div v-if="apps.browserLogin" class="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4 text-sm">
    <p class="flex items-center gap-2 text-zinc-200"><Loader2 :size="15" class="animate-spin text-indigo-300" /> Aguardando a autorizacao no navegador...</p>
    <p class="mt-2 text-xs text-zinc-400">Confira se o Flexi mostra este mesmo codigo antes de autorizar:</p>
    <p class="mt-1 font-mono text-2xl font-semibold tracking-[0.3em] text-zinc-100">{{ apps.browserLogin.code }}</p>
    <div class="mt-3 flex gap-2">
      <button class="btn-secondary" @click="apps.openBrowserLogin()"><ExternalLink :size="14" /> Abrir o navegador de novo</button>
      <button class="btn-ghost" @click="apps.cancelBrowserLogin()">Cancelar</button>
    </div>
  </div>
  <button v-else class="btn-primary inline-flex items-center justify-center gap-2" :class="large ? 'h-11 w-full text-base' : ''" @click="start">
    <img :src="flexiIcon" alt="" class="h-5 w-5" /> {{ label ?? "Logar com Flexi" }}
  </button>
</template>
