import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { AppConnection } from "@dots/shared-types";

vi.mock("@/composables/useRuntimeEvents", () => ({ useRuntimeEvents: () => undefined }));
vi.mock("@/stores/updater", () => ({ useUpdaterStore: () => ({ init: () => undefined, state: null, dismissed: false }) }));
vi.mock("@/components/MessengerShell.vue", () => ({ default: { template: "<div><slot /></div>" } }));

import App from "./App.vue";
import { ACCOUNT_CONNECTION, useAppsStore } from "@/stores/apps";
import { useRuntimeStore } from "@/stores/runtime";
import { ONBOARDING_COMPLETED, useSettingsStore } from "@/stores/settings";

describe("entrada do app", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("mostra o login antes dos DOTs quando a conta Flexi nao esta conectada", async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/", name: "home", component: { template: "<div>Tela de DOTs</div>" } },
        { path: "/onboarding", name: "onboarding", component: { template: "<div>Login do Flexi</div>" }, meta: { bare: true } },
      ],
    });
    await router.push("/");
    await router.isReady();

    const runtime = useRuntimeStore();
    const settings = useSettingsStore();
    const apps = useAppsStore();
    const wrapper = mount(App, { global: { plugins: [router] } });
    expect(wrapper.text()).not.toContain("Tela de DOTs");

    runtime.connection = "connected";
    settings.items[ONBOARDING_COMPLETED] = true;
    settings.loaded = true;
    apps.loaded = true;
    await flushPromises();
    expect(router.currentRoute.value.name).toBe("onboarding");
    expect(wrapper.text()).toContain("Login do Flexi");
    expect(wrapper.text()).not.toContain("Tela de DOTs");

    apps.connections = [{ id: "c1", kind: "flexi", status: "connected" } as AppConnection];
    settings.items[ACCOUNT_CONNECTION] = "c1";
    await router.push("/");
    await flushPromises();
    expect(wrapper.text()).toContain("Tela de DOTs");

    apps.connections[0]!.status = "needs_login";
    await flushPromises();
    expect(router.currentRoute.value.name).toBe("onboarding");
    expect(wrapper.text()).not.toContain("Tela de DOTs");
  });
});
