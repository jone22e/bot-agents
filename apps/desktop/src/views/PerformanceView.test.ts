import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { Dot } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import PerformanceView from "./PerformanceView.vue";
import { useDotsStore } from "@/stores/dots";

describe("avaliacao de desempenho", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("mostra os indicadores totais e por DOT e troca o periodo", async () => {
    const h = (hoursAgo: number) => new Date(Date.now() - hoursAgo * 3600000).toISOString();
    invoke.mockImplementation(async (_c: string, args: { method: string; params?: Record<string, unknown> }) => {
      if (args.method === "tasks.list") return [
        { id: "1", dot_id: "carol", status: "COMPLETED", title: "A", created_at: h(5), updated_at: h(4), finished_at: h(4), attempts: 1 },
        { id: "2", dot_id: "carol", status: "FAILED", title: "B", created_at: h(3), updated_at: h(2), finished_at: h(2), attempts: 1 },
      ];
      if (args.method === "executions.in_period") return [{ id: "e1", dot_id: "carol", task_id: "1", started_at: h(5), cost_usd: 1 }];
      if (args.method === "approvals.list") return [{ id: "a1", dot_id: "carol", task_id: "1", requested_at: h(5) }];
      if (args.method === "audit.list") return [{ id: "x", actor: "dot:carol", action: "tool.dots_delete_file", details: { outcome: "rejected" }, created_at: h(5) }];
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    useDotsStore().items = [{ id: "carol", name: "Carolina" } as Dot];
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
    const periodCalls = () => invoke.mock.calls.filter(([, a]) => a.method === "executions.in_period").length;
    expect(periodCalls()).toBe(1);
    await wrapper.find('button[data-days="30"]').trigger("click");
    await flushPromises();
    expect(periodCalls()).toBe(2);
    await row.trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.path).toBe("/chat/carol");
  });
});
