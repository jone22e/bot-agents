import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import type { Dot, Team } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import DotContextMenu from "./DotContextMenu.vue";
import { useDotMenu } from "@/composables/useDotMenu";
import { useDotsStore } from "@/stores/dots";
import { useTeamsStore } from "@/stores/teams";

const dot = (id: string, status: Dot["status"] = "active") => ({ id, name: id[0].toUpperCase() + id.slice(1), status }) as Dot;
const rightClick = (d: Dot, x = 40, y = 50) => {
  const e = new MouseEvent("contextmenu", { clientX: x, clientY: y, bubbles: true, cancelable: true });
  useDotMenu().open(e, d);
  return e;
};

describe("menu de contexto do DOT", () => {
  let mounted: ReturnType<typeof mount> | null = null;
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); useDotMenu().close(); });
  afterEach(() => { mounted?.unmount(); mounted = null; });

  function setup() {
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/", component: { template: "<div />" } }, { path: "/chat/:dotId", component: { template: "<div />" } }] });
    mounted = mount(DotContextMenu, { global: { plugins: [router] }, attachTo: document.body });
    return { router };
  }
  const menuEl = () => document.body.querySelector('[role="menu"]');

  it("abre no botao direito, pausa o DOT ativo e fecha", async () => {
    invoke.mockImplementation(async (_c: string, args: { method: string; params: { id: string } }) => {
      if (args.method === "dots.pause") return dot(args.params.id, "paused");
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    setup();
    expect(menuEl()).toBeNull();
    const e = rightClick(dot("carolina"));
    expect(e.defaultPrevented).toBe(true);
    await flushPromises();
    const menu = menuEl() as HTMLElement;
    expect(menu).not.toBeNull();
    expect(menu.textContent).toContain("Carolina");
    expect(menu.textContent).toContain("Pausar");
    expect(menu.textContent).toContain("Abrir conversa");
    (menu.querySelector('[data-action="pause"]') as HTMLButtonElement).click();
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "dots.pause", params: { id: "carolina" } });
    expect(useDotsStore().byId("carolina")?.status).toBe("paused");
    expect(menuEl()).toBeNull();
  });

  it("oferece retomar quando pausado e nomeia a equipe quando e gerente", async () => {
    invoke.mockImplementation(async (_c: string, args: { method: string; params: { id: string } }) => {
      if (args.method === "dots.resume") return dot(args.params.id, "active");
      throw new Error(`chamada inesperada: ${args.method}`);
    });
    useTeamsStore().items = [{ id: "google", name: "Google", manager_dot_id: "carolina", member_ids: ["rafael"] }] as Team[];
    setup();
    rightClick(dot("carolina", "paused"));
    await flushPromises();
    const menu = menuEl() as HTMLElement;
    expect(menu.textContent).toContain("Retomar gerente");
    expect(menu.querySelector('[data-action="pause"]')).toBeNull();
    (menu.querySelector('[data-action="resume"]') as HTMLButtonElement).click();
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "dots.resume", params: { id: "carolina" } });
    expect(menuEl()).toBeNull();

    rightClick(dot("carolina", "active"));
    await flushPromises();
    expect((menuEl() as HTMLElement).textContent).toContain("Pausar equipe");
  });

  it("abre a conversa e fecha com Esc ou clique fora", async () => {
    const { router } = setup();
    rightClick(dot("jose"));
    await flushPromises();
    const items = Array.from((menuEl() as HTMLElement).querySelectorAll('[role="menuitem"]')) as HTMLButtonElement[];
    items.find((b) => b.textContent?.includes("Abrir conversa"))!.click();
    await flushPromises();
    expect(router.currentRoute.value.path).toBe("/chat/jose");
    expect(menuEl()).toBeNull();

    rightClick(dot("jose"));
    await flushPromises();
    expect(menuEl()).not.toBeNull();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await flushPromises();
    expect(menuEl()).toBeNull();

    rightClick(dot("jose"));
    await flushPromises();
    document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    await flushPromises();
    expect(menuEl()).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });
});
