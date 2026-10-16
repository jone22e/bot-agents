<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { Activity, Bot, Database, FolderOpen, HardDriveDownload, Palette, Power, RefreshCw, Server, Settings2, Users, Wrench } from "lucide-vue-next";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import EngineCard from "@/components/EngineCard.vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import PageHeader from "@/components/PageHeader.vue";
import Toggle from "@/components/Toggle.vue";
import { api, RuntimeError } from "@/api";
import { useEnginesStore } from "@/stores/engines";
import { useOpsStore } from "@/stores/ops";
import { useRuntimeStore } from "@/stores/runtime";
import { useSettingsStore } from "@/stores/settings";
import { useUsageStore } from "@/stores/usage";
import { useUpdaterStore } from "@/stores/updater";
import { THEME_SETTING, applyTheme, currentTheme } from "@/lib/theme";
import { THEMES } from "@/lib/themes";

const runtime = useRuntimeStore();
const settings = useSettingsStore();
const engines = useEnginesStore();
const ops = useOpsStore();
const backups = ref<string[]>([]);
const backupError = ref<string | null>(null);
const backupBusy = ref(false);
const confirmStop = ref(false);
const section = ref<"geral" | "servico" | "assistentes" | "backups" | "diagnostico">("geral");

const nav = [
  { id: "geral", label: "Geral", icon: Settings2 },
  { id: "servico", label: "Servico em segundo plano", icon: Server },
  { id: "assistentes", label: "Assistentes de IA", icon: Bot },
  { id: "backups", label: "Copias de seguranca", icon: Database },
  { id: "diagnostico", label: "Diagnostico", icon: Wrench },
] as const;

async function loadBackups() {
  try { backups.value = await api.runtime.backups(); backupError.value = null; }
  catch (e) { backupError.value = e instanceof RuntimeError ? e.message : String(e); }
}
async function createBackup() {
  backupBusy.value = true;
  try { await api.runtime.backup(); await loadBackups(); }
  catch (e) { backupError.value = e instanceof RuntimeError ? e.message : String(e); }
  finally { backupBusy.value = false; }
}
onMounted(async () => {
  await runtime.refresh();
  if (runtime.isConnected) {
    await Promise.allSettled([loadBackups(), engines.load(), ops.load()]);
    if (engines.byKind("agy")?.last_detection?.installed) await engines.agyStatus();
    await usage.load();
  }
});
function uptime(s: number) { const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60); return h > 0 ? `${h} h ${m} min` : `${m} min`; }
function fileName(p: string) { return p.split("/").pop() ?? p; }
const paused = computed(() => ops.stats?.paused ?? false);
async function setPaused(v: boolean) { await api.runtime.pauseAll(v); await ops.load(); }
const readyEngines = computed(() => engines.items.filter((e) => e.last_detection?.installed));
const usage = useUsageStore();
const updater = useUpdaterStore();
const MAX_CONCURRENT = "runtime.max_concurrent_executions";
const maxConcurrent = computed(() => Number(settings.items[MAX_CONCURRENT] ?? 4));
const concurrencyError = ref<string | null>(null);
const concurrencyRestartNeeded = ref(false);
async function setMaxConcurrent(el: HTMLInputElement) {
  const n = Number(el.value);
  if (!Number.isInteger(n) || n < 1 || n > 8) { el.value = String(maxConcurrent.value); return; }
  if (n === maxConcurrent.value) return;
  try {
    await settings.set(MAX_CONCURRENT, n);
    concurrencyError.value = null;
    concurrencyRestartNeeded.value = true;
  } catch (e) {
    el.value = String(maxConcurrent.value);
    concurrencyError.value = e instanceof Error ? e.message : String(e);
  }
}
/** Limite de funcionarios que cada gerente pode contratar (o runtime usa 5 sem configuracao; aceita de 1 a 50). */
const MAX_EMPLOYEES = "teams.max_employees_per_manager";
const maxEmployees = computed(() => Number(settings.items[MAX_EMPLOYEES] ?? 5));
async function setMaxEmployees(el: HTMLInputElement) {
  const n = Math.round(Number(el.value));
  if (!Number.isFinite(n) || n < 1 || n > 50) { el.value = String(maxEmployees.value); return; }
  await settings.set(MAX_EMPLOYEES, n);
}
/** Tema (skin): aplica na hora e guarda no servico; se o servico estiver parado, vale so nesta sessao. */
const theme = ref(currentTheme().id);
async function pickTheme(id: string) {
  theme.value = id;
  applyTheme(id);
  if (runtime.isConnected) await settings.set(THEME_SETTING, id);
}
</script>

