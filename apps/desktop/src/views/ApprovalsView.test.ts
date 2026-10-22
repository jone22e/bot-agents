import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { Approval } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import ApprovalsView from "./ApprovalsView.vue";

const base = { execution_id: "e1", task_id: "t1", conversation_id: "c1", options: [], resources: [], input: {}, status: "pending", resolved_at: null, reason: null, expires_at: "2026-10-22T20:00:00Z" } as const;
const permission = (id: string, at: string): Approval => ({ ...base, id, kind: "permission", dot_id: "rafa", dot_name: "Rafael", tool: "dots_run_command", input: { command: "rm -rf build" }, risk: "exec", resources: ["/tmp/build"], summary: "rodar rm -rf build", requested_at: at });
const question = (id: string, at: string): Approval => ({ ...base, id, kind: "question", dot_id: "carol", dot_name: "Carolina", tool: "ask_user", input: {}, risk: "read", options: ["Publicar agora", "Revisar antes"], summary: "Publico o lote?", requested_at: at });

async function setup(list: Approval[]) {
  invoke.mockImplementation(async (_c: string, args: { method: string; params?: Record<string, unknown> }) => {
    if (args.method === "approvals.list") return list;
    if (args.method === "approvals.decide") { list = list.filter((a) => a.id !== args.params?.id); return { status: "approved" }; }
    throw new Error(`chamada inesperada: ${args.method}`);
  });
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/approvals", component: ApprovalsView }, { path: "/chat/:dotId", component: { template: "<div />" } }] });
  await router.push("/approvals");
  await router.isReady();
  const wrapper = mount(ApprovalsView, { global: { plugins: [router], stubs: { Avatar: true } } });
  await flushPromises();
  return { wrapper, router };
}

describe("central de aprovacoes", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("sem pendencias, diz que nao ha decisao a tomar", async () => {
    const { wrapper } = await setup([{ ...permission("old", "2026-10-22T08:00:00Z"), status: "approved" }]);
    expect(wrapper.text()).toContain("Nenhuma decisão pendente");
    expect(wrapper.text()).toContain("Decididas recentemente (1)");
  });

  it("lista a fila da mais antiga para a mais nova, com justificativa, impacto e evidencias, e decide por item", async () => {
    const { wrapper, router } = await setup([question("q", "2026-10-22T10:00:00Z"), permission("p", "2026-10-22T09:00:00Z")]);
    expect(wrapper.text()).toContain("2 pendentes");
    const items = wrapper.findAll("li[data-approval]");
    expect(items.map((li) => li.attributes("data-approval"))).toEqual(["p", "q"]);
    const perm = items[0]!;
    expect(perm.text()).toContain('Rafael quer rodar o comando "rm -rf build"');
    expect(perm.text()).toContain("executa comandos · /tmp/build");
    expect(perm.text()).toContain("dots_run_command");
    expect(items[1]!.text()).toContain("Carolina pergunta: Publico o lote?");
    expect(items[1]!.text()).toContain("nenhuma acao ate a sua resposta");
    await perm.findAll("button").find((b) => b.text().includes("Nao"))!.trigger("click");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "approvals.decide", params: { id: "p", approved: false, reason: null } });
    expect(wrapper.findAll("li[data-approval]").map((li) => li.attributes("data-approval"))).toEqual(["q"]);
    await wrapper.find('li[data-approval] button[title="Abrir conversa com Carolina"]').trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.path).toBe("/chat/carol");
  });
});
