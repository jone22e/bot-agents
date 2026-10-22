import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { AppConnection } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

import FlexiCard from "./FlexiCard.vue";
import { ACCOUNT_CONNECTION, AGENTS_CONNECTION, useAppsStore } from "@/stores/apps";
import { useSettingsStore } from "@/stores/settings";

const connection = (id: string, name: string) => ({
  id, kind: "flexi", label: name, email: `${id}@teste.com`, user_name: name,
  base_url: "https://flexi.teste.com", empresa: "Teste", status: "connected", remember_password: false,
}) as AppConnection;

describe("contas do Flexi", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("escolhe a conta dos agentes sem trocar a conta de login", async () => {
    const settings = useSettingsStore();
    settings.loaded = true;
    settings.items[ACCOUNT_CONNECTION] = "pessoal";
    const apps = useAppsStore();
    invoke.mockImplementation(async (_command: string, args: { method: string; params?: { key: string; value: string } }) => {
      if (args.method === "apps.list") return [connection("pessoal", "Jone"), connection("agente", "Claude")];
      if (args.method === "settings.set") return { key: args.params?.key, value: args.params?.value };
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/", component: { template: "<div />" } }] });
    await router.push("/");
    await router.isReady();
    const wrapper = mount(FlexiCard, { global: { plugins: [router], stubs: { FlexiBrowserLogin: true, ConfirmDialog: true, Toggle: true, ErrorBanner: true } } });
    await flushPromises();

    expect(wrapper.text()).toContain("Sua conta");
    const rows = wrapper.findAll("li");
    const claude = rows.find((row) => row.text().includes("Claude"));
    expect(claude).toBeDefined();
    await claude!.findAll("button").find((button) => button.text() === "Usar esta conta nos agentes")!.trigger("click");
    await flushPromises();

    expect(settings.items[ACCOUNT_CONNECTION]).toBe("pessoal");
    expect(settings.items[AGENTS_CONNECTION]).toBe("agente");
    expect(wrapper.text()).toContain("Conta dos agentes");
    expect(wrapper.text()).toContain("Os agentes acessam o Flexi como Claude");
    expect(invoke.mock.calls.find(([, args]) => args.method === "settings.set")?.[1].params).toEqual({ key: AGENTS_CONNECTION, value: "agente" });
    expect(apps.flexi).toHaveLength(2);
  });

  it("reabre o canal com o Flexi web pelo botao Reconectar canal", async () => {
    const settings = useSettingsStore();
    settings.loaded = true;
    settings.items[ACCOUNT_CONNECTION] = "pessoal";
    const link = { connected: true, since: "2026-10-22T14:46:21Z", last_error: null, base_url: "https://flexi.teste.com" };
    invoke.mockImplementation(async (_command: string, args: { method: string }) => {
      if (args.method === "apps.list") return [connection("pessoal", "Jone")];
      if (args.method === "apps.flexi.link_status") return link;
      if (args.method === "apps.flexi.link_reconnect") return link;
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/", component: { template: "<div />" } }] });
    await router.push("/");
    await router.isReady();
    const wrapper = mount(FlexiCard, { global: { plugins: [router], stubs: { FlexiBrowserLogin: true, ConfirmDialog: true, Toggle: true, ErrorBanner: true } } });
    await flushPromises();

    expect(wrapper.text()).toContain("Conectado ao Flexi web");
    const button = wrapper.findAll("button").find((b) => b.text().includes("Reconectar canal"));
    expect(button).toBeDefined();
    await button!.trigger("click");
    await flushPromises();
    expect(invoke.mock.calls.some(([, args]) => (args as { method: string }).method === "apps.flexi.link_reconnect")).toBe(true);
    expect(useAppsStore().error).toBeNull();
  });
});
