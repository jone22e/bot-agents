<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import type { EngineKind } from "@dots/shared-types";
import flexiIcon from "@/assets/flexi-icon.svg";
import EngineCard from "@/components/EngineCard.vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import FlexiBrowserLogin from "@/components/FlexiBrowserLogin.vue";
import FlexiCard from "@/components/FlexiCard.vue";
import { ENGINE_LABEL } from "@/lib/humanize";
import { ACCOUNT_CONNECTION, AGENTS_CONNECTION, useAppsStore } from "@/stores/apps";
import { useEnginesStore } from "@/stores/engines";
import { useRuntimeStore } from "@/stores/runtime";
import { useSettingsStore } from "@/stores/settings";
import { useUsageStore } from "@/stores/usage";

const router = useRouter();
const runtime = useRuntimeStore();
const engines = useEnginesStore();
const settings = useSettingsStore();
const apps = useAppsStore();
const usage = useUsageStore();
/** A conta do usuario precisa estar conectada antes dos passos de criacao dos DOTs. */
const flexiDone = computed(() => apps.flexi.some((c) => c.id === settings.items[ACCOUNT_CONNECTION] && c.status === "connected"));
const personal = computed(() => apps.flexi.find((c) => c.id === settings.items[ACCOUNT_CONNECTION]));
const agent = computed(() => apps.flexi.find((c) => c.id === settings.items[AGENTS_CONNECTION]));
const agentReady = computed(() => agent.value?.status === "connected");
const step = ref(0);
const finishing = ref(false);
const error = ref<string | null>(null);
const selectedEngine = ref<EngineKind>("codex");
const engine = computed(() => engines.byKind(selectedEngine.value));
const engineReady = computed(() => {
  const cfg = engine.value;
  return !!(cfg?.enabled && cfg.last_detection?.installed && cfg.last_detection.auth_status !== "unauthenticated" && cfg.model?.trim()
    && (cfg.kind !== "agy" || engines.agy?.configured));
});

watch(
  () => [flexiDone.value, settings.onboardingCompleted] as const,
  ([connected, done]) => { if (connected && done && !finishing.value) void router.replace("/"); },
  { immediate: true },
);

onMounted(async () => {
  if (!runtime.isConnected) return;
  await Promise.allSettled([engines.load(), usage.load()]);
  if (engines.byKind("agy")?.last_detection?.installed) await engines.agyStatus();
  selectedEngine.value = engines.items.find((e) => e.enabled && e.last_detection?.installed && e.last_detection.auth_status !== "unauthenticated")?.kind ?? engines.items[0]?.kind ?? "codex";
});

async function installEngine(kind: EngineKind) {
  await usage.install(kind);
  if (!usage.errors[kind]) {
    await engines.detect(kind);
    if (kind === "agy") await engines.agyStatus();
  }
}

