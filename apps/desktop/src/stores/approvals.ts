import { defineStore } from "pinia";
import type { Approval } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const useApprovalsStore = defineStore("approvals", {
  state: () => ({ pending: [] as Approval[], history: [] as Approval[], error: null as string | null, deciding: null as string | null }),
  actions: {
    async load() {
      try {
        const all = await api.approvals.list(false, 200);
        this.pending = all.filter((a) => a.status === "pending");
        this.history = all.filter((a) => a.status !== "pending");
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
    onRequested(a: Approval) {
      if (!this.pending.some((p) => p.id === a.id)) this.pending.unshift(a);
    },
    async decide(id: string, approved: boolean, reason?: string) {
      this.deciding = id;
      try {
        await api.approvals.decide(id, approved, reason);
        await this.load();
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.deciding = null;
      }
    },
  },
});