<template>
  <div class="flex h-full flex-col">
    <PageHeader title="Configuracoes" subtitle="Servico, assistentes de IA, copias de seguranca e diagnostico" />
    <div class="flex min-h-0 flex-1">
      <nav class="w-60 shrink-0 space-y-0.5 px-4">
        <button v-for="n in nav" :key="n.id" class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm" :class="section === n.id ? 'bg-zinc-800 text-zinc-50' : 'text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200'" @click="section = n.id">
          <component :is="n.icon" :size="16" class="shrink-0" />
          <span>{{ n.label }}</span>
        </button>
      </nav>

      <div class="min-w-0 flex-1 overflow-y-auto px-6 pb-8">
        <ErrorBanner :message="runtime.error" />

        <!-- Geral -->
        <div v-if="section === 'geral'" class="max-w-2xl space-y-4">
          <section class="card space-y-4">
            <div class="flex items-center gap-2 text-sm font-semibold text-zinc-200"><Activity :size="16" /> Trabalho dos DOTs</div>
            <Toggle :model-value="!paused" label="Permitir que os DOTs trabalhem" description="Desligue para pausar toda a fila. Nada e perdido; os trabalhos esperam ate religar." :disabled="!runtime.isConnected" @update:model-value="(v) => setPaused(!v)" />
            <label class="flex items-center justify-between gap-4">
              <span>
                <span class="block text-sm text-zinc-200">Trabalhos simultâneos</span>
                <span class="block text-xs text-zinc-500">Quantas tarefas podem ser executadas ao mesmo tempo, somando todos os DOTs. Padrão: 4.</span>
              </span>
              <input type="number" min="1" max="8" step="1" class="input w-20 text-center" :value="maxConcurrent" :disabled="!runtime.isConnected" @change="(e) => setMaxConcurrent(e.target as HTMLInputElement)" />
            </label>
            <p v-if="concurrencyRestartNeeded" class="text-xs text-amber-300">Limite salvo. Reinicie em Serviço em segundo plano quando os trabalhos atuais terminarem para aplicar.</p>
            <ErrorBanner :message="concurrencyError" />
            <Toggle v-if="runtime.paths && !runtime.paths.dev_mode" :model-value="runtime.paths.start_at_login" label="Iniciar com o Mac" description="Gatilhos e horarios funcionam mesmo sem abrir o app." @update:model-value="(v) => runtime.setStartAtLogin(v)" />
          </section>
          <section class="card space-y-4">
            <div class="flex items-center gap-2 text-sm font-semibold text-zinc-200"><HardDriveDownload :size="16" /> Atualização</div>
            <div class="flex items-center justify-between gap-4">
              <div class="min-w-0">
                <p class="text-sm text-zinc-100">DOTS {{ updater.state?.current }}</p>
                <p class="text-xs" :class="updater.state?.status === 'error' ? 'text-red-300' : 'text-zinc-500'">{{ updater.text }}</p>
              </div>
              <button v-if="updater.state?.status === 'ready'" class="btn-primary shrink-0" @click="updater.install()">Reiniciar e atualizar</button>
              <button v-else-if="updater.state?.status === 'available'" class="btn-secondary shrink-0" @click="updater.download()">Baixar</button>
              <button v-else class="btn-secondary shrink-0" :disabled="!updater.state || ['unsupported', 'checking', 'downloading'].includes(updater.state.status)" @click="updater.check()">Verificar agora</button>
            </div>
            <Toggle :model-value="updater.state?.auto ?? true" label="Atualizar automaticamente" description="Procura versões novas ao abrir o app, a cada 30 minutos e ao voltar a ele, e baixa em segundo plano. A instalação acontece ao reiniciar." :disabled="!updater.state || updater.state.status === 'unsupported'" @update:model-value="(v) => updater.setAuto(v)" />
          </section>
          <section class="card space-y-3">
            <div class="flex items-center gap-2 text-sm font-semibold text-zinc-200"><Users :size="16" /> Equipes</div>
            <label class="flex items-center justify-between gap-4">
              <span>
                <span class="block text-sm text-zinc-200">Limite de funcionarios por gerente</span>
                <span class="block text-xs text-zinc-500">O gerente nunca faz o trabalho: contrata funcionarios (cria nome e funcao) ate este limite. Para contratar alem dele, precisa demitir alguem.</span>
              </span>
              <input type="number" min="1" max="50" class="input w-20 text-center" :value="maxEmployees" :disabled="!runtime.isConnected" @change="(e) => setMaxEmployees(e.target as HTMLInputElement)" />
            </label>
          </section>
          <section class="card space-y-3">
            <div class="flex items-center gap-2 text-sm font-semibold text-zinc-200"><Palette :size="16" /> Aparencia</div>
            <p class="text-xs text-zinc-500">Temas do ovrgit. O padrao e o escuro do DOTS; os demais fixam a paleta de toda a interface.</p>
            <div class="themes">
              <button v-for="t in THEMES" :key="t.id" type="button" class="theme" :class="{ active: theme === t.id }" :title="t.name" @click="pickTheme(t.id)">
                <span v-if="t.colors" class="swatch" :style="{ background: t.colors.bg, borderColor: t.colors.border }">
                  <span class="bar" :style="{ background: t.colors.panel }"><span class="pill" :style="{ background: t.colors.accent }" /></span>
                  <span class="dots"><i :style="{ background: t.colors.add }" /><i :style="{ background: t.colors.del }" /><i :style="{ background: t.colors.mod }" /><i :style="{ background: t.colors.hunk }" /></span>
                </span>
                <span v-else class="swatch" style="background: #09090b; border-color: #27272a">
                  <span class="bar" style="background: #18181b"><span class="pill" style="background: #4f46e5" /></span>
                  <span class="dots"><i style="background: #10b981" /><i style="background: #ef4444" /><i style="background: #f59e0b" /><i style="background: #0ea5e9" /></span>
                </span>
                <span class="theme-name">{{ t.name }}</span>
              </button>
            </div>
          </section>
        </div>

        <!-- Servico -->
        <div v-else-if="section === 'servico'" class="max-w-2xl space-y-4">
          <section class="card">
            <div class="flex items-start justify-between gap-4">
              <div class="flex items-start gap-3">
                <span class="mt-0.5 flex h-9 w-9 items-center justify-center rounded-full" :class="runtime.isConnected ? 'bg-emerald-500/15 text-emerald-300' : 'bg-zinc-800 text-zinc-400'"><Power :size="18" /></span>
                <div>
                  <p class="text-sm font-semibold text-zinc-100">{{ runtime.isConnected ? "Servico ativo" : "Servico parado" }}</p>
                  <p class="text-xs text-zinc-500">{{ runtime.isConnected ? `Ha ${uptime(runtime.status?.uptime_seconds ?? 0)}. Guarda seus dados e faz os DOTs trabalharem, mesmo com a janela fechada.` : "Os DOTs nao trabalham e o app nao mostra dados ate ligar." }}</p>
                </div>
              </div>
              <div class="flex gap-2">
                <button v-if="runtime.isConnected" class="btn-secondary" :disabled="runtime.busy" @click="confirmStop = true">Parar</button>
                <button v-else class="btn-primary" :disabled="runtime.busy" @click="runtime.ensure()">Ligar</button>
                <button class="btn-ghost" :disabled="runtime.busy" title="Reiniciar" @click="runtime.restart()"><RefreshCw :size="16" /></button>
              </div>
            </div>
          </section>
          <section v-if="ops.stats" class="grid grid-cols-4 gap-3">
            <div class="card"><p class="text-xs text-zinc-500">Trabalhando</p><p class="text-xl font-semibold text-zinc-50">{{ ops.stats.running }}</p></div>
            <div class="card"><p class="text-xs text-zinc-500">Na fila</p><p class="text-xl font-semibold text-zinc-50">{{ ops.stats.queued }}</p></div>
            <div class="card"><p class="text-xs text-zinc-500">Esperando voce</p><p class="text-xl font-semibold" :class="ops.stats.approvals_pending + ops.stats.questions_pending > 0 ? 'text-amber-300' : 'text-zinc-50'">{{ ops.stats.approvals_pending + ops.stats.questions_pending }}</p></div>
            <div class="card"><p class="text-xs text-zinc-500">Falhas (24 h)</p><p class="text-xl font-semibold text-zinc-50">{{ ops.stats.failures_24h }}</p></div>
          </section>
        </div>

        <!-- Assistentes -->
        <div v-else-if="section === 'assistentes'" class="max-w-4xl space-y-4">
          <div class="flex items-center justify-between">
            <p class="text-[13px] text-zinc-400">Assistentes de IA já instalados no seu Mac que os DOTs podem usar.</p>
            <button class="rounded-lg bg-violet-500 px-4 py-2 text-sm font-semibold text-white hover:bg-violet-400 disabled:cursor-not-allowed disabled:opacity-50" :disabled="!runtime.isConnected || engines.detecting !== null" @click="engines.detect()">{{ engines.detecting === "all" ? "Verificando..." : "Verificar todos" }}</button>
          </div>
          <ErrorBanner :message="engines.error" />
          <EngineCard
            v-for="cfg in engines.items"
            :key="cfg.kind"
            :config="cfg"
            :busy="engines.detecting === cfg.kind || engines.detecting === 'all'"
            @detect="engines.detect(cfg.kind)"
            @override="(p) => engines.setOverride(cfg.kind, p)"
            @toggle="(v) => engines.setEnabled(cfg.kind, v)"
            @model="(m) => engines.setModel(cfg.kind, m)"
            :agy="cfg.kind === 'agy' ? engines.agy : null"
            :agy-busy="engines.agyBusy"
            @agy-setup="(remove) => engines.agySetup(remove)"
            :usage="usage.usage?.[cfg.kind] ?? null"
            :update="usage.updates?.[cfg.kind] ?? null"
            :updating="usage.updating === cfg.kind"
            :installing="usage.installing === cfg.kind"
            :cli-error="usage.errors[cfg.kind] ?? null"
            :installed-now="usage.installed[cfg.kind] ?? false"
            :updated-now="usage.updated[cfg.kind] ?? null"
            @cli-update="usage.update(cfg.kind)"
            @cli-install="usage.install(cfg.kind)"
          />
          <p v-if="engines.items.length && !readyEngines.length" class="text-xs text-amber-300">Nenhum assistente instalado. Os DOTs precisam de pelo menos um.</p>
        </div>

        <!-- Backups -->
        <div v-else-if="section === 'backups'" class="max-w-2xl space-y-4">
          <section class="card">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2 text-sm font-semibold text-zinc-200"><HardDriveDownload :size="16" /> Copias de seguranca</div>
              <button class="btn-primary" :disabled="!runtime.isConnected || backupBusy" @click="createBackup">{{ backupBusy ? "Copiando..." : "Fazer copia agora" }}</button>
            </div>
            <p class="mt-1 text-xs text-zinc-500">Uma copia automatica e feita antes de cada atualizacao. As 10 mais recentes sao mantidas.</p>
            <ErrorBanner :message="backupError" />
            <ul v-if="backups.length" class="mt-3 divide-y divide-zinc-800 text-sm">
              <li v-for="b in backups.slice().reverse()" :key="b" class="flex items-center justify-between py-2">
                <span class="flex items-center gap-2 text-zinc-300"><Database :size="14" class="text-zinc-500" /> {{ fileName(b) }}</span>
                <button class="btn-ghost" title="Mostrar no Finder" @click="revealItemInDir(b)"><FolderOpen :size="16" /></button>
              </li>
            </ul>
            <p v-else class="mt-3 text-xs text-zinc-500">Nenhuma copia ainda.</p>
          </section>
        </div>

        <!-- Diagnostico -->
        <div v-else class="max-w-4xl space-y-4">
          <section class="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/60">
            <h3 class="px-5 py-4 text-sm font-semibold text-zinc-100">Versões e caminhos</h3>
            <dl class="divide-y divide-zinc-800 border-t border-zinc-800 text-[13px]">
              <template v-if="runtime.status">
                <div class="diag-row"><dt>Serviço</dt><dd>{{ runtime.status.version }} · banco v{{ runtime.status.schema_version }} · pid {{ runtime.status.pid }} · {{ runtime.status.mode }}</dd></div>
                <div class="diag-row"><dt>Socket</dt><dd :title="runtime.status.socket_path">{{ runtime.status.socket_path }}</dd></div>
              </template>
              <div v-if="runtime.launchd" class="diag-row">
                <dt>Início automático</dt>
                <dd><template v-if="!runtime.launchd.supported">não suportado</template><template v-else-if="runtime.launchd.loaded">ativo (launchd)</template><template v-else-if="runtime.launchd.plist_installed">instalado, inativo</template><template v-else>não instalado{{ runtime.paths?.dev_mode ? " (desenvolvimento)" : "" }}</template></dd>
              </div>
              <template v-if="runtime.paths">
                <div class="diag-row"><dt>Dados</dt><dd :title="runtime.paths.data_dir">{{ runtime.paths.data_dir }}</dd><button class="diag-open" @click="openPath(runtime.paths!.data_dir)">Abrir</button></div>
                <div class="diag-row"><dt>Logs</dt><dd :title="runtime.paths.logs_dir">{{ runtime.paths.logs_dir }}</dd><button class="diag-open" @click="openPath(runtime.paths!.logs_dir)">Abrir</button></div>
              </template>
              <template v-if="settings.installation">
                <div class="diag-row"><dt>Instalação</dt><dd>{{ settings.installation.id }}</dd></div>
                <div class="diag-row"><dt>Criada em</dt><dd>{{ new Date(settings.installation.created_at).toLocaleString() }}</dd></div>
              </template>
            </dl>
          </section>
          <section class="overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-900/60">
            <div class="flex items-center justify-between px-5 py-3">
              <h3 class="text-sm font-semibold text-zinc-100">Registro do serviço</h3>
              <button class="rounded-lg border border-zinc-700 px-3 py-1.5 text-[13px] font-medium text-zinc-100 hover:bg-zinc-800" @click="ops.loadLogs()">{{ ops.logs.length ? "Atualizar" : "Carregar" }}</button>
            </div>
            <pre class="h-44 overflow-auto bg-zinc-950/70 px-5 py-4 font-mono text-[11px] leading-relaxed" :class="ops.logs.length ? 'text-zinc-400' : 'text-zinc-600'">{{ ops.logs.join("\n") || "Clique em carregar para ver as últimas linhas." }}</pre>
          </section>
        </div>
      </div>
    </div>
    <ConfirmDialog :open="confirmStop" title="Parar o servico" message="Com o servico parado os DOTs nao trabalham e o app nao mostra seus dados ate ligar de novo. Nada e apagado." confirm-label="Parar" @confirm="confirmStop = false; runtime.stop()" @cancel="confirmStop = false" />
  </div>
