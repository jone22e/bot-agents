import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import NewDotWizard from "./NewDotWizard.vue";

const team = (id: string, manager_dot_id: string | null) => ({
  id, name: id === "managed" ? "Marketplaces" : "Sem gerente", description: "", manager_dot_id,
  member_ids: [], created_at: "", updated_at: "",
});

async function openWizard(hasManager: () => boolean, canCreate = false) {
  invoke.mockImplementation(async (_command: string, args: { method: string }) => {
    if (args.method === "teams.list") return [team("managed", hasManager() ? "boss" : null), team("unmanaged", null)];
    if (args.method === "dots.list") return [{ id: "boss", name: "Ana" }];
    if (args.method === "engines.list") return canCreate ? [{ kind: "claude", enabled: true, last_detection: { installed: true, auth_status: "authenticated" } }] : [];
    if (args.method === "apps.list") return [];
    if (canCreate && args.method === "dots.create") return { id: "new-manager", name: "Carolina", objective: "Gerenciar a equipe", role: "supervisor", primary_engine: "claude" };
    if (canCreate && ["policies.set", "teams.set_manager", "dots.start_autonomy"].includes(args.method)) return { ok: true };
    throw new Error(`chamada inesperada: ${args.method}`);
  });
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/new", component: { template: "<div />" } }, { path: "/chat/:dotId", component: { template: "<div />" } }] });
  await router.push("/new");
  await router.isReady();
  const wrapper = mount(NewDotWizard, { global: { plugins: [router] } });
  await flushPromises();
  return wrapper;
}

describe("criacao de DOT", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("seleciona gerente por padrao e bloqueia funcionario sem gerente", async () => {
    const wrapper = await openWizard(() => false);
    const manager = wrapper.findAll("button").find((b) => b.text().includes("Gerente") && b.text().includes("Recebe objetivos"));
    const employee = wrapper.findAll("button").find((b) => b.text().includes("Funcionário") && b.text().includes("Executa o que"));
    expect(manager?.classes()).toContain("border-violet-500");
    expect(employee?.attributes("disabled")).toBeDefined();
    expect(wrapper.text()).toContain("Crie um gerente vinculado a uma equipe antes de criar funcionários.");
    expect(wrapper.text()).not.toContain("Sem equipe por enquanto");
  });

  it("inicia o gerente somente depois de configurar permissao e equipe", async () => {
    const wrapper = await openWizard(() => false, true);
    await wrapper.find('input[type="radio"][value="unmanaged"]').setValue();
    const continueButton = () => wrapper.findAll("button").find((b) => b.text().includes("Continuar"))!;
    await continueButton().trigger("click");
    await wrapper.find('input[placeholder="Nome do DOT"]').setValue("Carolina");
    await wrapper.find("textarea").setValue("Gerenciar a equipe.");
    await continueButton().trigger("click");
    expect(wrapper.text()).toContain("Começa a trabalhar automaticamente após a criação.");
    await wrapper.findAll("button").find((b) => b.text().includes("Criar e iniciar"))!.trigger("click");
    await flushPromises();
    const methods = invoke.mock.calls.map(([, args]) => args.method);
    expect(methods.indexOf("dots.create")).toBeLessThan(methods.indexOf("policies.set"));
    expect(methods.indexOf("policies.set")).toBeLessThan(methods.indexOf("teams.set_manager"));
    expect(methods.indexOf("teams.set_manager")).toBeLessThan(methods.indexOf("dots.start_autonomy"));
    expect(invoke.mock.calls.find(([, args]) => args.method === "dots.start_autonomy")?.[1].params).toEqual({ id: "new-manager" });
  });

  it("exige equipe com gerente e nao cria o funcionario se o gerente foi removido", async () => {
    let hasManager = true;
    const wrapper = await openWizard(() => hasManager);
    const employee = wrapper.findAll("button").find((b) => b.text().includes("Funcionário") && b.text().includes("Executa o que"));
    expect(employee?.attributes("disabled")).toBeUndefined();
    await employee!.trigger("click");
    expect(wrapper.text()).toContain("Ana");
    expect(wrapper.text()).not.toContain("Sem equipe por enquanto");
    const continueButton = () => wrapper.findAll("button").find((b) => b.text().includes("Continuar"))!;
    expect(continueButton().attributes("disabled")).toBeDefined();

    await wrapper.find('input[type="radio"][value="managed"]').setValue();
    expect(continueButton().attributes("disabled")).toBeUndefined();
    await continueButton().trigger("click");
    await wrapper.find('input[placeholder="Nome do DOT"]').setValue("Novo funcionário");
    await wrapper.find("textarea").setValue("Responde perguntas da equipe.");
    await continueButton().trigger("click");
    expect(wrapper.text()).toContain("Criar e iniciar");

    hasManager = false;
    await wrapper.findAll("button").find((b) => b.text().includes("Criar e iniciar"))!.trigger("click");
    await flushPromises();
    expect(invoke.mock.calls.map(([, args]) => args.method)).not.toContain("dots.create");
    expect(wrapper.text()).toContain("O gerente desta equipe mudou");
  });
});
