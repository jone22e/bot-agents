import type { Approval, AuditLog, Execution, Task } from "@dots/shared-types";

/**
 * Avaliacao de desempenho: indicadores por DOT calculados a partir das tarefas encerradas no periodo,
 * das execucoes (custo), das aprovacoes (intervencao humana) e da auditoria de ferramentas (acoes revertidas).
 */
export interface DotPerformance {
  dotId: string;
  dotName: string;
  /** Tarefas encerradas no periodo (concluidas, falhas e canceladas). */
  finished: number;
  completed: number;
  failed: number;
  /** Tarefas que precisaram de mais de uma tentativa ou cujo titulo ja tinha falhado antes no periodo. */
  recurrent: number;
  /** Custo somado das execucoes iniciadas no periodo (USD). */
  costUsd: number;
  executions: number;
  /** Mediana do tempo entre criacao e encerramento das tarefas concluidas, em ms. */
  resolutionMs: number | null;
  /** Acoes negadas pela politica, rejeitadas pelo usuario ou desfeitas via git (restore, reset, clean, stash). */
  reverted: number;
  /** Tarefas em que o DOT precisou de permissao ou resposta sua. */
  humanTasks: number;
}

export interface PerformanceInput {
  tasks: Task[];
  executions: Execution[];
  approvals: Approval[];
  audit: AuditLog[];
  dotNames: Map<string, string>;
  start: Date;
  end: Date;
}

const REVERT_GIT_OPS = new Set(["restore", "reset", "clean", "stash"]);

function inPeriod(iso: string | null, start: Date, end: Date) {
  if (!iso) return false;
  const t = Date.parse(iso);
  return t >= start.getTime() && t < end.getTime();
}

function median(values: number[]) {
  if (!values.length) return null;
  const s = [...values].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  return s.length % 2 ? s[mid]! : (s[mid - 1]! + s[mid]!) / 2;
}

/** Entrada de auditoria que representa uma acao revertida: bloqueada/rejeitada ou desfeita via git. */
export function isRevertedAction(a: AuditLog): boolean {
  if (!a.action.startsWith("tool.")) return false;
  const d = (a.details ?? {}) as { outcome?: string; input?: { op?: string } };
  if (d.outcome === "rejected" || d.outcome === "denied") return true;
  return a.action === "tool.dots_git" && d.outcome === "ok" && REVERT_GIT_OPS.has(d.input?.op ?? "");
}

export function computePerformance(input: PerformanceInput): DotPerformance[] {
  const { start, end } = input;
  const rows = new Map<string, DotPerformance>();
  const row = (dotId: string) => {
    let r = rows.get(dotId);
    if (!r) {
      r = { dotId, dotName: input.dotNames.get(dotId) ?? "(removido)", finished: 0, completed: 0, failed: 0, recurrent: 0, costUsd: 0, executions: 0, resolutionMs: null, reverted: 0, humanTasks: 0 };
      rows.set(dotId, r);
    }
    return r;
  };
  const durations = new Map<string, number[]>();
  const failedTitles = new Map<string, Set<string>>();
  const finishedTasks = input.tasks
    .filter((t) => (t.status === "COMPLETED" || t.status === "FAILED" || t.status === "CANCELLED") && inPeriod(t.finished_at ?? t.updated_at, start, end))
    .sort((a, b) => (a.finished_at ?? a.updated_at).localeCompare(b.finished_at ?? b.updated_at));
  for (const t of finishedTasks) {
    const r = row(t.dot_id);
    r.finished += 1;
    const seen = failedTitles.get(t.dot_id) ?? new Set<string>();
    failedTitles.set(t.dot_id, seen);
    const title = t.title.trim().toLowerCase();
    if (t.attempts > 1 || (title && seen.has(title))) r.recurrent += 1;
    if (t.status === "COMPLETED") {
      r.completed += 1;
      const ms = Date.parse(t.finished_at ?? t.updated_at) - Date.parse(t.created_at);
      if (Number.isFinite(ms) && ms >= 0) durations.set(t.dot_id, [...(durations.get(t.dot_id) ?? []), ms]);
    } else {
      r.failed += 1;
      if (title) seen.add(title);
    }
  }
  for (const [dotId, list] of durations) row(dotId).resolutionMs = median(list);
  for (const e of input.executions) {
    if (!inPeriod(e.started_at, start, end)) continue;
    const r = row(e.dot_id);
    r.executions += 1;
    r.costUsd += e.cost_usd ?? 0;
  }
  const humanByDot = new Map<string, Set<string>>();
  for (const a of input.approvals) {
    if (!inPeriod(a.requested_at, start, end)) continue;
    const set = humanByDot.get(a.dot_id) ?? new Set<string>();
    set.add(a.task_id);
    humanByDot.set(a.dot_id, set);
  }
  for (const [dotId, set] of humanByDot) row(dotId).humanTasks = set.size;
  for (const a of input.audit) {
    if (!a.actor.startsWith("dot:") || !inPeriod(a.created_at, start, end) || !isRevertedAction(a)) continue;
    row(a.actor.slice(4)).reverted += 1;
  }
  return [...rows.values()].sort((a, b) => b.finished - a.finished || a.dotName.localeCompare(b.dotName));
}

/** Soma de todos os DOTs (a mediana de tempo vira a mediana das medianas, aproximacao suficiente para o painel). */
export function totalPerformance(rows: DotPerformance[]): DotPerformance {
  const t: DotPerformance = { dotId: "*", dotName: "Todos", finished: 0, completed: 0, failed: 0, recurrent: 0, costUsd: 0, executions: 0, resolutionMs: null, reverted: 0, humanTasks: 0 };
  for (const r of rows) {
    t.finished += r.finished; t.completed += r.completed; t.failed += r.failed; t.recurrent += r.recurrent;
    t.costUsd += r.costUsd; t.executions += r.executions; t.reverted += r.reverted; t.humanTasks += r.humanTasks;
  }
  t.resolutionMs = median(rows.map((r) => r.resolutionMs).filter((v): v is number => v !== null));
  return t;
}

export const pct = (part: number, total: number) => (total ? `${Math.round((part / total) * 100)}%` : "—");
export const usd = (v: number, per = 1) => (per ? `US$ ${(v / per).toFixed(v / per < 1 ? 3 : 2)}` : "—");
export function duration(ms: number | null) {
  if (ms === null) return "—";
  const min = Math.round(ms / 60000);
  if (min < 1) return `${Math.round(ms / 1000)}s`;
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  return h < 24 ? `${h}h ${String(min % 60).padStart(2, "0")}min` : `${Math.round(h / 24)} dia(s)`;
}
