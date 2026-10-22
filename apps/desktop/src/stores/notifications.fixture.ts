import type { Notification } from "@dots/shared-types";

/** Aviso pendente de teste. */
export const notice = (id: string, dotId: string, name: string, title: string, extra: Partial<Notification> = {}): Notification => ({
  id, dot_id: dotId, dot_name: name, task_id: "t1", conversation_id: "c1", title, detail: "", action: "", status: "pending",
  created_at: "2026-10-22T08:00:00Z", cleared_at: null, ...extra,
});
