import { defineStore } from "pinia";
import type { Team, TeamCreate, TeamUpdate } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";
import { moveId } from "@/composables/useDragReorder";
import { useSettingsStore } from "@/stores/settings";

/** Ordem das equipes escolhida pelo usuario (lista de contatos e visao geral): ids, da primeira a ultima. */
export const TEAMS_ORDER = "ui.teams_order";

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
    /** Equipes na ordem escolhida pelo usuario; as que ele nunca ordenou vao ao final, na ordem do servico. */
    ordered: (s): Team[] => {
      const raw = useSettingsStore().items[TEAMS_ORDER];
      const order = Array.isArray(raw) ? (raw as unknown[]).filter((x): x is string => typeof x === "string") : [];
      if (!order.length) return s.items;
      const rank = new Map(order.map((id, i) => [id, i]));
      return [...s.items].map((t, i) => ({ t, k: rank.get(t.id) ?? order.length + i })).sort((a, b) => a.k - b.k).map((x) => x.t);
    },
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
    /** Move a equipe `fromId` para antes (ou depois) de `toId` e guarda a ordem completa. */
    async reorder(fromId: string, toId: string, before: boolean) {
      const ids = this.ordered.map((t) => t.id);
      const next = moveId(ids, fromId, toId, before);
      if (next === ids) return;
      await useSettingsStore().set(TEAMS_ORDER, next);
    },
  },
});
