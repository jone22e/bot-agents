import { defineStore } from "pinia";
import type { Installation, Setting } from "@dots/shared-types";
import { api } from "@/api";

export const ONBOARDING_COMPLETED = "onboarding.completed";

export const useSettingsStore = defineStore("settings", {
  state: () => ({
    items: {} as Record<string, unknown>,
    installation: null as Installation | null,
    loaded: false,
  }),
  getters: {
    onboardingCompleted: (s) => s.items[ONBOARDING_COMPLETED] === true,
  },
  actions: {
    async load() {
      const list: Setting[] = await api.settings.list();
      const map: Record<string, unknown> = {};
      for (const s of list) map[s.key] = s.value;
      this.items = map;
      this.installation = await api.installation.get();
      this.loaded = true;
    },
    async set(key: string, value: unknown) {
      const s = await api.settings.set(key, value);
      this.items[s.key] = s.value;
    },
    async completeOnboarding() {
      await this.set(ONBOARDING_COMPLETED, true);
    },
  },
});
