import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useEnginesStore } from "./engines";

const cfg = (kind: string, installed: boolean | null) => ({
  kind, enabled: true, executable_override: null, model: null, updated_at: "2026-10-15T00:00:00Z",
  last_detection: installed === null ? null : {
    kind, display_name: kind, installed, executable_path: installed ? `/x/${kind}` : null, version: installed ? "1.0.0" : null,
    auth_status: "unknown", auth_detail: null, capabilities: [], issues: [], guidance: null, checked_at: "2026-10-15T00:00:00Z",
  },
});

describe("engines store", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("lists and computes availability", async () => {
    invoke.mockResolvedValueOnce([cfg("codex", true), cfg("claude", false), cfg("agy", null)]);
    const s = useEnginesStore();
    await s.load();
    expect(s.available.map((e) => e.kind)).toEqual(["codex"]);
    expect(s.detectedAll).toBe(false);
  });

  it("detect calls engines.detect then reloads", async () => {
    invoke.mockResolvedValueOnce([]).mockResolvedValueOnce([cfg("codex", true), cfg("claude", true), cfg("agy", true)]);
    const s = useEnginesStore();
    await s.detect("codex");
    expect(invoke).toHaveBeenNthCalledWith(1, "ipc_call", { method: "engines.detect", params: { kind: "codex" } });
    expect(s.detectedAll).toBe(true);
    expect(s.detecting).toBeNull();
  });

  it("override trims and nulls empty path", async () => {
    invoke.mockResolvedValueOnce([]).mockResolvedValueOnce([]);
    const s = useEnginesStore();
    await s.setOverride("agy", "   ");
    expect(invoke).toHaveBeenNthCalledWith(1, "ipc_call", { method: "engines.set_override", params: { kind: "agy", path: null } });
  });

  it("setModel trims, nulls empty and replaces the item in place", async () => {
    invoke.mockResolvedValueOnce([cfg("codex", true), cfg("claude", true), cfg("agy", true)]);
    const s = useEnginesStore();
    await s.load();
    invoke.mockResolvedValueOnce({ ...cfg("claude", true), model: "claude-opus-5-5" });
    await s.setModel("claude", "  claude-opus-5-5 ");
    expect(invoke).toHaveBeenNthCalledWith(2, "ipc_call", { method: "engines.set_model", params: { kind: "claude", model: "claude-opus-5-5" } });
    expect(s.byKind("claude")?.model).toBe("claude-opus-5-5");
    invoke.mockResolvedValueOnce({ ...cfg("claude", true), model: null });
    await s.setModel("claude", "   ");
    expect(invoke).toHaveBeenNthCalledWith(3, "ipc_call", { method: "engines.set_model", params: { kind: "claude", model: null } });
    expect(s.byKind("claude")?.model).toBeNull();
  });
});
