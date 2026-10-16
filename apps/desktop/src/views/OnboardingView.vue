<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import flexiIcon from "@/assets/flexi-icon.svg";
import ErrorBanner from "@/components/ErrorBanner.vue";
import FlexiBrowserLogin from "@/components/FlexiBrowserLogin.vue";
import { ENGINE_LABEL } from "@/lib/humanize";
import { useAppsStore } from "@/stores/apps";
import { useEnginesStore } from "@/stores/engines";
import { useRuntimeStore } from "@/stores/runtime";
import { useSettingsStore } from "@/stores/settings";

const router = useRouter();
const runtime = useRuntimeStore();
const engines = useEnginesStore();
const settings = useSettingsStore();
const apps = useAppsStore();
/** Primeira tela: "Logar com Flexi" (pode pular). Depois seguem os passos de sempre. */
const flexiDone = ref(false);
const step = ref(0);
const finishing = ref(false);

onMounted(() => { if (runtime.isConnected && engines.items.length === 0) engines.load(); });

const installed = computed(() => engines.items.filter((e) => e.last_detection?.installed));
const steps = ["Bem-vindo", "Servico", "Assistentes de IA", "Pronto"];

async function finish(createDot: boolean) {
  finishing.value = true;
  try {
    await settings.completeOnboarding();
    router.replace(createDot ? "/dots/new" : "/");
  } finally {
    finishing.value = false;
  }
}
</script>

<template>
  <div v-if="!flexiDone" class="flex h-full items-center justify-center bg-zinc-950 p-8">
    <div class="card w-full max-w-sm text-center">
      <img :src="flexiIcon" alt="" class="mx-auto h-14 w-14" />
      <h1 class="mt-4 text-lg font-semibold">Entre com a sua conta do Flexi</h1>
      <p class="mt-2 text-sm text-zinc-400">O Flexi abre no navegador, onde voce ja esta logado. Confirme o acesso la e volte para ca.</p>
      <div class="mt-5 text-left">
        <FlexiBrowserLogin large @done="flexiDone = true" />
        <ErrorBanner :message="apps.error" />
      </div>
      <button v-if="!apps.browserLogin" class="btn-ghost mt-3 text-xs" @click="flexiDone = true">Continuar sem o Flexi</button>
    </div>
  </div>
  <div v-else class="flex h-full items-center justify-center bg-zinc-950 p-8">
    <div class="w-full max-w-xl">
      <ol class="mb-6 flex gap-2 text-xs">
        <li v-for="(s, i) in steps" :key="s" class="flex items-center gap-2" :class="i === step ? 'text-indigo-300' : 'text-zinc-500'">
          <span class="inline-flex h-5 w-5 items-center justify-center rounded-full border" :class="i <= step ? 'border-indigo-400' : 'border-zinc-700'">{{ i + 1 }}</span>{{ s }}
        </li>
      </ol>

      <div class="card min-h-64">
        <template v-if="step === 0">
          <h1 class="text-lg font-semibold">Bem-vindo ao DOTS</h1>
          <p class="mt-2 text-sm text-zinc-300">Um DOT e um ajudante digital que voce cria com um nome e uma tarefa fixa. Ele usa um assistente de IA ja instalado no seu Mac, lembra do que fez e so faz o que voce permitir. Vamos conferir se esta tudo pronto.</p>
        </template>
        <template v-else-if="step === 1">
          <h1 class="text-lg font-semibold">Servico em segundo plano</h1>
          <p class="mt-2 text-sm text-zinc-300">Ele guarda seus dados e deixa os DOTs trabalharem mesmo com a janela fechada.</p>
          <p class="mt-4 text-sm" :class="runtime.isConnected ? 'text-emerald-300' : 'text-amber-300'">
            {{ runtime.isConnected ? "Servico ativo." : runtime.error ?? "Ainda iniciando..." }}
          </p>
          <button v-if="!runtime.isConnected" class="btn-secondary mt-3" :disabled="runtime.busy" @click="runtime.ensure()">Tentar ligar</button>
        </template>
        <template v-else-if="step === 2">
          <h1 class="text-lg font-semibold">Assistentes de IA</h1>
          <p class="mt-2 text-sm text-zinc-300">Os DOTs usam assistentes ja instalados no seu Mac (Codex, Claude Code ou Antigravity). Encontrados:</p>
          <ul class="mt-3 space-y-1 text-sm">
            <li v-for="e in engines.items" :key="e.kind" class="flex justify-between">
              <span>{{ ENGINE_LABEL[e.kind] }}</span>
              <span v-if="!e.last_detection" class="text-zinc-500">verificando...</span>
              <span v-else-if="e.last_detection.installed" :class="e.last_detection.auth_status === 'unauthenticated' ? 'text-amber-300' : 'text-emerald-300'">{{ e.last_detection.auth_status === "authenticated" ? "pronto" : e.last_detection.auth_status === "unauthenticated" ? "precisa entrar pelo Terminal" : "pronto (login nao verificavel)" }}</span>
              <span v-else class="text-zinc-500">nao instalado</span>
            </li>
          </ul>
          <p v-if="engines.detectedAll && installed.length === 0" class="mt-3 text-xs text-amber-300">Nenhum assistente encontrado. Instale um deles e volte aqui; voce pode criar DOTs mesmo assim.</p>
          <button class="btn-secondary mt-3" :disabled="engines.detecting !== null" @click="engines.detect()">Verificar de novo</button>
        </template>
        <template v-else>
          <h1 class="text-lg font-semibold">Tudo pronto</h1>
          <p class="mt-2 text-sm text-zinc-300">Crie o primeiro DOT: um nome, o que ele faz, e pronto.</p>
        </template>
      </div>

      <div class="mt-4 flex justify-between">
        <button class="btn-ghost" :disabled="step === 0" @click="step--">Voltar</button>
        <div class="flex gap-2">
          <template v-if="step < steps.length - 1">
            <button class="btn-primary" :disabled="step === 1 && !runtime.isConnected" @click="step++">Continuar</button>
          </template>
          <template v-else>
            <button class="btn-secondary" :disabled="finishing" @click="finish(false)">Ir para a visao geral</button>
            <button class="btn-primary" :disabled="finishing" @click="finish(true)">Criar o primeiro DOT</button>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>
