import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { Dot, GroupMessage, Team } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import GroupChatView from "./GroupChatView.vue";
import { useRuntimeStore } from "@/stores/runtime";

describe("Grupo geral", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("mostra a equipe gerenciada por cada participante e pelo autor da mensagem", async () => {
    const members = [{ id: "carolina", name: "Carolina" }, { id: "jose", name: "Jose" }] as Dot[];
    const teams = [
      { id: "ads", name: "Google Ads", manager_dot_id: "carolina", member_ids: [] },
      { id: "ml", name: "Mercado Livre", manager_dot_id: "jose", member_ids: [] },
      { id: "parent", name: "Marketplaces", manager_dot_id: null, member_ids: ["carolina"] },
    ] as Team[];
    const messages = [{ id: "m1", role: "dot", content: "Pronto", dot_id: "carolina", dot_name: "Carolina", task_id: null, metadata: null, created_at: "2026-10-16T12:00:00Z" }] as GroupMessage[];
    invoke.mockImplementation(async (_command: string, args: { method: string }) => {
      if (args.method === "group.members") return members;
      if (args.method === "group.messages") return messages;
      if (args.method === "teams.list") return teams;
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    useRuntimeStore().connection = "connected";
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/", component: GroupChatView }, { path: "/chat/:id", component: { template: "<div />" } }] });
    await router.push("/");
    await router.isReady();
    const wrapper = mount(GroupChatView, { global: { plugins: [router], stubs: { Avatar: true, ChatBubble: true, Heartbeat: true } } });
    await flushPromises();

    expect(wrapper.find("header").text()).toMatch(/Carolina· Google Ads/);
    expect(wrapper.find("header").text()).toMatch(/Jose· Mercado Livre/);
    expect(wrapper.find(".who").text()).toContain("Equipe Google Ads");
    expect(wrapper.find("header").text()).not.toContain("Marketplaces");
  });
});
