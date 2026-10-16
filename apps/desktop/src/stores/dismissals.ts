import { defineStore } from "pinia";
import type { Dismissal } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

/** Demissoes (motivo + licao) por equipe; carregadas sob demanda e recarregadas quando a equipe muda. */
export const useDismissalsStore = defineStore("dismissals", {
  state: () => ({ byTeam: {} as Record<string, Dismissal[]>, error: null as string | null }),
  getters: {
    forTeam: (s) => (teamId: string | null | undefined) => (teamId ? s.byTeam[teamId] ?? [] : []),
  },
  actions: {
    async load(teamId: string) {
      try { this.byTeam[teamId] = await api.dismissals.list({ team_id: teamId, limit: 50 }); this.error = null; }
      catch (e) { this.error = e instanceof RuntimeError ? e.message : String(e); }
    },
    async reloadAll() {
      await Promise.allSettled(Object.keys(this.byTeam).map((id) => this.load(id)));
    },
  },
});
