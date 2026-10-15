import { defineStore } from "pinia";
import type { AgySetupStatus, EngineConfiguration, EngineKind } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const useEnginesStore = defineStore("engines", {
  state: () => ({
    items: [] as EngineConfiguration[],
    loading: false,
    detecting: null as EngineKind | "all" | null,
    error: null as string | null,
    agy: null as AgySetupStatus | null,
    agyBusy: false,
  }),
  getters: {
    byKind: (s) => (kind: EngineKind) => s.items.find((e) => e.kind === kind) ?? null,
    available: (s) => s.items.filter((e) => e.enabled && e.last_detection?.installed),
    detectedAll: (s) => s.items.length > 0 && s.items.every((e) => e.last_detection !== null),
  },
  actions: {
    async load() {
      this.loading = true;
      this.error = null;
      try {
        this.items = await api.engines.list();
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.loading = false;
      }
    },
    async detect(kind?: EngineKind) {
      this.detecting = kind ?? "all";
      this.error = null;
      try {
        await api.engines.detect(kind);
        await this.load();
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.detecting = null;
      }
    },
    async setOverride(kind: EngineKind, path: string | null) {
      this.detecting = kind;
      try {
        await api.engines.setOverride(kind, path && path.trim() ? path.trim() : null);
        await this.load();
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.detecting = null;
      }
    },
    async agyStatus() {
      try { this.agy = await api.engines.agyStatus(); } catch { this.agy = null; }
    },
    async agySetup(remove = false) {
      this.agyBusy = true;
      try {
        this.agy = remove ? await api.engines.agyRemove() : await api.engines.agySetup();
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.agyBusy = false;
      }
    },
    async setEnabled(kind: EngineKind, enabled: boolean) {
      const cfg = await api.engines.setEnabled(kind, enabled);
      const i = this.items.findIndex((e) => e.kind === kind);
      if (i >= 0) this.items[i] = cfg;
    },
    async setModel(kind: EngineKind, model: string | null) {
      try {
        const cfg = await api.engines.setModel(kind, model && model.trim() ? model.trim() : null);
        const i = this.items.findIndex((e) => e.kind === kind);
        if (i >= 0) this.items[i] = cfg;
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
  },
});
