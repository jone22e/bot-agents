import { defineStore } from "pinia";
import type { AppConnection, FlexiConnectInput, FlexiRoleTemplate } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const useAppsStore = defineStore("apps", {
  state: () => ({ connections: [] as AppConnection[], templates: [] as FlexiRoleTemplate[], busy: false, error: null as string | null }),
  getters: {
    flexi: (s) => s.connections.filter((c) => c.kind === "flexi"),
    flexiConnected: (s) => s.connections.some((c) => c.kind === "flexi" && c.status === "connected"),
    needsAttention: (s) => s.connections.filter((c) => c.status !== "connected"),
  },
  actions: {
    async load() {
      try { this.connections = await api.apps.list(); this.error = null; }
      catch (e) { this.error = e instanceof RuntimeError ? e.message : String(e); }
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
    remove(id: string) { return this.wrap(() => api.apps.flexiRemove(id)); },
    createTeam(id: string) { return this.wrap(() => api.apps.flexiCreateTeam(id)); },
  },
});
