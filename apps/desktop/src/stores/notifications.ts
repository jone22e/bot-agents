import { defineStore } from "pinia";
import type { Notification } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

/**
 * Avisos que os DOTs mandam ao usuario (dots_notify_user). Ficam no balao do topo e no destaque da
 * lista de contatos ate o usuario clicar: abrir a conversa limpa o aviso daquele DOT; "Limpar" limpa todos.
 */
export const useNotificationsStore = defineStore("notifications", {
  state: () => ({ items: [] as Notification[], loaded: false, error: null as string | null }),
  getters: {
    /** Quantos avisos pendentes cada DOT tem (para o destaque na lista). */
    countByDot: (s) => s.items.reduce((m, n) => m.set(n.dot_id, (m.get(n.dot_id) ?? 0) + 1), new Map<string, number>()),
    /** DOTs distintos com aviso pendente, na ordem de chegada. */
    dotIds: (s) => [...new Set(s.items.map((n) => n.dot_id))],
  },
  actions: {
    async load() {
      try {
        this.items = await api.notifications.list(true);
        this.loaded = true;
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
    /** Sem `id`, limpa todas; com `dotId`, limpa as daquele DOT (ao abrir a conversa). */
    async clear(opts: { id?: string; dotId?: string } = {}) {
      const targets = opts.id ? this.items.filter((n) => n.id === opts.id) : opts.dotId ? this.items.filter((n) => n.dot_id === opts.dotId) : this.items;
      if (!targets.length) return;
      const ids = new Set(targets.map((n) => n.id));
      this.items = this.items.filter((n) => !ids.has(n.id));
      try {
        if (!opts.id && !opts.dotId) await api.notifications.clear();
        else await Promise.all(targets.map((n) => api.notifications.clear(n.id)));
        this.error = null;
      } catch (e) {
        await this.load();
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
  },
});
