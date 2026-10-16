import { defineStore } from "pinia";
import type { CliUpdateInfo, CliUpdates, EngineKind, ProviderUsage, UsageInfo } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

/** Nomes curtos das assinaturas (como no ovrgit). */
export const PROVIDER_NAME: Record<EngineKind, string> = { claude: "Claude", codex: "ChatGPT", agy: "Antigravity" };
/** Comando que abre o CLI para entrar na conta pela primeira vez. */
export const PROVIDER_LOGIN: Record<EngineKind, string> = { claude: "claude", codex: "codex login", agy: "agy" };
export const PROVIDERS: EngineKind[] = ["claude", "codex", "agy"];

const msg = (e: unknown) => (e instanceof RuntimeError ? e.message : String(e));

/**
 * Uso das assinaturas (Claude, ChatGPT, Antigravity) e versoes dos CLIs, com instalar/atualizar pelo app.
 * O runtime consulta e guarda em cache; aqui so o estado da interface (quem esta atualizando, erros, etc.).
 */
export const useUsageStore = defineStore("usage", {
  state: () => ({
    usage: null as UsageInfo | null,
    updates: null as CliUpdates | null,
    loading: false,
    /** CLI sendo atualizado / instalado agora. */
    updating: null as EngineKind | null,
    installing: null as EngineKind | null,
    /** Atualizado nesta sessao (versao) e instalado nesta sessao (falta entrar na conta). */
    updated: {} as Partial<Record<EngineKind, string>>,
    installed: {} as Partial<Record<EngineKind, boolean>>,
    errors: {} as Partial<Record<EngineKind, string>>,
  }),
  getters: {
    rows: (s) => {
      const u = s.usage;
      if (!u) return [] as { id: EngineKind; name: string; u: ProviderUsage }[];
      return PROVIDERS.filter((id) => u[id]).map((id) => ({ id, name: PROVIDER_NAME[id], u: u[id]! }));
    },
    /** CLIs que nao estao nesta maquina (a consulta de versoes respondeu e nao achou o executavel), mais os
     * instalados agora e ainda sem uso: aparecem com o botao de instalar ou o aviso de login. */
    missing(s): EngineKind[] {
      const rows = this.rows as { id: EngineKind }[];
      return PROVIDERS.filter((id) => s.updates && (s.updates[id] === null || s.installed[id]) && !rows.some((r) => r.id === id));
    },
    visible(): boolean {
      return (this.rows as { u: ProviderUsage }[]).some((r) => r.u.week || r.u.five_hour) || (this.missing as EngineKind[]).length > 0;
    },
    hasUpdate: (s) => PROVIDERS.some((id) => s.updates?.[id]?.available),
    /** Uso semanal combinado: media das assinaturas com dado (100% das duas = 100%). */
    weekAvg(): number {
      const vals = (this.rows as { u: ProviderUsage }[]).map((r) => r.u.week?.pct).filter((p): p is number => typeof p === "number");
      return vals.length ? vals.reduce((a, b) => a + b, 0) / vals.length : 0;
    },
  },
  actions: {
    async load(force = false) {
      this.loading = true;
      try {
        const [usage, updates] = await Promise.allSettled([api.usage.get(force), api.usage.cliUpdates(force)]);
        if (usage.status === "fulfilled") this.usage = usage.value;
        if (updates.status === "fulfilled") this.updates = updates.value;
      } finally {
        this.loading = false;
      }
    },
    setUpdate(id: EngineKind, after: CliUpdateInfo | null) {
      if (this.updates) this.updates[id] = after;
    },
    async update(id: EngineKind) {
      if (this.updating) return;
      this.updating = id;
      delete this.errors[id];
      try {
        const after = await api.usage.cliUpdate(id);
        this.setUpdate(id, after);
        if (after && !after.available) this.updated[id] = after.current;
      } catch (e) {
        this.errors[id] = msg(e);
      } finally {
        this.updating = null;
      }
    },
    async install(id: EngineKind) {
      if (this.installing || this.updating) return;
      this.installing = id;
      delete this.errors[id];
      try {
        const after = await api.usage.cliInstall(id);
        this.setUpdate(id, after);
        this.installed[id] = true;
      } catch (e) {
        this.errors[id] = msg(e);
      } finally {
        this.installing = null;
      }
    },
  },
});

export function resetsIn(w: { resets_at: string | null } | null | undefined, now = Date.now()): string {
  if (!w?.resets_at) return "";
  const ms = Date.parse(w.resets_at) - now;
  if (!Number.isFinite(ms)) return "";
  if (ms <= 0) return "zera em instantes";
  const h = Math.floor(ms / 3600_000);
  const m = Math.round((ms % 3600_000) / 60_000);
  if (h >= 24) return `zera em ${Math.floor(h / 24)} d ${h % 24} h`;
  return h ? `zera em ${h} h ${m} min` : `zera em ${m} min`;
}

export function ago(at: string, now = Date.now()): string {
  const m = Math.round((now - Date.parse(at)) / 60_000);
  return m < 1 ? "agora" : m < 60 ? `ha ${m} min` : `ha ${Math.floor(m / 60)} h`;
}
