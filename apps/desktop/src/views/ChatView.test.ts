import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { Dot } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: async () => () => undefined }) }));

import ChatView from "./ChatView.vue";
import { useDotsStore } from "@/stores/dots";
import { useRuntimeStore } from "@/stores/runtime";

describe("primeira mensagem do DOT", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("explica o inicio automatico e permite iniciar pelo atalho", async () => {
    invoke.mockImplementation(async (_command: string, args: { method: string; params?: { content?: string } }) => {
      if (args.method === "conversations.main") return { id: "c1", dot_id: "d1" };
      if (args.method === "conversations.messages" || args.method === "tasks.list" || args.method === "contacts.list") return [];
      if (args.method === "conversations.send") return {
        message: { id: "m1", role: "user", content: args.params?.content, created_at: "2026-10-16T12:00:00Z", metadata: {} },
        task: { id: "t1" },
      };
      throw new Error(`chamada inesperada: ${args.method}`);
    });

    const dots = useDotsStore();
    dots.items = [{ id: "d1", name: "Carolina", objective: "Cuidar dos anúncios" } as Dot];
    useRuntimeStore().connection = "connected";
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/chat/:dotId", component: ChatView }] });
    await router.push("/chat/d1");
    await router.isReady();
    const wrapper = mount(ChatView, {
      global: { plugins: [router], stubs: { Avatar: true, ChatBubble: true, ComputerPanel: true, ProfilePanel: true, RequestCard: true, ToolGroup: true } },
    });
    await flushPromises();

    expect(wrapper.text()).toContain("inicia automaticamente ao ser criado");
    const sent = () => invoke.mock.calls.filter(([, args]) => args.method === "conversations.send");
    expect(sent()).toHaveLength(0);
    await wrapper.findAll("button").find((button) => button.text() === "Iniciar trabalho")!.trigger("click");
    await flushPromises();
    expect(sent()).toHaveLength(1);
    expect(sent()[0]![1].params.content).toBe("Comece a trabalhar no objetivo que defini para você. Se faltar alguma informação para iniciar, me pergunte.");
    expect(wrapper.text()).not.toContain("Iniciar trabalho");
    wrapper.unmount();
  });

  it("mostra o tempo real de uma tarefa iniciada antes de abrir a conversa", async () => {
    invoke.mockImplementation(async (_command: string, args: { method: string }) => {
      if (args.method === "conversations.main") return { id: "c1", dot_id: "d1" };
      if (args.method === "conversations.messages") return [];
      if (args.method === "tasks.list") return [{ id: "t1", status: "RUNNING", started_at: new Date(Date.now() - 7 * 60_000).toISOString() }];
      throw new Error(`chamada inesperada: ${args.method}`);
    });

    useDotsStore().items = [{ id: "d1", name: "Lucas" } as Dot];
    useRuntimeStore().connection = "connected";
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/chat/:dotId", component: ChatView }] });
    await router.push("/chat/d1");
    await router.isReady();
    const wrapper = mount(ChatView, {
      global: { plugins: [router], stubs: { Avatar: true, ChatBubble: true, ComputerPanel: true, ProfilePanel: true, RequestCard: true, ToolGroup: true } },
    });
    await flushPromises();

    expect(wrapper.text()).toContain("7m 0s");
    expect(invoke.mock.calls.filter(([, args]) => args.method === "conversations.send")).toHaveLength(0);
    wrapper.unmount();
  });
});
