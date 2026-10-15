import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { useApprovalsStore } from "./approvals";

const ap = (id: string, status = "pending") => ({ id, status, tool: "dots_write_file", risk: "write", resources: [], summary: "x", input: {}, execution_id: "e", task_id: "t", dot_id: "d", dot_name: "D", requested_at: "2026-10-15T00:00:00Z", expires_at: "2026-10-15T00:10:00Z", resolved_at: null, reason: null });

describe("approvals store", () => {
  beforeEach(() => { setActivePinia(createPinia()); invoke.mockReset(); });

  it("splits pending and history, decides and reloads", async () => {
    invoke.mockResolvedValueOnce([ap("1"), ap("2", "approved")]);
    const s = useApprovalsStore();
    await s.load();
    expect(s.pending.map((a) => a.id)).toEqual(["1"]);
    expect(s.history.map((a) => a.id)).toEqual(["2"]);
    s.onRequested(ap("3") as never);
    s.onRequested(ap("3") as never);
    expect(s.pending).toHaveLength(2);
    invoke.mockResolvedValueOnce(ap("1", "approved")).mockResolvedValueOnce([ap("1", "approved"), ap("2", "approved")]);
    await s.decide("1", true);
    expect(invoke).toHaveBeenCalledWith("ipc_call", { method: "approvals.decide", params: { id: "1", approved: true, reason: null } });
    expect(s.pending).toHaveLength(0);
  });
});
