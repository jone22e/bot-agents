import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { Approval, Dot } from "@dots/shared-types";

const { invoke, badge, attention, focused } = vi.hoisted(() => ({
  invoke: vi.fn(), badge: vi.fn(), attention: vi.fn(), focused: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/window", () => ({
  UserAttentionType: { Informational: 2 },
  getCurrentWindow: () => ({ setBadgeCount: badge, requestUserAttention: attention, isFocused: focused }),
}));

import ManagerRequestAlert from "./ManagerRequestAlert.vue";
import { useApprovalsStore } from "@/stores/approvals";
import { useDotsStore } from "@/stores/dots";

function question(id: string, dotId: string, name: string): Approval {
  return {
    id, kind: "question", status: "pending", dot_id: dotId, dot_name: name,
    summary: `Publicar o lote ${id}?`, options: ["Publicar agora", "Revisar antes"],
    tool: "ask_user", input: {}, risk: "read", resources: [], execution_id: "e1", task_id: "t1",
    conversation_id: "c1", requested_at: "2026-10-16T17:00:00Z", expires_at: "2026-10-17T17:00:00Z",
    resolved_at: null, reason: null,
  };
}

async function setup() {
  const router = createRouter({ history: createMemoryHistory(), routes: [
    { path: "/", component: { template: "<div />" } },
    { path: "/chat/:dotId", component: { template: "<div />" } },
  ] });
  await router.push("/");
  await router.isReady();
  const wrapper = mount(ManagerRequestAlert, { global: { plugins: [router] } });
  await flushPromises();
  return { wrapper, router };
}

describe("alerta de resposta do gerente", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    badge.mockResolvedValue(undefined);
    attention.mockResolvedValue(undefined);
    focused.mockResolvedValue(false);
  });

  it("mostra as opcoes do gerente, sinaliza no Dock e envia a escolha clicada", async () => {
    useDotsStore().items = [
      { id: "manager", name: "Carolina", role: "supervisor" } as Dot,
      { id: "worker", name: "Lucas", role: "worker" } as Dot,
    ];
    const approvals = useApprovalsStore();
    approvals.pending = [question("2", "worker", "Lucas"), question("1", "manager", "Carolina")];
    invoke.mockImplementation(async (_command: string, args: { method: string }) => {
      if (args.method === "approvals.decide") return { status: "approved" };
      if (args.method === "approvals.list") return [];
      throw new Error(`chamada inesperada: ${args.method}`);
    });

    const { wrapper } = await setup();
    expect(wrapper.text()).toContain("Carolina espera sua resposta");
    expect(wrapper.text()).toContain("Publicar o lote 1?");
    expect(wrapper.text()).toContain("Publicar agora");
    expect(wrapper.text()).toContain("Revisar antes");
    expect(wrapper.text()).not.toContain("Lucas espera sua resposta");
    expect(badge).toHaveBeenCalledWith(1);
    expect(attention).toHaveBeenCalledOnce();

    await wrapper.findAll("button").find((button) => button.text().includes("Revisar antes"))!.trigger("click");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("ipc_call", {
      method: "approvals.decide", params: { id: "1", approved: true, reason: "Revisar antes" },
    });
    expect(wrapper.text()).not.toContain("Carolina espera sua resposta");
    expect(badge).toHaveBeenCalledWith(undefined);
    wrapper.unmount();
  });

  it("permite guardar para depois e abre novamente quando chega outra pergunta", async () => {
    badge.mockRejectedValue(new Error("badge indisponivel"));
    useDotsStore().items = [{ id: "manager", name: "Carolina", role: "supervisor" } as Dot];
    const approvals = useApprovalsStore();
    approvals.pending = [question("1", "manager", "Carolina")];
    const { wrapper } = await setup();

    await wrapper.find('[aria-label="Ver depois"]').trigger("click");
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(wrapper.text()).toContain("Carolina espera sua resposta");
    await wrapper.findAll("button").find((button) => button.text() === "Responder")!.trigger("click");
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true);

    await wrapper.find('[aria-label="Ver depois"]').trigger("click");
    approvals.onRequested(question("3", "manager", "Carolina"));
    await flushPromises();
    expect(wrapper.find('[role="dialog"]').text()).toContain("Publicar o lote 3?");
    expect(badge).toHaveBeenCalledWith(2);
    expect(attention).toHaveBeenCalledTimes(2);
    wrapper.unmount();
  });
});
