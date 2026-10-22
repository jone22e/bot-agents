import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import PerformanceView from "./PerformanceView.vue";

describe("avaliacao de desempenho", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("mostra os indicadores totais e por DOT e troca o periodo", async () => {
    const carol = { dotId: "carol", dotName: "Carolina", finished: 2, completed: 1, failed: 1, recurrent: 0, costUsd: 1, executions: 1, resolutionMs: 3600000, reverted: 1, humanTasks: 1 };
    const total = { ...carol, dotId: "*", dotName: "Todos" };
    invoke.mockImplementation(async (_c: string, args: { method: string; params?: Record<string, unknown> }) => {
      if (args.method === "performance.summary") return { start: "2026-10-15T00:00:00Z", end: "2026-10-22T00:00:00Z", days: args.params?.days, total, dots: [carol] };
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/performance", component: PerformanceView }, { path: "/chat/:dotId", component: { template: "<div />" } }] });
    await router.push("/performance");
    await router.isReady();
    const wrapper = mount(PerformanceView, { global: { plugins: [router], stubs: { Avatar: true } } });
    await flushPromises();

    const totals = wrapper.find('[data-testid="totals"]').text();
    expect(totals).toContain("Taxa de sucesso50%");
    expect(totals).toContain("Reincidência0%");
    expect(totals).toContain("Custo por operaçãoUS$ 0.500");
    expect(totals).toContain("Tempo de resolução1h 00min");
    expect(totals).toContain("Ações revertidas1");
    expect(totals).toContain("Intervenção humana1 (50%)");
    const row = wrapper.find('tr[data-dot="carol"]');
    expect(row.text()).toContain("Carolina");
    expect(row.text()).toContain("50%");
    const summaryDays = () => invoke.mock.calls.filter(([, a]) => a.method === "performance.summary").map(([, a]) => a.params.days);
    expect(summaryDays()).toEqual([7]);
    await wrapper.find('button[data-days="30"]').trigger("click");
    await flushPromises();
    expect(summaryDays()).toEqual([7, 30]);
    await row.trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.path).toBe("/chat/carol");
  });
});
