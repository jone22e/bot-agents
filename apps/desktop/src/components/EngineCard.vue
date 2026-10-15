<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { AgySetupStatus, EngineConfiguration } from "@dots/shared-types";
import { ENGINE_LABEL } from "@/lib/humanize";
import ModelPicker from "@/components/ModelPicker.vue";

const props = defineProps<{ config: EngineConfiguration; busy: boolean; agy?: AgySetupStatus | null; agyBusy?: boolean }>();
const emit = defineEmits<{ detect: []; override: [path: string | null]; toggle: [enabled: boolean]; model: [model: string | null]; agySetup: [remove: boolean] }>();

const det = computed(() => props.config.last_detection);
const overridePath = ref(props.config.executable_override ?? "");
watch(() => props.config.executable_override, (v) => (overridePath.value = v ?? ""));
const model = ref(props.config.model ?? "");
watch(() => props.config.model, (v) => (model.value = v ?? ""));
const modelDirty = computed(() => model.value.trim() !== (props.config.model ?? ""));
const cli = computed(() => ({ codex: "codex", claude: "claude", agy: "agy" }[props.config.kind]));

const state = computed<{ title: string; help: string; cls: string }>(() => {
  const d = det.value;
  if (!d) return { title: "Verificando...", help: "", cls: "bg-zinc-700/50 text-zinc-300" };
  if (!d.installed) return { title: "Nao instalado", help: d.guidance ?? "", cls: "bg-zinc-700/50 text-zinc-300" };
  if (!props.config.enabled) return { title: "Desligado", help: "Ligue para que os DOTs possam usar.", cls: "bg-zinc-700/50 text-zinc-300" };
  if (d.auth_status === "unauthenticated") return { title: "Precisa entrar", help: `Abra o Terminal, rode \`${cli.value}\` e faca login. Depois clique em Verificar de novo.`, cls: "bg-amber-500/15 text-amber-300" };
  if (props.config.kind === "agy" && props.agy && !props.agy.configured) return { title: "Instalado, falta integrar", help: "Clique em Integrar ao DOTS abaixo.", cls: "bg-amber-500/15 text-amber-300" };
  if (d.auth_status === "unknown") return { title: "Pronto (login nao verificavel)", help: "Este assistente nao informa se esta logado. Se um trabalho falhar por login, entre pelo Terminal.", cls: "bg-emerald-500/15 text-emerald-300" };
  return { title: "Pronto", help: "", cls: "bg-emerald-500/15 text-emerald-300" };
});
</script>

<template>
  <div class="card">
    <div class="flex items-start justify-between gap-4">
      <div>
        <h3 class="text-sm font-semibold text-zinc-100">{{ ENGINE_LABEL[config.kind] }}</h3>
        <p v-if="det?.installed" class="text-xs text-zinc-500">versao {{ det.version }}</p>
      </div>
      <div class="flex items-center gap-2">
        <span class="badge" :class="state.cls">{{ state.title }}</span>
        <label v-if="det?.installed" class="ml-2 flex items-center gap-1 text-xs text-zinc-400"><input type="checkbox" :checked="config.enabled" @change="emit('toggle', ($event.target as HTMLInputElement).checked)" /> ligado</label>
      </div>
    </div>
    <p v-if="state.help" class="mt-2 text-sm text-zinc-300">{{ state.help }}</p>
    <p v-if="det?.issues.length" class="mt-2 text-xs text-amber-200">{{ det.issues.join(" · ") }}</p>

    <div v-if="config.kind === 'agy' && det?.installed" class="mt-3 rounded-md border border-zinc-800 bg-zinc-950/60 p-3 text-xs">
      <p class="text-zinc-300">O Antigravity precisa de uma integracao unica para que os DOTs usem as ferramentas do DOTS. Ela altera a configuracao global do Antigravity (com copia de seguranca) e pode ser removida a qualquer momento.</p>
      <div class="mt-2 flex items-center gap-2">
        <button class="btn-primary" :disabled="agyBusy" @click="emit('agySetup', false)">{{ agy?.configured ? "Reaplicar integracao" : "Integrar ao DOTS" }}</button>
        <button v-if="agy?.server_registered || agy?.rules_present.length" class="btn-secondary" :disabled="agyBusy" @click="emit('agySetup', true)">Remover integracao</button>
        <span v-if="agy?.configured" class="text-emerald-300">integrado</span>
      </div>
    </div>

    <div v-if="det?.installed" class="mt-3 flex items-end gap-2">
      <div class="flex-1">
        <label class="label" :for="`model-${config.kind}`">Modelo (opcional)</label>
        <ModelPicker :id="`model-${config.kind}`" v-model="model" :engine="config.kind" empty-label="Modelo padrao do assistente" />
      </div>
      <button class="btn-secondary" :disabled="busy || !modelDirty" @click="emit('model', model || null)">Aplicar</button>
    </div>
    <p v-if="det?.installed" class="mt-1 text-xs text-zinc-500">
      Vale para todos os DOTs que usam este assistente. Cada DOT pode escolher outro modelo em Avancado.
      <template v-if="config.kind !== 'claude'">{{ det.models?.length ? `A lista tem ${det.models.length} modelos informados pelo proprio assistente e e atualizada em Verificar de novo.` : "O assistente ainda nao informou a lista de modelos: clique em Verificar de novo." }}</template>
    </p>

    <div class="mt-3 flex items-center gap-2">
      <button class="btn-secondary" :disabled="busy" @click="emit('detect')">{{ busy ? "Verificando..." : "Verificar de novo" }}</button>
      <details class="flex-1 text-xs">
        <summary class="cursor-pointer text-zinc-500 hover:text-zinc-300">Detalhes tecnicos</summary>
        <dl v-if="det" class="mt-2 grid grid-cols-[120px_1fr] gap-y-1">
          <dt class="text-zinc-500">Executavel</dt><dd class="truncate font-mono text-zinc-300">{{ det.executable_path ?? "—" }}</dd>
          <dt class="text-zinc-500">Login</dt><dd class="text-zinc-300">{{ det.auth_detail ?? "—" }}</dd>
          <dt class="text-zinc-500">Verificado</dt><dd class="text-zinc-300">{{ new Date(det.checked_at).toLocaleString() }}</dd>
          <dt v-if="agy" class="text-zinc-500">Config. AGY</dt><dd v-if="agy" class="truncate font-mono text-zinc-300">{{ agy.settings_path }}</dd>
        </dl>
        <ul v-if="det?.capabilities.length" class="mt-2 space-y-0.5"><li v-for="c in det.capabilities" :key="c.id" class="text-zinc-400">{{ c.label }} <code class="text-zinc-600">{{ c.evidence }}</code></li></ul>
        <div class="mt-2 flex items-end gap-2">
          <div class="flex-1"><label class="label">Caminho do executavel (manual)</label><input v-model="overridePath" class="input font-mono text-xs" placeholder="/caminho/para/executavel" /></div>
          <button class="btn-secondary" :disabled="busy" @click="emit('override', overridePath || null)">Aplicar</button>
        </div>
      </details>
    </div>
  </div>
</template>
