import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => []) }));

import SettingsView from "./SettingsView.vue";
import { useEnginesStore } from "@/stores/engines";
import { useOpsStore } from "@/stores/ops";
import { useRuntimeStore } from "@/stores/runtime";
import { useSettingsStore } from "@/stores/settings";
import { useUsageStore } from "@/stores/usage";

describe("trabalhos simultâneos", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("mostra o novo padrão e salva o limite sem reiniciar trabalhos ativos", async () => {
    const runtime = useRuntimeStore();
    runtime.connection = "connected";
    runtime.refresh = vi.fn();
    runtime.restart = vi.fn();
    useEnginesStore().load = vi.fn();
    useOpsStore().load = vi.fn();
    useUsageStore().load = vi.fn();
    const settings = useSettingsStore();
    settings.set = vi.fn(async (key: string, value: unknown) => { settings.items[key] = value; });

    const wrapper = mount(SettingsView, {
      global: { stubs: { PageHeader: true, Toggle: true, EngineCard: true, ErrorBanner: true } },
    });
    const input = wrapper.find('input[type="number"][max="8"]');
    expect((input.element as HTMLInputElement).value).toBe("4");

    await input.setValue("6");
    await flushPromises();
    expect(settings.set).toHaveBeenCalledWith("runtime.max_concurrent_executions", 6);
    expect(wrapper.text()).toContain("Reinicie em Serviço em segundo plano");
    expect(runtime.restart).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
