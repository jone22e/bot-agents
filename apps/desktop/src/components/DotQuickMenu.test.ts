import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import type { Dot, Schedule } from "@dots/shared-types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import DotQuickMenu from "./DotQuickMenu.vue";
import { useDotsStore } from "@/stores/dots";
import { useEnginesStore } from "@/stores/engines";

const dot = (extra: Partial<Dot> = {}): Dot => ({ id: "d1", name: "Jose", status: "active", primary_engine: "codex", model: "gpt-5.6-terra", role: "worker", ...extra } as Dot);
const schedule: Schedule = { id: "s1", dot_id: "d1", kind: "every", spec: "10", prompt: "Conferir pendencias", enabled: true, next_run_at: new Date(Date.now() + 4 * 60000).toISOString(), last_run_at: null, created_at: "2026-10-22T08:00:00Z" };

function mockRuntime(schedules: Schedule[] = []) {
  invoke.mockImplementation(async (_c: string, args: { method: string; params?: Record<string, unknown> }) => {
    if (args.method === "schedules.list") return schedules;
    if (args.method === "subscriptions.list") return [];
    if (args.method === "dots.update") return dot({ model: (args.params?.patch as { model: string }).model });
    if (args.method === "dots.pause") return dot({ status: "paused" });
    if (args.method === "dots.resume") return dot();
    throw new Error(`chamada inesperada: ${args.method}`);
  });
}

async function setup(d = dot()) {
  useDotsStore().items = [d];
  useEnginesStore().items = [{ kind: "codex", model: "gpt-5", last_detection: { models: [{ id: "gpt-5.6-terra", label: "GPT-5.6 Terra" }, { id: "gpt-5", label: "GPT-5" }] } } as never];
  const wrapper = mount(DotQuickMenu, { props: { dot: d }, global: { stubs: { Avatar: true } } });
  await flushPromises();
  return wrapper;
}
const action = (w: ReturnType<typeof mount>, name: string) => w.find(`[data-action="${name}"]`);

describe("menu rapido do DOT", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("mostra estado, modelo e proxima rodada, e emite as acoes de perfil", async () => {
    mockRuntime([schedule]);
    const w = await setup();
    expect(w.text()).toContain("Ativo · proxima rodada em 4 min");
    expect(w.text()).toContain("GPT-5.6 Terra · Codex (OpenAI)");
    expect(w.text()).toContain("A cada 10 min");
    await action(w, "schedule").trigger("click");
    await action(w, "instructions").trigger("click");
    await action(w, "profile").trigger("click");
    expect(w.emitted("profile")).toEqual([["gatilhos"], ["sobre"], [null]]);
    await action(w, "run").trigger("click");
    expect(w.emitted("run")).toHaveLength(1);
  });

  it("troca o modelo sem sair da conversa", async () => {
    mockRuntime();
    const w = await setup();
    expect(w.text()).toContain("Nenhum");
    await action(w, "change-model").trigger("click");
    const picker = w.findComponent({ name: "ModelPicker" });
    expect(picker.exists()).toBe(true);
    picker.vm.$emit("update:modelValue", "gpt-5");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "dots.update", params: { id: "d1", patch: { model: "gpt-5" } } });
    expect(useDotsStore().byId("d1")?.model).toBe("gpt-5");
    expect(w.text()).toContain("GPT-5 · Codex (OpenAI)");
    expect(w.findComponent({ name: "ModelPicker" }).exists()).toBe(false);
  });

  it("pausa e retoma; pausado nao executa", async () => {
    mockRuntime();
    const w = await setup();
    await action(w, "pause").trigger("click");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "dots.pause", params: { id: "d1" } });
    expect(w.text()).toContain("Pausado");
    expect((action(w, "run").element as HTMLButtonElement).disabled).toBe(true);
    await action(w, "resume").trigger("click");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "dots.resume", params: { id: "d1" } });
    expect((action(w, "run").element as HTMLButtonElement).disabled).toBe(false);
  });

  it("fecha com Esc", async () => {
    mockRuntime();
    const w = await setup();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(w.emitted("close")).toHaveLength(1);
  });
});
