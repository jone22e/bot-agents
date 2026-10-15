import { defineStore } from "pinia";
import type { Contact } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const useContactsStore = defineStore("contacts", {
  state: () => ({ items: [] as Contact[], query: "", loading: false, error: null as string | null }),
  getters: {
    filtered: (s) => {
      const q = s.query.trim().toLowerCase();
      return q ? s.items.filter((c) => c.dot.name.toLowerCase().includes(q) || (c.title ?? "").toLowerCase().includes(q) || (c.last_message ?? "").toLowerCase().includes(q)) : s.items;
    },
    byDot: (s) => (dotId: string) => s.items.find((c) => c.dot.id === dotId) ?? null,
    attention: (s) => s.items.reduce((n, c) => n + c.pending_requests, 0),
  },
  actions: {
    async load() {
      this.loading = true;
      try {
        this.items = await api.contacts.list();
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.loading = false;
      }
    },
  },
});
