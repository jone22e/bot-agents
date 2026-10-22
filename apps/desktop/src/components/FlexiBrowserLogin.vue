<script setup lang="ts">
import { ref } from "vue";
import { ExternalLink, Loader2 } from "lucide-vue-next";
import type { AppConnection } from "@dots/shared-types";
import flexiIcon from "@/assets/flexi-icon.svg";
import { useAppsStore } from "@/stores/apps";

/** Botao "Logar com Flexi": abre o Flexi no navegador e espera o usuario autorizar la. */
const props = defineProps<{ baseUrl?: string | null; label?: string; large?: boolean }>();
const emit = defineEmits<{ done: [connection: AppConnection] }>();
const apps = useAppsStore();
const linkInput = ref<HTMLInputElement | null>(null);
const copyMessage = ref("");

async function start() {
  copyMessage.value = "";
  const c = await apps.loginWithBrowser(props.baseUrl);
  if (c) emit("done", c);
}

async function copyLink() {
  const url = apps.browserLogin?.authorize_url;
  if (!url) return;
  try {
    await navigator.clipboard.writeText(url);
    copyMessage.value = "Link copiado. Cole na barra de endereços do navegador.";
  } catch {
    linkInput.value?.select();
    copyMessage.value = "Link selecionado. Copie e cole na barra de endereços do navegador.";
  }
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
    <div class="mt-3 border-t border-zinc-800 pt-3">
      <p class="text-xs text-zinc-400">Se o navegador mostrar código HTML, copie este link e cole na barra de endereços:</p>
      <div class="mt-2 flex gap-2">
        <input ref="linkInput" class="input min-w-0 flex-1 font-mono text-xs" aria-label="Link de autorização do Flexi" :value="apps.browserLogin.authorize_url" readonly @focus="linkInput?.select()" />
        <button class="btn-secondary shrink-0" @click="copyLink">Copiar link</button>
      </div>
      <p v-if="copyMessage" class="mt-2 text-xs text-zinc-300" role="status">{{ copyMessage }}</p>
    </div>
  </div>
  <button v-else class="btn-primary inline-flex items-center justify-center gap-2" :class="large ? 'h-11 w-full text-base' : ''" @click="start">
    <img :src="flexiIcon" alt="" class="h-5 w-5" /> {{ label ?? "Logar com Flexi" }}
  </button>
</template>
