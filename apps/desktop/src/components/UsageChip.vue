<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Download, RefreshCw } from "lucide-vue-next";
import type { UsageWindow } from "@dots/shared-types";
import EngineLogo from "@/components/EngineLogo.vue";
import { PROVIDER_LOGIN, PROVIDER_NAME, ago, resetsIn, useUsageStore } from "@/stores/usage";

/**
 * Consumo das assinaturas (copiado do ovrgit): um anel com a media semanal. Clique: detalhe por assinatura,
 * com a janela de 5 horas e quando cada limite zera, e se ha versao nova do CLI (com um botao que roda o
 * `update` do proprio CLI); CLIs ausentes aparecem com o botao de instalar.
 */
const props = defineProps<{ /** Para onde o painel abre (na barra de baixo do mensageiro, para cima). */ opens?: "up" | "down" }>();
const store = useUsageStore();
const open = ref(false);
const root = ref<HTMLElement>();
let timer: ReturnType<typeof setInterval> | undefined;

const tone = (pct: number) => (pct >= 90 ? "hot" : pct >= 70 ? "warm" : "");
/** Comprimento do anel (2πr, r = 8) */
const RING = 2 * Math.PI * 8;
const title = computed(() => `Uso semanal das assinaturas (media): ${store.rows.map((r) => `${r.name} ${Math.round(r.u.week?.pct ?? 0)}%`).join(" · ")}`);
/** A janela de 5 horas so aparece quando o provedor a informa (o ChatGPT Pro hoje so tem limite semanal). */
const windows = (u: { week: UsageWindow | null; five_hour: UsageWindow | null }): [string, UsageWindow | null][] => (u.five_hour ? [["Semana", u.week], ["5 horas", u.five_hour]] : [["Semana", u.week]]);

const onDoc = (e: MouseEvent) => {
  if (open.value && root.value && !root.value.contains(e.target as Node)) open.value = false;
};
function toggle() {
  open.value = !open.value;
  if (open.value) store.load(true);
}
onMounted(() => {
  store.load();
  timer = setInterval(() => store.load(), 5 * 60_000);
  document.addEventListener("mousedown", onDoc);
});
onUnmounted(() => {
  clearInterval(timer);
  document.removeEventListener("mousedown", onDoc);
});
</script>

