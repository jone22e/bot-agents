import { beforeEach, describe, expect, it } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { AppConnection, EngineConfiguration } from "@dots/shared-types";
import OnboardingView from "./OnboardingView.vue";
import { ACCOUNT_CONNECTION, AGENTS_CONNECTION, useAppsStore } from "@/stores/apps";
import { useEnginesStore } from "@/stores/engines";
import { useSettingsStore } from "@/stores/settings";

describe("primeira etapa", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("so avanca depois de conectar a conta Flexi", async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: "/onboarding", name: "onboarding", component: OnboardingView }],
    });
    await router.push("/onboarding");
    await router.isReady();

    const apps = useAppsStore();
    const settings = useSettingsStore();
    const wrapper = mount(OnboardingView, {
      global: { plugins: [router], stubs: { FlexiBrowserLogin: { template: "<button>Logar com Flexi</button>" }, FlexiCard: true, EngineCard: true, ErrorBanner: true } },
    });
    expect(wrapper.text()).toContain("Entre com a sua conta do Flexi");
    expect(wrapper.text()).not.toContain("Continuar sem o Flexi");
    expect(wrapper.text()).not.toContain("Bem-vindo ao DOTS");

    apps.connections = [{ id: "c1", kind: "flexi", status: "connected", user_name: "Jone" } as AppConnection];
    settings.items[ACCOUNT_CONNECTION] = "c1";
    await flushPromises();
    expect(wrapper.text()).toContain("Primeiros passos");
    expect(wrapper.text()).toContain("Escolha quem os agentes serão no Flexi");
    const next = wrapper.findAll("button").find((button) => button.text() === "Continuar")!;
    expect(next.attributes("disabled")).toBeDefined();

    apps.connections.push({ id: "c2", kind: "flexi", status: "connected", user_name: "Claude" } as AppConnection);
    settings.items[AGENTS_CONNECTION] = "c2";
    await flushPromises();
    expect(next.attributes("disabled")).toBeUndefined();
    await next.trigger("click");
    expect(wrapper.text()).toContain("Configure o assistente de IA");

    const engines = useEnginesStore();
    engines.items = [{ kind: "codex", enabled: true, model: null, last_detection: { installed: true, auth_status: "authenticated", models: [] } } as EngineConfiguration];
    await flushPromises();
    expect(wrapper.findAll("button").find((button) => button.text() === "Criar o primeiro DOT")?.attributes("disabled")).toBeUndefined();
  });
});
