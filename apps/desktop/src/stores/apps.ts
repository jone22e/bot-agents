import { defineStore } from "pinia";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AppConnection, FlexiConnectInput, FlexiRoleTemplate } from "@dots/shared-types";
import { api, RuntimeError, type FlexiBrowserLogin } from "@/api";
import { useSettingsStore } from "@/stores/settings";

/** Conexao Flexi da conta do proprio usuario (vinculada pelo "Logar com Flexi"); a foto dela vai na bolinha "EU". */
export const ACCOUNT_CONNECTION = "account.flexi_connection_id";
/** Conta do Flexi usada pelos DOTs, separada da conta que entra no aplicativo. */
export const AGENTS_CONNECTION = "agents.flexi_connection_id";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const CLOSED_MESSAGE: Record<string, string> = {
  denied: "Acesso recusado no navegador.",
  expired: "O pedido expirou. Clique em Logar com Flexi de novo.",
};

export const useAppsStore = defineStore("apps", {
  state: () => ({
    connections: [] as AppConnection[], templates: [] as FlexiRoleTemplate[], busy: false, error: null as string | null,
    loaded: false,
    /** "Logar com Flexi" aguardando a autorizacao no navegador. */
    browserLogin: null as FlexiBrowserLogin | null,
    /** Foto de perfil (data URL) da conta do usuario. */
    photo: null as string | null,
  }),
  getters: {
    flexi: (s) => s.connections.filter((c) => c.kind === "flexi"),
    flexiConnected: (s) => s.connections.some((c) => c.kind === "flexi" && c.status === "connected"),
    needsAttention: (s) => s.connections.filter((c) => c.status !== "connected"),
  },
  actions: {
    async load() {
      try { this.connections = await api.apps.list(); this.error = null; }
      catch (e) { this.error = e instanceof RuntimeError ? e.message : String(e); }
      finally { this.loaded = true; }
    },
    async loadTemplates() {
      try { this.templates = await api.apps.flexiTemplates(); } catch { this.templates = []; }
    },
    async wrap<T>(fn: () => Promise<T>): Promise<T | null> {
      this.busy = true;
      this.error = null;
      try { const r = await fn(); await this.load(); return r; }
      catch (e) { this.error = e instanceof RuntimeError ? e.message : String(e); return null; }
      finally { this.busy = false; }
    },
    connect(input: FlexiConnectInput) { return this.wrap(() => api.apps.flexiConnect(input)); },
    reconnect(id: string, senha?: string, remember?: boolean) { return this.wrap(() => api.apps.flexiReconnect(id, senha, remember)); },
    mfa(id: string, code: string) { return this.wrap(() => api.apps.flexiMfa(id, code)); },
    check(id: string) { return this.wrap(() => api.apps.flexiCheck(id)); },
    async remove(id: string) {
      const result = await this.wrap(() => api.apps.flexiRemove(id));
      const settings = useSettingsStore();
      if (result && settings.items[AGENTS_CONNECTION] === id) settings.items[AGENTS_CONNECTION] = null;
      return result;
    },
    createTeam(id: string) { return this.wrap(() => api.apps.flexiCreateTeam(id)); },
    /**
     * "Logar com Flexi": abre o Flexi no navegador (onde o usuario ja esta logado) e espera a autorizacao.
     * Autorizado: a conexao e criada (ou atualizada) e vira a conta do usuario no app.
     */
    async loginWithBrowser(baseUrl?: string | null): Promise<AppConnection | null> {
      this.error = null;
      let login: FlexiBrowserLogin;
      try { login = await api.apps.flexiBrowserStart(baseUrl); }
      catch (e) { this.error = e instanceof RuntimeError ? e.message : String(e); return null; }
      this.browserLogin = login;
      await this.openBrowserLogin();
      const deadline = login.expires_at ? Date.parse(login.expires_at) : Date.now() + 10 * 60_000;
      const interval = Math.max(1000, login.poll_interval_ms || 2000);
      try {
        while (this.browserLogin?.token === login.token) {
          if (Date.now() > deadline) { this.error = CLOSED_MESSAGE.expired; break; }
          await sleep(interval);
          if (this.browserLogin?.token !== login.token) break;
          const r = await api.apps.flexiBrowserPoll(login.token);
          if (r.status === "pending") continue;
          if (r.status === "authenticated" && r.connection) {
            await this.load();
            await useSettingsStore().set(ACCOUNT_CONNECTION, r.connection.id);
            void this.loadPhoto(r.connection.id);
            return r.connection;
          }
          this.error = CLOSED_MESSAGE[r.status] ?? "O pedido nao vale mais. Clique em Logar com Flexi de novo.";
          break;
        }
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        if (this.browserLogin?.token === login.token) this.browserLogin = null;
        void api.apps.flexiBrowserCancel(login.token).catch(() => undefined);
      }
      return null;
    },
    async openBrowserLogin() {
      if (!this.browserLogin) return;
      try { await openUrl(this.browserLogin.authorize_url); }
      catch (e) { this.error = `Nao foi possivel abrir o navegador: ${e instanceof Error ? e.message : String(e)}`; }
    },
    cancelBrowserLogin() { this.browserLogin = null; },
    async loadPhoto(id: string | null) {
      if (!id) { this.photo = null; return; }
      try { this.photo = (await api.apps.flexiPhoto(id)).data_url; } catch { this.photo = null; }
    },
  },
});
