<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Download } from "lucide-vue-next";
import type { AgySetupStatus, CliUpdateInfo, EngineConfiguration, ProviderUsage, UsageWindow } from "@dots/shared-types";
import { ENGINE_LABEL } from "@/lib/humanize";
import ModelPicker from "@/components/ModelPicker.vue";
import { PROVIDER_LOGIN, resetsIn } from "@/stores/usage";

const props = defineProps<{
  config: EngineConfiguration; busy: boolean; agy?: AgySetupStatus | null; agyBusy?: boolean;
  /** Uso da assinatura e versao (como no ovrgit); instalar/atualizar rodam no servico. */
  usage?: ProviderUsage | null; update?: CliUpdateInfo | null; updating?: boolean; installing?: boolean; cliError?: string | null; installedNow?: boolean; updatedNow?: string | null;
}>();
const emit = defineEmits<{ detect: []; override: [path: string | null]; toggle: [enabled: boolean]; model: [model: string | null]; agySetup: [remove: boolean]; cliUpdate: []; cliInstall: [] }>();
const windows = computed((): [string, UsageWindow | null][] => (!props.usage || props.config.kind === "agy" ? [] : [["Semana", props.usage.week], ["5 horas", props.usage.five_hour]]));
const pctTone = (pct: number) => (pct >= 90 ? "bg-red-500" : pct >= 70 ? "bg-amber-500" : "bg-violet-500");
/** "Codex (OpenAI)" vira nome e fornecedor. */
const name = computed(() => ENGINE_LABEL[props.config.kind].replace(/\s*\(.*\)$/, ""));
const provider = computed(() => ENGINE_LABEL[props.config.kind].match(/\(([^)]*)\)$/)?.[1] ?? "");
const version = computed(() => { const v = props.update?.current ?? det.value?.version; return v ? `v${String(v).replace(/^v/i, "")}` : ""; });

const det = computed(() => props.config.last_detection);
const overridePath = ref(props.config.executable_override ?? "");
watch(() => props.config.executable_override, (v) => (overridePath.value = v ?? ""));
const model = ref(props.config.model ?? "");
watch(() => props.config.model, (v) => (model.value = v ?? ""));
const modelDirty = computed(() => model.value.trim() !== (props.config.model ?? ""));
const cli = computed(() => ({ codex: "codex", claude: "claude", agy: "agy" }[props.config.kind]));