async function finish(createDot: boolean) {
  if (!agentReady.value || !engineReady.value) return;
  finishing.value = true;
  error.value = null;
  try {
    await settings.completeOnboarding();
    await router.replace(createDot ? "/dots/new" : "/");
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
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
        <FlexiBrowserLogin large />
        <ErrorBanner :message="apps.error" />
      </div>
    </div>
  </div>
  <div v-else class="flex h-full items-center justify-center overflow-y-auto bg-zinc-950 p-5">
    <div role="dialog" aria-modal="true" aria-label="Primeiros passos" class="max-h-full w-full max-w-4xl overflow-y-auto rounded-2xl border border-zinc-800 bg-zinc-900 p-5 shadow-2xl sm:p-7">
      <h1 class="text-xl font-semibold text-zinc-100">Primeiros passos</h1>
      <p class="mt-1 text-sm text-zinc-400">Você entrou no DOTS como {{ personal?.user_name ?? personal?.email }}. Agora prepare o acesso dos agentes.</p>
      <ol class="mb-5 mt-5 flex gap-4 border-b border-zinc-800 pb-4 text-sm">
        <li :class="step === 0 ? 'text-violet-300' : 'text-emerald-300'">1. Conta dos agentes</li>
        <li :class="step === 1 ? 'text-violet-300' : 'text-zinc-500'">2. Assistente de IA</li>
      </ol>

      <template v-if="step === 0">
        <h2 class="text-base font-semibold text-zinc-100">Escolha quem os agentes serão no Flexi</h2>
        <p class="mb-4 mt-1 text-sm text-zinc-400">Conecte outro usuário ou escolha uma conta já conectada. Os agentes existentes e novos usarão a conta marcada como “Conta dos agentes”.</p>
        <FlexiCard agent-setup />
      </template>
      <template v-else>
        <h2 class="text-base font-semibold text-zinc-100">Configure o assistente de IA</h2>
        <p class="mt-1 text-sm text-zinc-400">Escolha Codex, Claude Code ou Antigravity. Instale se necessário, entre na conta e defina o modelo padrão dos DOTs.</p>
        <div class="mt-4 flex flex-wrap gap-2">
          <button v-for="cfg in engines.items" :key="cfg.kind" class="rounded-lg border px-3 py-2 text-sm" :class="selectedEngine === cfg.kind ? 'border-violet-500 bg-violet-500/15 text-violet-200' : 'border-zinc-700 text-zinc-300 hover:bg-zinc-800'" @click="selectedEngine = cfg.kind">{{ ENGINE_LABEL[cfg.kind] }}</button>
        </div>
        <div v-if="!engines.items.length" class="mt-4 flex items-center gap-3 text-sm text-zinc-400">
          <span>Nenhum assistente disponível.</span>
          <button class="btn-secondary" :disabled="engines.loading" @click="engines.load()">Verificar de novo</button>
        </div>
        <EngineCard
          v-if="engine" :key="engine.kind" class="mt-4" :config="engine"
          :busy="engines.detecting === engine.kind || engines.detecting === 'all'"
          :agy="engine.kind === 'agy' ? engines.agy : null" :agy-busy="engines.agyBusy"
          :usage="usage.usage?.[engine.kind] ?? null" :update="usage.updates?.[engine.kind] ?? null"
          :updating="usage.updating === engine.kind" :installing="usage.installing === engine.kind"
          :cli-error="usage.errors[engine.kind] ?? null" :installed-now="usage.installed[engine.kind] ?? false"
          :updated-now="usage.updated[engine.kind] ?? null"
          @detect="engines.detect(engine.kind)" @override="(path) => engines.setOverride(engine!.kind, path)"
          @toggle="(enabled) => engines.setEnabled(engine!.kind, enabled)" @model="(model) => engines.setModel(engine!.kind, model)"
          @agy-setup="(remove) => engines.agySetup(remove)" @cli-update="usage.update(engine.kind)" @cli-install="installEngine(engine.kind)"
        />
        <p v-if="engine && !engineReady" class="mt-3 text-xs text-amber-300">Para concluir, deixe o assistente ligado, com login pronto e um modelo escolhido. O Antigravity também precisa da integração ao DOTS.</p>
        <ErrorBanner :message="engines.error" />
      </template>

      <ErrorBanner :message="error" />
      <div class="mt-6 flex items-center justify-between border-t border-zinc-800 pt-4">
        <button class="btn-ghost" :disabled="step === 0 || finishing" @click="step = 0">Voltar</button>
        <button v-if="step === 0" class="btn-primary" :disabled="!agentReady" @click="step = 1">Continuar</button>
        <div v-else class="flex gap-2">
          <button class="btn-secondary" :disabled="!engineReady || finishing" @click="finish(false)">Ir para a visão geral</button>
          <button class="btn-primary" :disabled="!engineReady || finishing" @click="finish(true)">Criar o primeiro DOT</button>
        </div>
      </div>
    </div>
  </div>
</template>
