import { defineStore } from "pinia";
import type { Dot, DotCreate, DotUpdate } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const useDotsStore = defineStore("dots", {
  state: () => ({
    items: [] as Dot[],
    includeArchived: false,
    loading: false,
    error: null as string | null,
  }),
  getters: {
    byId: (s) => (id: string) => s.items.find((d) => d.id === id) ?? null,
    active: (s) => s.items.filter((d) => d.status === "active"),
    paused: (s) => s.items.filter((d) => d.status === "paused"),
    archived: (s) => s.items.filter((d) => d.status === "archived"),
  },
  actions: {
    async load() {
      this.loading = true;
      this.error = null;
      try {
        this.items = await api.dots.list(this.includeArchived);
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.loading = false;
      }
    },
    async setIncludeArchived(v: boolean) {
      this.includeArchived = v;
      await this.load();
    },
    upsert(dot: Dot) {
      const i = this.items.findIndex((d) => d.id === dot.id);
      if (i >= 0) this.items[i] = dot;
      else this.items.push(dot);
    },
    async create(input: DotCreate) {
      const dot = await api.dots.create(input);
      this.upsert(dot);
      return dot;
    },
    async update(id: string, patch: DotUpdate) {
      const dot = await api.dots.update(id, patch);
      this.upsert(dot);
      return dot;
    },
    async duplicate(id: string) {
      const dot = await api.dots.duplicate(id);
      this.upsert(dot);
      return dot;
    },
    async pause(id: string) { this.upsert(await api.dots.pause(id)); },
    async resume(id: string) { this.upsert(await api.dots.resume(id)); },
    async archive(id: string) {
      const dot = await api.dots.archive(id);
      if (this.includeArchived) this.upsert(dot);
      else this.items = this.items.filter((d) => d.id !== id);
    },
    async remove(id: string) {
      await api.dots.delete(id);
      this.items = this.items.filter((d) => d.id !== id);
    },
  },
});
