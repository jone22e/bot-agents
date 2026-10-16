import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useChatStore } from "./chat";

describe("chat store (uma conversa por DOT)", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("open resolves the main conversation and loads messages", async () => {
    invoke.mockImplementation(async (_c: string, a: { method: string }) => {
      if (a.method === "conversations.main") return { id: "c1", dot_id: "d1" };
      if (a.method === "conversations.messages") return [{ id: "m1", role: "user", content: "oi" }];
      if (a.method === "tasks.list") return [{ id: "t1", status: "RUNNING" }];
      return null;
    });
    const s = useChatStore();
    await s.open("d1");
    expect(s.conversationId).toBe("c1");
    expect(s.messages).toHaveLength(1);
    expect(s.live.status).toBe("running");
    expect(s.live.taskId).toBe("t1");
  });

  it("sends a message with only attachments", async () => {
    invoke.mockResolvedValue({ message: { id: "m2", role: "user", content: "" }, task: { id: "t2" } });
    const s = useChatStore();
    s.conversationId = "c1";
    await s.send("", ["/ws/anexos/a.png"]);
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "conversations.send", params: { conversation_id: "c1", content: "", attachments: ["/ws/anexos/a.png"] } });
    invoke.mockClear();
    await s.send("  ", []);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("streams events for the live task and picks up trigger-started work", async () => {
    const s = useChatStore();
    s.dotId = "d1";
    s.conversationId = "c1";
    invoke.mockResolvedValue([]);
    s.onTaskChanged({ id: "t9", dot_id: "d1", status: "RUNNING" });
    expect(s.live.taskId).toBe("t9");
    s.onExecutionEvent({ task_id: "t9", dot_id: "d1", event: { type: "text_delta", text: "Vi " } });
    s.onExecutionEvent({ task_id: "t9", dot_id: "d1", event: { type: "text_delta", text: "duas coisas" } });
    s.onExecutionEvent({ task_id: "t9", dot_id: "d1", event: { type: "tool_call", call_id: "x", name: "mcp__dots__dots_read_file", input: {} } });
    expect(s.live.text).toBe("Vi duas coisas");
    expect(s.live.tools[0].name).toBe("dots_read_file");
    s.onTaskChanged({ id: "t9", dot_id: "d1", status: "COMPLETED" });
    await new Promise((r) => setTimeout(r, 0));
    expect(s.live.status).toBe("idle");
  });

  it("filters pending requests for the open conversation", () => {
    const s = useChatStore();
    s.dotId = "d1";
    s.conversationId = "c1";
    const list = [
      { id: "a", status: "pending", conversation_id: "c1", dot_id: "d1" },
      { id: "b", status: "pending", conversation_id: "c2", dot_id: "d2" },
      { id: "c", status: "approved", conversation_id: "c1", dot_id: "d1" },
    ] as never[];
    expect(s.pendingFor(list).map((a) => a.id)).toEqual(["a"]);
  });
});
