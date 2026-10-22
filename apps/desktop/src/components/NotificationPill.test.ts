import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";

const { invoke, attention, focused } = vi.hoisted(() => ({ invoke: vi.fn(), attention: vi.fn(), focused: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/window", () => ({
  UserAttentionType: { Informational: 2 },
  getCurrentWindow: () => ({ requestUserAttention: attention, isFocused: focused }),
}));

import NotificationPill from "./NotificationPill.vue";
import { useNotificationsStore } from "@/stores/notifications";
import { notice } from "@/stores/notifications.fixture";

async function setup() {
  const router = createRouter({ history: createMemoryHistory(), routes: [
    { path: "/", component: { template: "<div />" } },
    { path: "/chat/:dotId", component: { template: "<div />" } },
  ] });
  await router.push("/");
  await router.isReady();
  const wrapper = mount(NotificationPill, { global: { plugins: [router] } });
  await flushPromises();
  return { wrapper, router };
}

describe("balao de notificacoes", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    invoke.mockResolvedValue({ cleared: 1 });
    attention.mockResolvedValue(undefined);
    focused.mockResolvedValue(false);
  });

  it("nao aparece sem avisos", async () => {
    const { wrapper } = await setup();
    expect(wrapper.find('[data-testid="notification-pill"]').exists()).toBe(false);
  });

  it("um DOT: mostra o aviso, chama a atencao no Dock e o botao abre a conversa limpando so os dele", async () => {
    const store = useNotificationsStore();
    store.items = [
      notice("1", "carol", "Carolina", "AGY terminou com status ERROR", { detail: "aguardando decisão", action: "Responder" }),
      notice("2", "carol", "Carolina", "Outra coisa"),
    ];
    const { wrapper, router } = await setup();
    const pill = wrapper.find('[data-testid="notification-pill"]');
    expect(pill.text()).toContain("Carolina precisa de você");
    expect(pill.text()).toContain("AGY terminou com status ERROR · aguardando decisão");
    expect(pill.text()).toContain("+1");
    expect(attention).toHaveBeenCalledTimes(1);
    const buttons = pill.findAll("button");
    expect(buttons[buttons.length - 1]!.text()).toBe("Responder");
    await buttons[buttons.length - 1]!.trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.path).toBe("/chat/carol");
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "notifications.clear", params: { id: "1" } });
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "notifications.clear", params: { id: "2" } });
    expect(wrapper.find('[data-testid="notification-pill"]').exists()).toBe(false);
  });

  it("varios DOTs: resume, 'Ver' abre a lista com uma acao por aviso e 'Limpar' zera tudo", async () => {
    const store = useNotificationsStore();
    store.items = [
      notice("1", "carol", "Carolina", "AGY com ERROR", { action: "Responder" }),
      notice("2", "rafa", "Rafael", "3 notas não transmitidas", { detail: "R$ 14.820,00", action: "Ver notas" }),
    ];
    const { wrapper, router } = await setup();
    const pill = wrapper.find('[data-testid="notification-pill"]');
    expect(pill.text()).toContain("2 agentes precisam de você");
    expect(pill.text()).toContain("Carolina · AGY com ERROR · Rafael · 3 notas não transmitidas");
    expect(pill.text()).not.toContain("+1");
    const ver = pill.findAll("button").at(-1)!;
    expect(ver.text()).toBe("Ver");
    await ver.trigger("click");
    const list = wrapper.find('[role="dialog"]');
    expect(list.text()).toContain("2 notificações");
    const rows = list.findAll("li");
    expect(rows[1]!.text()).toContain("Rafael precisa de você");
    expect(rows[1]!.text()).toContain("3 notas não transmitidas · R$ 14.820,00");
    expect(rows[1]!.find("button").text()).toBe("Ver notas");
    await rows[1]!.find("button").trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.path).toBe("/chat/rafa");
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "notifications.clear", params: { id: "2" } });
    expect(invoke).not.toHaveBeenCalledWith("ipc_call", { method: "notifications.clear", params: { id: "1" } });
    // Carolina continua na tela ate o usuario clicar.
    expect(wrapper.find('[data-testid="notification-pill"]').text()).toContain("Carolina precisa de você");
    await wrapper.find('[data-testid="notification-pill"]').findAll("button")[0]!.trigger("click");
    await wrapper.find('[role="dialog"] button').trigger("click");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "notifications.clear", params: { id: null } });
    expect(wrapper.find('[data-testid="notification-pill"]').exists()).toBe(false);
  });
});
