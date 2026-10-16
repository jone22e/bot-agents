import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { Dot, GroupMessage } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import GroupChatView from "./GroupChatView.vue";
import { useRuntimeStore } from "@/stores/runtime";

describe("Grupo geral", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("mostra um chat em ordem de envio e deixa respostas em andamento fora da conversa", async () => {
    const members = [{ id: "carolina", name: "Carolina" }, { id: "jose", name: "Jose" }] as Dot[];
    const messages = [
      { id: "m1", role: "user", content: "Primeira", dot_id: null, dot_name: null, task_id: null, metadata: null, created_at: "2026-10-16T12:00:00Z" },
      { id: "m2", role: "user", content: "Segunda", dot_id: null, dot_name: null, task_id: null, metadata: null, created_at: "2026-10-16T12:01:00Z" },
      { id: "m3", role: "dot", content: "Resposta", dot_id: "carolina", dot_name: "Carolina", task_id: "t1", metadata: { streaming: false }, created_at: "2026-10-16T12:02:00Z" },
      { id: "m4", role: "dot", content: "Parcial", dot_id: "jose", dot_name: "Jose", task_id: "t2", metadata: { streaming: true }, created_at: "2026-10-16T12:03:00Z" },
    ] as GroupMessage[];
    invoke.mockImplementation(async (_command: string, args: { method: string }) => {
      if (args.method === "group.members") return members;
      if (args.method === "group.messages") return messages;
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    useRuntimeStore().connection = "connected";
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/", component: GroupChatView }, { path: "/chat/:id", component: { template: "<div />" } }] });
    await router.push("/");
    await router.isReady();
    const wrapper = mount(GroupChatView, { global: { plugins: [router], stubs: { Avatar: true } } });
    await flushPromises();

    expect(wrapper.find("header").text()).toContain("2 gerentes");
    const rows = wrapper.findAll(".message-row");
    expect(rows.map((row) => row.text())).toEqual([
      expect.stringContaining("Primeira"),
      expect.stringContaining("Segunda"),
      expect.stringContaining("Carolina"),
    ]);
    expect(rows[0]?.classes()).toContain("mine");
    expect(rows[2]?.classes()).toContain("theirs");
    expect(wrapper.find("main").text()).not.toContain("Parcial");
  });

  it("mantém uma resposta longa e sua lista numerada em um único balão", async () => {
    const messages = [
      { id: "u1", role: "user", content: "Como está?", dot_id: null, dot_name: null, task_id: null, metadata: null, created_at: "2026-10-16T12:00:00Z" },
      { id: "r1", role: "dot", content: `Primeiro: ${"a".repeat(430)}\n\n1. Etapa um: ${"b".repeat(120)}\n2. Etapa dois: ${"c".repeat(120)}\n3. Etapa três concluída`, dot_id: "carolina", dot_name: "Carolina", task_id: "t1", metadata: {}, created_at: "2026-10-16T12:01:00Z" },
      { id: "u2", role: "user", content: "Obrigado", dot_id: null, dot_name: null, task_id: null, metadata: null, created_at: "2026-10-16T12:02:00Z" },
    ] as GroupMessage[];
    invoke.mockImplementation(async (_command: string, args: { method: string }) => {
      if (args.method === "group.members") return [{ id: "carolina", name: "Carolina" }];
      if (args.method === "group.messages") return messages;
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    useRuntimeStore().connection = "connected";
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/", component: GroupChatView }, { path: "/chat/:id", component: { template: "<div />" } }] });
    await router.push("/");
    await router.isReady();
    const wrapper = mount(GroupChatView, { global: { plugins: [router], stubs: { Avatar: true } } });
    await flushPromises();

    const rows = wrapper.findAll(".message-row");
    expect(rows).toHaveLength(3);
    expect(rows.map((row) => row.find(".message-bubble").text().slice(0, 8))).toEqual(["Como est", "Primeiro", "Obrigado"]);
    expect(rows[1]?.find(".sender").text()).toBe("Carolina");
    expect(rows[1]?.findAll(".message-bubble ol li")).toHaveLength(3);
    expect(rows[1]?.find(".message-bubble ol li:last-child").text()).toContain("Etapa três concluída");
  });
});
