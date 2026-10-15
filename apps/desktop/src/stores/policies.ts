import { defineStore } from "pinia";
import type { DotWorkspace, ToolPolicy, ToolPolicyUpdate } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const usePoliciesStore = defineStore("policies", {
  state: () => ({ dotId: null as string | null, policy: null as ToolPolicy | null, workspaces: [] as DotWorkspace[], error: null as string | null, saving: false }),
  actions: {
    async load(dotId: string) {
      this.dotId = dotId;
      try {
        [this.policy, this.workspaces] = await Promise.all([api.policies.get(dotId), api.dots.workspaces(dotId)]);
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
    async save(patch: ToolPolicyUpdate) {
      if (!this.dotId) return;
      this.saving = true;
      try {
        this.policy = await api.policies.set(this.dotId, patch);
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.saving = false;
      }
    },
    async addWorkspace(path: string, canWrite: boolean) {
      if (!this.dotId) return;
      try {
        await api.workspaces.add(this.dotId, path, true, canWrite);
        this.workspaces = await api.dots.workspaces(this.dotId);
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
    async removeWorkspace(path: string) {
      if (!this.dotId) return;
      await api.workspaces.remove(this.dotId, path);
      this.workspaces = await api.dots.workspaces(this.dotId);
    },
  },
});
