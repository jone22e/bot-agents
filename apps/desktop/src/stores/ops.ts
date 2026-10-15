import { defineStore } from "pinia";
import type { RuntimeStats } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const useOpsStore = defineStore("ops", {
  state: () => ({ stats: null as RuntimeStats | null, logs: [] as string[], error: null as string | null }),
  actions: {
    async load() {
      try { this.stats = await api.stats.get(); this.error = null; }
      catch (e) { this.error = e instanceof RuntimeError ? e.message : String(e); }
    },
    async loadLogs() {
      try { this.logs = await api.runtime.logs(200); } catch { this.logs = []; }
    },
  },
});
