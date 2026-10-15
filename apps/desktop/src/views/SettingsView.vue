<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { Activity, Bot, Database, FolderOpen, HardDriveDownload, Info, Power, RefreshCw, Server, Settings2, ShieldCheck, Terminal, Wrench } from "lucide-vue-next";
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
  }
});
function uptime(s: number) { const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60); return h > 0 ? `${h} h ${m} min` : `${m} min`; }
function fileName(p: string) { return p.split("/").pop() ?? p; }
const paused = computed(() => ops.stats?.paused ?? false);
async function setPaused(v: boolean) { await api.runtime.pauseAll(v); await ops.load(); }
const readyEngines = computed(() => engines.items.filter((e) => e.last_detection?.installed));
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
            <Toggle v-if="runtime.paths && !runtime.paths.dev_mode" :model-value="runtime.paths.start_at_login" label="Iniciar com o Mac" description="Gatilhos e horarios funcionam mesmo sem abrir o app." @update:model-value="(v) => runtime.setStartAtLogin(v)" />
          </section>
          <section class="card space-y-3">
            <div class="flex items-center gap-2 text-sm font-semibold text-zinc-200"><ShieldCheck :size="16" /> Como as permissoes funcionam</div>
            <ul class="space-y-1.5 text-sm text-zinc-300">
              <li>· Cada DOT so acessa as pastas que voce liberou para ele.</li>
              <li>· Acoes acima do nivel permitido aparecem na conversa para voce Permitir ou Negar. Sem resposta em 10 minutos, a acao nao acontece.</li>
              <li>· Comandos perigosos (apagar tudo, sudo, baixar e executar) sao sempre bloqueados.</li>
              <li>· Tudo fica registrado e pode ser visto em Atividades.</li>
            </ul>
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
        <div v-else-if="section === 'assistentes'" class="max-w-2xl space-y-4">
          <div class="flex items-center justify-between">
            <p class="text-xs text-zinc-500">Assistentes de IA ja instalados no seu Mac que os DOTs podem usar.</p>
            <button class="btn-primary" :disabled="!runtime.isConnected || engines.detecting !== null" @click="engines.detect()">{{ engines.detecting === "all" ? "Verificando..." : "Verificar todos" }}</button>
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
        <div v-else class="max-w-2xl space-y-4">
          <section class="card">
            <div class="flex items-center gap-2 text-sm font-semibold text-zinc-200"><Info :size="16" /> Versoes e caminhos</div>
            <dl class="mt-3 grid grid-cols-[200px_1fr] gap-y-2 text-xs">
              <template v-if="runtime.status">
                <dt class="text-zinc-500">Servico</dt><dd class="text-zinc-300">{{ runtime.status.version }} · banco v{{ runtime.status.schema_version }} · pid {{ runtime.status.pid }} · {{ runtime.status.mode }}</dd>
                <dt class="text-zinc-500">Socket</dt><dd class="truncate font-mono text-zinc-300">{{ runtime.status.socket_path }}</dd>
              </template>
              <template v-if="runtime.launchd">
                <dt class="text-zinc-500">Inicio automatico</dt>
                <dd class="text-zinc-300"><template v-if="!runtime.launchd.supported">nao suportado</template><template v-else-if="runtime.launchd.loaded">ativo (launchd)</template><template v-else-if="runtime.launchd.plist_installed">instalado, inativo</template><template v-else>nao instalado{{ runtime.paths?.dev_mode ? " (desenvolvimento)" : "" }}</template></dd>
              </template>
              <template v-if="runtime.paths">
                <dt class="text-zinc-500">Dados</dt><dd class="flex items-center gap-2 font-mono text-zinc-300"><span class="truncate">{{ runtime.paths.data_dir }}</span><button class="text-indigo-400 hover:underline" @click="openPath(runtime.paths!.data_dir)">abrir</button></dd>
                <dt class="text-zinc-500">Logs</dt><dd class="flex items-center gap-2 font-mono text-zinc-300"><span class="truncate">{{ runtime.paths.logs_dir }}</span><button class="text-indigo-400 hover:underline" @click="openPath(runtime.paths!.logs_dir)">abrir</button></dd>
              </template>
              <template v-if="settings.installation">
                <dt class="text-zinc-500">Instalacao</dt><dd class="font-mono text-zinc-300">{{ settings.installation.id }}</dd>
                <dt class="text-zinc-500">Criada em</dt><dd class="text-zinc-300">{{ new Date(settings.installation.created_at).toLocaleString() }}</dd>
              </template>
            </dl>
          </section>
          <section class="card">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2 text-sm font-semibold text-zinc-200"><Terminal :size="16" /> Registro do servico</div>
              <button class="btn-ghost" @click="ops.loadLogs()"><RefreshCw :size="14" /></button>
            </div>
            <pre class="mt-2 max-h-72 overflow-auto rounded bg-zinc-950 p-2 font-mono text-[10px] text-zinc-400">{{ ops.logs.join("\n") || "Clique em atualizar para carregar." }}</pre>
          </section>
        </div>
      </div>
    </div>
    <ConfirmDialog :open="confirmStop" title="Parar o servico" message="Com o servico parado os DOTs nao trabalham e o app nao mostra seus dados ate ligar de novo. Nada e apagado." confirm-label="Parar" @confirm="confirmStop = false; runtime.stop()" @cancel="confirmStop = false" />
  </div>
</template>
