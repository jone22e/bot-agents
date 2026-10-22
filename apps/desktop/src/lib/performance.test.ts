import { describe, expect, it } from "vitest";
import type { Approval, AuditLog, Execution, Task } from "@dots/shared-types";
import { computePerformance, duration, isRevertedAction, pct, totalPerformance, usd } from "./performance";

const task = (id: string, dot: string, status: Task["status"], created: string, finished: string | null, extra: Partial<Task> = {}): Task =>
  ({ id, dot_id: dot, status, title: `Tarefa ${id}`, created_at: created, updated_at: finished ?? created, finished_at: finished, attempts: 1, ...extra } as Task);
const exec = (dot: string, started: string, cost: number | null): Execution => ({ id: `e-${started}`, task_id: "t", dot_id: dot, started_at: started, cost_usd: cost } as Execution);
const audit = (dot: string, action: string, details: Record<string, unknown>, at = "2026-10-20T10:00:00Z"): AuditLog => ({ id: "a", actor: `dot:${dot}`, action, entity_type: "execution", entity_id: null, details, created_at: at });

describe("avaliacao de desempenho", () => {
  const start = new Date("2026-10-15T00:00:00Z");
  const end = new Date("2026-10-22T00:00:00Z");

  it("calcula sucesso, reincidencia, custo, tempo, revertidas e intervencao por DOT", () => {
    const rows = computePerformance({
      start, end,
      dotNames: new Map([["carol", "Carolina"], ["rafa", "Rafael"]]),
      tasks: [
        task("1", "carol", "COMPLETED", "2026-10-20T10:00:00Z", "2026-10-20T10:10:00Z"),
        task("2", "carol", "FAILED", "2026-10-20T11:00:00Z", "2026-10-20T11:05:00Z", { title: "Publicar lote" }),
        task("3", "carol", "FAILED", "2026-10-21T11:00:00Z", "2026-10-21T11:05:00Z", { title: "publicar lote " }),
        task("4", "carol", "COMPLETED", "2026-10-21T12:00:00Z", "2026-10-21T12:30:00Z", { attempts: 2 }),
        task("5", "carol", "COMPLETED", "2026-10-01T12:00:00Z", "2026-10-01T12:30:00Z"),
        task("6", "rafa", "RUNNING", "2026-10-21T12:00:00Z", null),
        task("7", "rafa", "CANCELLED", "2026-10-21T12:00:00Z", "2026-10-21T12:01:00Z"),
      ],
      executions: [exec("carol", "2026-10-20T10:00:00Z", 0.5), exec("carol", "2026-10-21T12:00:00Z", null), exec("carol", "2026-10-01T12:00:00Z", 9)],
      approvals: [
        { dot_id: "carol", task_id: "1", requested_at: "2026-10-20T10:02:00Z" } as Approval,
        { dot_id: "carol", task_id: "1", requested_at: "2026-10-20T10:03:00Z" } as Approval,
        { dot_id: "carol", task_id: "4", requested_at: "2026-10-21T12:03:00Z" } as Approval,
        { dot_id: "rafa", task_id: "9", requested_at: "2026-10-02T12:03:00Z" } as Approval,
      ],
      audit: [
        audit("carol", "tool.dots_run_command", { outcome: "rejected" }),
        audit("carol", "tool.dots_git", { outcome: "ok", input: { op: "restore" } }),
        audit("carol", "tool.dots_git", { outcome: "ok", input: { op: "commit" } }),
        audit("carol", "tool.dots_write_file", { outcome: "denied" }, "2026-10-01T10:00:00Z"),
        { ...audit("rafa", "dot.update", { outcome: "rejected" }), actor: "user" },
      ],
    });
    expect(rows.map((r) => r.dotName)).toEqual(["Carolina", "Rafael"]);
    const c = rows[0]!;
    expect([c.finished, c.completed, c.failed, c.recurrent]).toEqual([4, 2, 2, 2]);
    expect(c.costUsd).toBe(0.5);
    expect(c.executions).toBe(2);
    expect(c.resolutionMs).toBe(20 * 60000);
    expect(c.reverted).toBe(2);
    expect(c.humanTasks).toBe(2);
    const r = rows[1]!;
    expect([r.finished, r.completed, r.failed, r.humanTasks, r.reverted]).toEqual([1, 0, 1, 0, 0]);
    const t = totalPerformance(rows);
    expect([t.finished, t.completed, t.costUsd, t.reverted]).toEqual([5, 2, 0.5, 2]);
    expect(t.resolutionMs).toBe(20 * 60000);
  });

  it("formata percentuais, custo e duracao", () => {
    expect(pct(2, 4)).toBe("50%");
    expect(pct(0, 0)).toBe("—");
    expect(usd(1.5, 3)).toBe("US$ 0.500");
    expect(usd(5, 2)).toBe("US$ 2.50");
    expect(usd(1, 0)).toBe("—");
    expect(duration(null)).toBe("—");
    expect(duration(20000)).toBe("20s");
    expect(duration(5 * 60000)).toBe("5 min");
    expect(duration(125 * 60000)).toBe("2h 05min");
    expect(isRevertedAction({ action: "tool.dots_git", details: { outcome: "ok", input: { op: "push" } } } as AuditLog)).toBe(false);
  });
});
