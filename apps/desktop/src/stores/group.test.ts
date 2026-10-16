import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useGroupStore } from "./group";

const members = [{ id: "m1", name: "Gerente A" }, { id: "m2", name: "Gerente B" }];
const user = { id: "g1", role: "user", content: "Bom dia", dot_id: null, dot_name: null, task_id: null, metadata: {}, created_at: "2026-10-16T10:00:00Z" };
const reply = { id: "r1", role: "dot", content: "", dot_id: "m1", dot_name: "Gerente A", task_id: "t1", metadata: { streaming: true }, created_at: "2026-10-16T10:00:01Z" };

describe("group store", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("loads members and messages and sends to everyone", async () => {
    invoke.mockImplementation(async (_c: string, a: { method: string }) => {
      if (a.method === "group.members") return members;
      if (a.method === "group.messages") return [user];
      if (a.method === "group.send") return { message: { ...user, id: "g2", content: "Oi" }, tasks: [{}, {}] };
      return null;
    });
    const s = useGroupStore();
    await s.load();
    expect(s.isMember("m2")).toBe(true);
    expect(s.isMember("x")).toBe(false);
    await s.send("  Oi ");
    expect(s.last?.content).toBe("Oi");
    expect(invoke.mock.calls.find((c) => (c[1] as { method: string }).method === "group.send")![1]).toMatchObject({ params: { content: "Oi" } });
  });

  it("follows the live text of a manager reply only while open", async () => {
    invoke.mockImplementation(async (_c: string, a: { method: string }) => (a.method === "group.members" ? members : a.method === "group.messages" ? [user, reply] : null));
    const s = useGroupStore();
    await s.load();
    expect(s.last?.id).toBe("g1");
    const ev = (text: string) => ({ task_id: "t1", dot_id: "m1", event: { type: "text_delta", text } as never });
    s.onExecutionEvent(ev("ignorado"));
    expect(s.live.t1).toBeUndefined();
    s.open = true;
    s.onExecutionEvent(ev("Tudo "));
    s.onExecutionEvent(ev("certo"));
    expect(s.live.t1).toBe("Tudo certo");
    // fora do grupo (outro DOT) nao entra
    s.onExecutionEvent({ task_id: "t1", dot_id: "outro", event: { type: "text_delta", text: "x" } as never });
    expect(s.live.t1).toBe("Tudo certo");
  });
});