const dotCls = computed(() => (state.value.cls.includes("emerald") ? "bg-emerald-400" : state.value.cls.includes("amber") ? "bg-amber-400" : "bg-zinc-500"));
const textCls = computed(() => (state.value.cls.includes("emerald") ? "text-emerald-400" : state.value.cls.includes("amber") ? "text-amber-300" : "text-zinc-500"));
const off = computed(() => !!det.value?.installed && !props.config.enabled);
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
  <div class="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/60">
    <div class="flex items-start justify-between gap-4 px-5 py-4">
      <div class="min-w-0">
        <h3 class="flex items-baseline gap-2">
          <span class="text-[15px] font-semibold" :class="off ? 'text-zinc-400' : 'text-zinc-100'">{{ name }}</span>
          <span v-if="provider" class="text-[13px] text-zinc-500">{{ provider }}</span>
        </h3>
        <p v-if="det?.installed" class="mt-0.5 flex items-center gap-1.5 font-mono text-xs text-zinc-500">
          <span>{{ version }}</span>
          <template v-if="update?.available"><span>·</span><span class="font-semibold text-violet-400">nova: v{{ String(update.latest).replace(/^v/i, "") }}</span></template>
          <template v-else-if="updatedNow"><span>·</span><span class="text-emerald-400">atualizado</span></template>
          <template v-else-if="update?.latest"><span>·</span><span class="text-emerald-400">em dia</span></template>
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-3">
        <span class="flex items-center gap-1.5 text-[13px]" :class="textCls"><span class="h-1.5 w-1.5 rounded-full" :class="dotCls"></span>{{ state.title }}</span>
        <button v-if="det?.installed" type="button" role="switch" :aria-checked="config.enabled" :title="config.enabled ? 'Desligar' : 'Ligar'" class="relative h-6 w-10 shrink-0 rounded-full transition-colors" :class="config.enabled ? 'bg-violet-500' : 'bg-zinc-700'" @click="emit('toggle', !config.enabled)">
          <span class="absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-white shadow transition-transform" :class="config.enabled ? 'translate-x-4' : 'translate-x-0'"></span>
        </button>
      </div>
    </div>

    <div v-if="state.help || cliError || (det && !det.installed) || update?.available || det?.issues.length || (config.kind === 'agy' && det?.installed)" class="space-y-3 border-t border-zinc-800 px-5 py-4 text-xs">
      <p v-if="state.help" class="text-sm text-zinc-300">{{ state.help }}</p>
      <p v-if="cliError" class="text-red-300">{{ cliError }}</p>
      <!-- Nao instalado: instalador oficial (o mesmo do site), sem perguntas. -->
      <div v-if="det && !det.installed" class="flex items-center gap-3">
        <button class="btn-primary" :disabled="installing || busy" @click="emit('cliInstall')"><Download :size="14" /> {{ installing ? "Instalando…" : installedNow ? "Instalar de novo" : "Instalar" }}</button>
        <span v-if="installedNow" class="text-emerald-300">Instalado. Para entrar na conta, rode <code class="font-mono text-zinc-300">{{ PROVIDER_LOGIN[config.kind] }}</code> no terminal e clique em Verificar de novo.</span>
        <span v-else class="text-zinc-500">Baixa e instala pelo instalador oficial em ~/.local/bin (pode levar alguns minutos).</span>
      </div>
      <!-- Versao nova: o `update` do proprio CLI. -->
      <div v-else-if="update?.available" class="flex items-center gap-3">
        <button class="btn-secondary" :disabled="updating || busy" @click="emit('cliUpdate')"><Download :size="14" /> {{ updating ? "Atualizando…" : `Atualizar para v${String(update.latest).replace(/^v/i, "")}` }}</button>
        <span class="text-zinc-500">Roda o comando de atualização do próprio assistente.</span>
      </div>
      <p v-if="det?.issues.length" class="text-amber-200">{{ det.issues.join(" · ") }}</p>
      <div v-if="config.kind === 'agy' && det?.installed" class="rounded-lg border border-zinc-800 bg-zinc-950/60 p-3">
        <p class="text-zinc-300">O Antigravity precisa de uma integração única para que os DOTs usem as ferramentas do DOTS. Ela altera a configuração global do Antigravity (com cópia de segurança) e pode ser removida a qualquer momento.</p>
        <div class="mt-2 flex items-center gap-2">
          <button class="btn-primary" :disabled="agyBusy" @click="emit('agySetup', false)">{{ agy?.configured ? "Reaplicar integração" : "Integrar ao DOTS" }}</button>
          <button v-if="agy?.server_registered || agy?.rules_present.length" class="btn-secondary" :disabled="agyBusy" @click="emit('agySetup', true)">Remover integração</button>
          <span v-if="agy?.configured" class="text-emerald-300">integrado</span>
        </div>
      </div>
    </div>

    <div v-if="det?.installed" class="grid gap-6 border-t border-zinc-800 px-5 py-4" :class="[usage ? 'grid-cols-2' : 'grid-cols-1', off ? 'opacity-60' : '']">
      <!-- Uso da assinatura (Claude e ChatGPT): janela semanal e de 5 horas. -->
      <div v-if="usage" class="min-w-0 text-xs">
        <div class="flex items-center gap-2 text-zinc-400"><span>Uso da assinatura</span><span v-if="usage.plan" class="rounded-md bg-zinc-800 px-2 py-0.5 text-[11px] font-medium text-zinc-200" :class="usage.plan.includes('@') ? '' : 'capitalize'">{{ usage.plan }}</span></div>
        <p v-if="usage.error" class="mt-2 text-red-300">{{ usage.error }}</p>
        <div v-for="w in windows" :key="w[0]" class="mt-3">
          <div class="flex items-center justify-between gap-2">
            <span class="text-zinc-300">{{ w[0] }}</span>
            <span v-if="w[1]" class="font-mono text-[11px] text-zinc-500"><strong class="font-semibold text-zinc-100">{{ Math.round(w[1].pct) }}%</strong> {{ resetsIn(w[1]) }}</span>
            <span v-else class="font-mono text-[11px] text-zinc-500">sem dado</span>
          </div>
          <div class="mt-1.5 h-1 overflow-hidden rounded-full bg-zinc-800"><div v-if="w[1]" class="h-full rounded-full" :class="pctTone(w[1].pct)" :style="{ width: `${Math.max(1, w[1].pct)}%` }" /></div>
        </div>
        <p v-if="config.kind === 'agy'" class="mt-2 text-zinc-500">O Antigravity não informa limites; a conta conectada aparece acima.</p>
      </div>

      <div class="min-w-0">
        <label class="mb-1.5 block text-xs text-zinc-400" :for="`model-${config.kind}`">Modelo <span class="text-zinc-600">(opcional)</span></label>
        <div class="flex items-center gap-2">
          <div class="min-w-0 flex-1"><ModelPicker :id="`model-${config.kind}`" v-model="model" :engine="config.kind" empty-label="Modelo padrão do assistente" /></div>
          <button class="h-[38px] shrink-0 rounded-lg border border-zinc-700 px-3 text-[13px] font-medium text-zinc-100 hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-50" :disabled="busy || !modelDirty" @click="emit('model', model || null)">Aplicar</button>
        </div>
        <p class="mt-2 text-xs leading-relaxed text-zinc-500">
          Vale para todos os DOTs que usam este assistente. Cada DOT pode escolher outro modelo em Avançado.
          <template v-if="config.kind !== 'claude'">{{ det.models?.length ? `A lista tem ${det.models.length} modelos informados pelo próprio assistente e é atualizada em Verificar de novo.` : "O assistente ainda não informou a lista de modelos: clique em Verificar de novo." }}</template>
        </p>
      </div>
    </div>

    <div class="flex items-start gap-4 border-t border-zinc-800 px-5 py-3 text-xs">
      <button class="shrink-0 font-medium text-violet-400 hover:text-violet-300 disabled:opacity-50" :disabled="busy" @click="emit('detect')">{{ busy ? "Verificando..." : "Verificar de novo" }}</button>
      <details class="min-w-0 flex-1">
        <summary class="cursor-pointer list-none text-zinc-500 hover:text-zinc-300 [&::-webkit-details-marker]:hidden">Detalhes técnicos</summary>
        <dl v-if="det" class="mt-2 grid grid-cols-[120px_1fr] gap-y-1">
          <dt class="text-zinc-500">Executável</dt><dd class="truncate font-mono text-zinc-300">{{ det.executable_path ?? "—" }}</dd>
          <dt class="text-zinc-500">Login</dt><dd class="text-zinc-300">{{ det.auth_detail ?? "—" }}</dd>
          <dt class="text-zinc-500">Verificado</dt><dd class="text-zinc-300">{{ new Date(det.checked_at).toLocaleString() }}</dd>
          <dt v-if="agy" class="text-zinc-500">Config. AGY</dt><dd v-if="agy" class="truncate font-mono text-zinc-300">{{ agy.settings_path }}</dd>
        </dl>
        <ul v-if="det?.capabilities.length" class="mt-2 space-y-0.5"><li v-for="c in det.capabilities" :key="c.id" class="text-zinc-400">{{ c.label }} <code class="text-zinc-600">{{ c.evidence }}</code></li></ul>
        <div class="mt-2 flex items-end gap-2">
          <div class="flex-1"><label class="label">Caminho do executável (manual)</label><input v-model="overridePath" class="input font-mono text-xs" placeholder="/caminho/para/executavel" /></div>
          <button class="btn-secondary" :disabled="busy" @click="emit('override', overridePath || null)">Aplicar</button>
        </div>
      </details>
    </div>
  </div>
</template>
