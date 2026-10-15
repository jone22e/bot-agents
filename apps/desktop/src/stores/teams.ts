import { defineStore } from "pinia";
import type { Team, TeamCreate, TeamUpdate } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

/** Equipes: gerente (DOT) + funcionarios (DOTs). A delegacao do gerente e derivada disso no runtime. */
export const useTeamsStore = defineStore("teams", {
  state: () => ({ items: [] as Team[], loading: false, busy: false, error: null as string | null }),
  getters: {
    byId: (s) => (id: string) => s.items.find((t) => t.id === id) ?? null,
    /** Equipe da qual o DOT e funcionario. */
    memberOf: (s) => (dotId: string) => s.items.find((t) => t.member_ids.includes(dotId)) ?? null,
    /** Equipe que o DOT gerencia. */
    managedBy: (s) => (dotId: string) => s.items.find((t) => t.manager_dot_id === dotId) ?? null,
    withoutManager: (s) => s.items.filter((t) => !t.manager_dot_id),
  },
  actions: {
    async load() {
      this.loading = true;
      try { this.items = await api.teams.list(); this.error = null; }
      catch (e) { this.error = e instanceof RuntimeError ? e.message : String(e); }
      finally { this.loading = false; }
    },
    async wrap<T>(fn: () => Promise<T>): Promise<T | null> {
      this.busy = true;
      this.error = null;
      try { const r = await fn(); await this.load(); return r; }
      catch (e) { this.error = e instanceof RuntimeError ? e.message : String(e); return null; }
      finally { this.busy = false; }
    },
    create(input: TeamCreate) { return this.wrap(() => api.teams.create(input)); },
    update(id: string, patch: TeamUpdate) { return this.wrap(() => api.teams.update(id, patch)); },
    remove(id: string) { return this.wrap(() => api.teams.remove(id)); },
    setManager(id: string, dotId: string | null) { return this.wrap(() => api.teams.setManager(id, dotId)); },
    addMember(id: string, dotId: string) { return this.wrap(() => api.teams.addMember(id, dotId)); },
    removeMember(dotId: string) { return this.wrap(() => api.teams.removeMember(dotId)); },
  },
});