</template>

<style scoped>
/* Grade de temas (copiada do ovrgit) */
.themes { display: grid; grid-template-columns: repeat(auto-fill, minmax(124px, 1fr)); gap: 8px; }
.theme { display: flex; flex-direction: column; align-items: stretch; gap: 6px; padding: 6px; border-radius: 8px; border: 1px solid var(--color-zinc-800); background: var(--color-zinc-900); font-weight: 500; font-size: 12px; color: var(--color-zinc-200); cursor: pointer; }
.theme:hover { background: var(--color-zinc-800); }
.theme.active { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent); }
.swatch { display: flex; flex-direction: column; justify-content: space-between; height: 46px; border-radius: 6px; border: 1px solid; overflow: hidden; }
.swatch .bar { height: 14px; display: flex; align-items: center; padding: 0 6px; }
.swatch .pill { width: 22px; height: 5px; border-radius: 3px; }
.swatch .dots { display: flex; gap: 4px; padding: 0 6px 7px; }
.swatch .dots i { width: 7px; height: 7px; border-radius: 50%; }
/* Linhas do diagnostico: rotulo, valor em fonte fixa e o "Abrir" a direita */
.diag-row { display: flex; align-items: center; gap: 16px; padding: 9px 20px; min-width: 0; }
.diag-row dt { width: 160px; flex: none; color: var(--color-zinc-500); }
.diag-row dd { flex: 1 1 auto; min-width: 0; margin: 0; font-family: var(--mono); font-size: 12.5px; color: var(--color-zinc-200); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.diag-open { flex: none; font-size: 12px; font-weight: 500; color: var(--color-violet-400); }
.diag-open:hover { color: var(--color-violet-300); }
.theme-name { text-align: center; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
