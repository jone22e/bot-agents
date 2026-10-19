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

  it("mostra ações e tempo de cada agente e a soma do grupo", async () => {
    const iso = (day: number, hour: number, minute = 0) => new Date(2026, 9, day, hour, minute).toISOString();
    invoke.mockImplementation(async (_command: string, args: { method: string }) => {
      if (args.method === "executions.in_period") return [
        { dot_id: "jose", started_at: iso(18, 23, 30), finished_at: iso(19, 0, 30) },
        { dot_id: "jose", started_at: iso(19, 9), finished_at: iso(19, 10, 20) },
        { dot_id: "jose", started_at: iso(19, 11, 45), finished_at: null },
        { dot_id: "bruno", started_at: iso(19, 10), finished_at: iso(19, 10, 35) },
      ];
      if (args.method === "executions.tool_calls_in_period") return { jose: 3, bruno: 2 };
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    useContactsStore().items = [
      { dot: { id: "jose", name: "Jose" }, state: "busy" },
      { dot: { id: "bruno", name: "Bruno" }, state: "idle" },
    ] as Contact[];
    useTeamsStore().items = [{ id: "faturamento", name: "Faturamento", manager_dot_id: "jose", member_ids: ["bruno"] }] as Team[];
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/overview", component: OverviewView }, { path: "/chat/:dotId", component: { template: "<div />" } }] });
    await router.push("/overview");
    await router.isReady();
    const wrapper = mount(OverviewView, { global: { plugins: [router] } });
    await flushPromises();

    const rows = wrapper.findAll("table tbody tr");
    expect(rows.map((row) => row.findAll("th, td").map((cell) => cell.text()))).toEqual([
      ["Faturamento", "5", "2h 40min"],
      ["Jose", "3", "2h 05min"],
      ["Bruno", "2", "0h 35min"],
    ]);
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "executions.in_period", params: { start_at: iso(19, 0), end_at: iso(20, 0) } });
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "executions.tool_calls_in_period", params: { start_at: iso(19, 0), end_at: iso(20, 0) } });
    wrapper.unmount();
  });
});