<template>
  <div v-if="store.visible" ref="root" class="usage relative" @click.stop>
    <button type="button" class="chip flex h-9 items-center gap-1.5 rounded-full pl-1.5 pr-3 text-zinc-100 hover:bg-zinc-700" :class="open ? 'bg-zinc-700' : 'bg-zinc-800'" :title="title" @click="toggle">
      <svg class="gauge" :class="tone(store.weekAvg)" viewBox="0 0 20 20" aria-hidden="true">
        <circle class="gauge-bg" cx="10" cy="10" r="8" />
        <circle class="gauge-fill" cx="10" cy="10" r="8" :stroke-dasharray="`${(Math.max(0, Math.min(100, store.weekAvg)) / 100) * RING} ${RING}`" />
      </svg>
      <span class="text-xs font-bold tabular-nums text-zinc-300">{{ Math.round(store.weekAvg) }}%</span>
      <span v-if="store.hasUpdate" class="dot" title="Ha versao nova de um CLI" />
    </button>

    <div v-if="open" class="pop absolute z-40 w-[340px] rounded-xl border border-zinc-700 bg-zinc-900 px-1.5 pb-2 pt-1.5 shadow-2xl" :class="props.opens === 'down' ? 'top-[calc(100%+6px)] right-0' : 'bottom-[calc(100%+6px)] left-0'">
      <header class="flex items-center justify-between px-2 pb-1 pt-2 text-[13px]">
        <strong class="text-zinc-100">Uso das assinaturas</strong>
        <button type="button" class="flex h-[26px] w-[26px] items-center justify-center rounded-md text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200" title="Atualizar" @click="store.load(true)">
          <span v-if="store.loading" class="spinner" />
          <RefreshCw v-else :size="13" />
        </button>
      </header>
      <section v-for="r in store.rows" :key="r.id" class="flex flex-col gap-1.5 border-t border-zinc-800 p-2">
        <div class="flex items-center gap-2 text-[12.5px]">
          <EngineLogo :engine="r.id" :size="14" />
          <strong class="text-zinc-100">{{ r.name }}</strong>
          <span v-if="r.u.plan" class="badge max-w-[200px] truncate bg-zinc-800 capitalize text-zinc-300" style="height: 18px; font-size: 10px">{{ r.u.plan }}</span>
          <span class="flex-1" />
          <small class="text-[11px] text-zinc-500">{{ ago(r.u.at) }}</small>
        </div>
        <div v-if="store.updates?.[r.id]" class="flex items-center gap-2 text-[11px]">
          <small class="font-mono text-zinc-500" :title="store.updates[r.id]!.latest ? `Mais recente: ${store.updates[r.id]!.latest}` : 'Nao foi possivel consultar a versao mais recente'">v{{ store.updates[r.id]!.current }}</small>
          <template v-if="store.updates[r.id]!.available">
            <small class="font-semibold text-indigo-400">nova: v{{ store.updates[r.id]!.latest }}</small>
            <span class="flex-1" />
            <button type="button" class="upd" :disabled="!!store.updating" @click="store.update(r.id)">
              <span v-if="store.updating === r.id" class="spinner" />
              <Download v-else :size="12" />
              {{ store.updating === r.id ? "Atualizando…" : "Atualizar" }}
            </button>
          </template>
          <small v-else-if="store.updated[r.id]" class="text-emerald-300">atualizado</small>
          <small v-else-if="store.updates[r.id]!.latest" class="text-zinc-500">em dia</small>
        </div>
        <p v-if="store.errors[r.id]" class="m-0 text-xs text-red-300">{{ store.errors[r.id] }}</p>
        <p v-if="r.u.error" class="m-0 text-xs text-red-300">{{ r.u.error }}</p>
        <template v-else>
          <div v-for="w in windows(r.u)" :key="w[0]" class="line">
            <span class="text-zinc-400">{{ w[0] }}</span>
            <template v-if="w[1]">
              <span class="track" :class="tone(w[1].pct)"><span class="fill" :class="r.id" :style="{ width: `${Math.max(1, w[1].pct)}%` }" /></span>
              <span class="text-right font-mono text-xs tabular-nums text-zinc-100">{{ Math.round(w[1].pct) }}%</span>
              <small class="whitespace-nowrap text-[11px] text-zinc-500">{{ resetsIn(w[1]) }}</small>
            </template>
            <small v-else class="text-zinc-500">sem dado</small>
          </div>
        </template>
      </section>
      <section v-for="id in store.missing" :key="id" class="flex flex-col gap-1.5 border-t border-zinc-800 p-2">
        <div class="flex items-center gap-2 text-[12.5px]">
          <EngineLogo :engine="id" :size="14" />
          <strong class="text-zinc-100">{{ PROVIDER_NAME[id] }}</strong>
          <span class="flex-1" />
          <small v-if="store.installed[id]" class="text-[11px] text-emerald-300">instalado</small>
          <button v-else type="button" class="upd" :disabled="!!store.installing || !!store.updating" :title="`Instala o CLI de ${PROVIDER_NAME[id]} pelo instalador oficial`" @click="store.install(id)">
            <span v-if="store.installing === id" class="spinner" />
            <Download v-else :size="12" />
            {{ store.installing === id ? "Instalando…" : "Instalar" }}
          </button>
        </div>
        <small v-if="store.installed[id]" class="text-[11.5px] text-zinc-500">Para entrar na conta, rode <code class="font-mono text-[11px] text-zinc-300">{{ PROVIDER_LOGIN[id] }}</code> no terminal.</small>
        <small v-else class="text-[11.5px] text-zinc-500">Nao instalado nesta maquina.</small>
        <p v-if="store.errors[id]" class="m-0 text-xs text-red-300">{{ store.errors[id] }}</p>
      </section>
    </div>
  </div>
</template>

<style scoped>
/* "gauge" e nao "ring": `ring` e um utilitario do Tailwind (contorno quadrado) */
.gauge { width: 20px; height: 20px; transform: rotate(-90deg); flex: none; }
.gauge circle { fill: none; stroke-width: 3; }
.gauge-bg { stroke: var(--color-zinc-700); }
.gauge-fill { stroke: var(--accent); stroke-linecap: round; transition: stroke-dasharray 0.3s; }
.gauge.warm .gauge-fill { stroke: var(--mod); }
.gauge.hot .gauge-fill { stroke: var(--del); }
.dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); margin-left: -2px; }
.pop { animation: drop 0.12s ease-out; }
@keyframes drop { from { opacity: 0; transform: translateY(4px); } }
.line { display: grid; grid-template-columns: 56px 1fr 40px auto; align-items: center; gap: 8px; font-size: 12px; }
.track { height: 6px; border-radius: 3px; background: var(--panel-2); overflow: hidden; }
.fill { display: block; height: 100%; border-radius: inherit; background: var(--accent); transition: width 0.3s; }
.fill.claude { background: #d97757; }
.fill.codex { background: var(--text); opacity: 0.75; }
.track.warm .fill { background: var(--mod); opacity: 1; }
.track.hot .fill { background: var(--del); opacity: 1; }
.upd { display: inline-flex; align-items: center; gap: 5px; height: 22px; padding: 0 8px; font-size: 11.5px; border-radius: 6px; background: var(--panel-2); color: var(--text); border: 1px solid var(--border); }
.upd:hover:not(:disabled) { background: var(--hover); }
.upd:disabled { opacity: 0.5; }
.spinner { width: 12px; height: 12px; border-radius: 50%; border: 2px solid var(--border); border-top-color: var(--accent); animation: spin 0.8s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
