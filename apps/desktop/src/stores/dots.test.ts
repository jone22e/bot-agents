import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useDotsStore } from "./dots";

const dot = (id: string, status = "active") => ({
  id, name: `Dot ${id}`, description: "", objective: "", instructions: "", primary_engine: null, model: null,
  workspace_path: null, limits: { max_execution_seconds: 1, max_concurrent_tasks: 1, max_tokens_per_task: null, max_cost_usd_per_task: null },
  status, created_at: "2026-10-15T00:00:00Z", updated_at: "2026-10-15T00:00:00Z",
});

describe("dots store", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("loads via ipc_call dots.list", async () => {
    invoke.mockResolvedValueOnce([dot("1"), dot("2", "paused")]);
    const s = useDotsStore();
    await s.load();
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "dots.list", params: { include_archived: false } });
    expect(s.items).toHaveLength(2);
    expect(s.paused).toHaveLength(1);
  });

  it("creates and upserts", async () => {
    invoke.mockResolvedValueOnce(dot("9"));
    const s = useDotsStore();
    await s.create({ name: "x", description: "", objective: "", instructions: "" });
    expect(s.items.map((d) => d.id)).toEqual(["9"]);
    invoke.mockResolvedValueOnce({ ...dot("9"), name: "renamed" });
    await s.update("9", { name: "renamed" });
    expect(s.items[0].name).toBe("renamed");
  });

  it("archive hides item when archived are not shown", async () => {
    invoke.mockResolvedValueOnce([dot("1")]);
    const s = useDotsStore();
    await s.load();
    invoke.mockResolvedValueOnce(dot("1", "archived"));
    await s.archive("1");
    expect(s.items).toHaveLength(0);
  });

  it("surfaces runtime errors with code", async () => {
    invoke.mockRejectedValueOnce({ code: "RUNTIME_UNAVAILABLE", message: "sem conexao" });
    const s = useDotsStore();
    await s.load();
    expect(s.error).toBe("sem conexao");
  });

  it("delete removes locally", async () => {
    invoke.mockResolvedValueOnce([dot("1"), dot("2")]);
    const s = useDotsStore();
    await s.load();
    invoke.mockResolvedValueOnce({ ok: true });
    await s.remove("1");
    expect(invoke).toHaveBeenLastCalledWith("ipc_call", { method: "dots.delete", params: { id: "1" } });
    expect(s.items.map((d) => d.id)).toEqual(["2"]);
  });
});
