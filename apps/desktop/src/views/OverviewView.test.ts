import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { Contact, Team } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import OverviewView from "./OverviewView.vue";
import { useContactsStore } from "@/stores/contacts";
import { useTeamsStore } from "@/stores/teams";

describe("relatório da visão geral", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 9, 19, 12));
    setActivePinia(createPinia());
    invoke.mockReset();
  });
  afterEach(() => vi.useRealTimers());

  it("abre fechado e mostra ações e tempo por grupo e agente ao expandir", async () => {
    const iso = (day: number, hour: number, minute = 0) => new Date(2026, 9, day, hour, minute).toISOString();
    invoke.mockImplementation(async (_command: string, args: { method: string }) => {
      if (args.method === "executions.observed_activity_in_period") return [
        { dot_id: "jose", started_at: iso(18, 23, 30), finished_at: iso(19, 0, 30) },
        { dot_id: "jose", started_at: iso(19, 9), finished_at: iso(19, 10, 20) },
        { dot_id: "jose", started_at: iso(19, 11, 45), finished_at: iso(19, 12) },
        { dot_id: "bruno", started_at: iso(19, 10), finished_at: iso(19, 10, 35) },
      ];
      if (args.method === "executions.tool_calls_in_period") return { jose: 3, bruno: 2 };
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    useContactsStore().items = [
      { dot: { id: "jose", name: "Jose" }, state: "busy" },
      { dot: { id: "bruno", name: "Bruno", status: "paused" }, state: "busy", done: true },
    ] as Contact[];
    useTeamsStore().items = [{ id: "faturamento", name: "Faturamento", manager_dot_id: "jose", member_ids: ["bruno"] }] as Team[];
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/overview", component: OverviewView }, { path: "/chat/:dotId", component: { template: "<div />" } }] });
    await router.push("/overview");
    await router.isReady();
    const wrapper = mount(OverviewView, { global: { plugins: [router] } });
    await flushPromises();

    expect(wrapper.text()).toContain("Tempo ativo observado hoje por DOT");
    expect(wrapper.findAll(".row .ball img")).toHaveLength(2);
    expect(wrapper.find(".row .ball img").attributes("src")).toMatch(/^data:image\/svg\+xml/);
    const paused = wrapper.findAll(".row").find((row) => row.text().includes("Bruno"));
    expect(paused?.find(".ball.paused").exists()).toBe(true);
    expect(paused?.text()).toContain("Pausado");
    expect(paused?.find(".ball.working").exists()).toBe(false);
    const toggle = wrapper.find('button[aria-controls="today-report-details"]');
    expect(toggle.attributes("aria-expanded")).toBe("false");
    expect(wrapper.find("#today-report-details").exists()).toBe(false);
    expect(wrapper.text()).toContain("2h 20min");
    await toggle.trigger("click");
    expect(toggle.attributes("aria-expanded")).toBe("true");
    const group = wrapper.find('[data-report-group="faturamento"]');
    expect(group.find("header").text()).toContain("Faturamento");
    expect(group.find("header").text()).toContain("2h 20min");
    expect(group.find("header").text()).toContain("5");
    expect(group.find('[data-report-agent="jose"]').text()).toContain("32h 05min");
    expect(group.find('[data-report-agent="bruno"]').text()).toContain("20h 35min");
    await toggle.trigger("click");
    expect(toggle.attributes("aria-expanded")).toBe("false");
    expect(wrapper.find("#today-report-details").exists()).toBe(false);
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "executions.observed_activity_in_period", params: { start_at: iso(19, 0), end_at: iso(20, 0) } });
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "executions.tool_calls_in_period", params: { start_at: iso(19, 0), end_at: iso(20, 0) } });
    wrapper.unmount();
  });
});
