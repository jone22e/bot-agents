import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Dot } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import FlexiSharePanel from "./FlexiSharePanel.vue";
import { useDotsStore } from "@/stores/dots";

const dot = (flexi_share: Dot["flexi_share"]): Dot => ({
  id: "d1", name: "Gerente", description: "", objective: "", instructions: "", primary_engine: null, model: null, workspace_path: null,
  limits: { max_execution_seconds: 60, max_concurrent_tasks: 1, max_cost_usd_per_task: null, max_file_size_bytes: 1 } as Dot["limits"],
  status: "active", role: "worker", flexi_share, created_at: "2026-10-22T10:00:00Z", updated_at: "2026-10-22T10:00:00Z",
});

describe("FlexiSharePanel", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    invoke.mockReset();
  });

  it("liga o compartilhamento, lista usuarios do Flexi e salva os escolhidos", async () => {
    const dots = useDotsStore();
    dots.items = [dot({ enabled: false, users: [] })];
    invoke.mockImplementation(async (_cmd: string, args: unknown) => {
      const { method, params } = args as { method: string; params: Record<string, unknown> };
      if (method === "apps.flexi.users") return [{ id: 7, nome: "Maria Silva", email: "maria@x.com" }, { id: 9, nome: "Joao", email: "joao@x.com" }];
      if (method === "dots.update") return { ...dots.items[0], flexi_share: (params.patch as { flexi_share: Dot["flexi_share"] }).flexi_share };
      throw new Error(`metodo inesperado ${method}`);
    });
    const wrapper = mount(FlexiSharePanel, { props: { dotId: "d1" } });
    await flushPromises();
    expect(wrapper.text()).not.toContain("Buscar usuário");

    await wrapper.find("button[role=switch]").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("Maria Silva");
    const boxes = wrapper.findAll("input[type=checkbox]");
    await boxes[0].setValue(true);
    await flushPromises();
    expect(wrapper.text()).toContain("Maria Silva ×");

    const save = wrapper.findAll("button").find((b) => b.text() === "Salvar")!;
    expect(save.attributes("disabled")).toBeUndefined();
    await save.trigger("click");
    await flushPromises();
    const call = invoke.mock.calls.find(([, a]) => (a as { method: string }).method === "dots.update");
    expect(call).toBeTruthy();
    expect((call![1] as { params: { patch: unknown } }).params.patch).toEqual({ flexi_share: { enabled: true, users: [{ id: 7, name: "Maria Silva" }] } });
    expect(dots.items[0].flexi_share.enabled).toBe(true);
  });

  it("desligar limpa a lista ao salvar", async () => {
    const dots = useDotsStore();
    dots.items = [dot({ enabled: true, users: [{ id: 7, name: "Maria Silva" }] })];
    invoke.mockImplementation(async (_cmd: string, args: unknown) => {
      const { method, params } = args as { method: string; params: Record<string, unknown> };
      if (method === "apps.flexi.users") return [];
      if (method === "dots.update") return { ...dots.items[0], flexi_share: (params.patch as { flexi_share: Dot["flexi_share"] }).flexi_share };
      throw new Error(`metodo inesperado ${method}`);
    });
    const wrapper = mount(FlexiSharePanel, { props: { dotId: "d1" } });
    await flushPromises();
    expect(wrapper.text()).toContain("Maria Silva ×");
    await wrapper.find("button[role=switch]").trigger("click");
    await wrapper.findAll("button").find((b) => b.text() === "Salvar")!.trigger("click");
    await flushPromises();
    const call = invoke.mock.calls.find(([, a]) => (a as { method: string }).method === "dots.update")!;
    expect((call[1] as { params: { patch: unknown } }).params.patch).toEqual({ flexi_share: { enabled: false, users: [] } });
  });
});
