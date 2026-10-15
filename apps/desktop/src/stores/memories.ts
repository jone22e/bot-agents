import { defineStore } from "pinia";
import type { Memory, MemoryCreate, MemoryKind, MemoryUpdate } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const useMemoriesStore = defineStore("memories", {
  state: () => ({ dotId: null as string | null, items: [] as Memory[], kind: null as MemoryKind | null, query: "", error: null as string | null, loading: false }),
  actions: {
    async load(dotId?: string) {
      if (dotId) this.dotId = dotId;
      if (!this.dotId) return;
      this.loading = true;
      try {
        this.items = await api.memories.list(this.dotId, this.kind ?? undefined, this.query || undefined);
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.loading = false;
      }
    },
    async create(input: MemoryCreate) {
      await api.memories.create(input);
      await this.load();
    },
    async update(id: string, patch: MemoryUpdate) {
      await api.memories.update(id, patch);
      await this.load();
    },
    async remove(id: string) {
      await api.memories.delete(id);
      this.items = this.items.filter((m) => m.id !== id);
    },
  },
});
