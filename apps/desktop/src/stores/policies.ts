import { defineStore } from "pinia";
import type { DotWorkspace, LearnedPermission, ToolPolicy, ToolPolicyUpdate } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const usePoliciesStore = defineStore("policies", {
  state: () => ({ dotId: null as string | null, policy: null as ToolPolicy | null, workspaces: [] as DotWorkspace[], learned: [] as LearnedPermission[], error: null as string | null, saving: false }),
  actions: {
    async load(dotId: string) {
      this.dotId = dotId;
      try {
        [this.policy, this.workspaces, this.learned] = await Promise.all([api.policies.get(dotId), api.dots.workspaces(dotId), api.policies.learned(dotId)]);
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
    async loadLearned(dotId: string) {
      if (this.dotId !== dotId) return;
      try { this.learned = await api.policies.learned(dotId); } catch { /* mantem */ }
    },
    async forget(id: string) {
      if (!this.dotId) return;
      try {
        await api.policies.forget(id);
        this.learned = await api.policies.learned(this.dotId);
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
