import { defineStore } from "pinia";
import type { EventSubscription, Schedule, ScheduleCreate, ScheduleUpdate, SubscriptionCreate, SubscriptionUpdate } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const useAutomationStore = defineStore("automation", {
  state: () => ({ dotId: null as string | null, subscriptions: [] as EventSubscription[], schedules: [] as Schedule[], error: null as string | null }),
  actions: {
    async load(dotId?: string) {
      if (dotId) this.dotId = dotId;
      if (!this.dotId) return;
      try {
        [this.subscriptions, this.schedules] = await Promise.all([api.subscriptions.list(this.dotId), api.schedules.list(this.dotId)]);
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
    async wrap(fn: () => Promise<unknown>) {
      try { await fn(); await this.load(); this.error = null; }
      catch (e) { this.error = e instanceof RuntimeError ? e.message : String(e); }
    },
    createSubscription(input: SubscriptionCreate) { return this.wrap(() => api.subscriptions.create(input)); },
    updateSubscription(id: string, patch: SubscriptionUpdate) { return this.wrap(() => api.subscriptions.update(id, patch)); },
    deleteSubscription(id: string) { return this.wrap(() => api.subscriptions.delete(id)); },
    createSchedule(input: ScheduleCreate) { return this.wrap(() => api.schedules.create(input)); },
    updateSchedule(id: string, patch: ScheduleUpdate) { return this.wrap(() => api.schedules.update(id, patch)); },
    deleteSchedule(id: string) { return this.wrap(() => api.schedules.delete(id)); },
  },
});
