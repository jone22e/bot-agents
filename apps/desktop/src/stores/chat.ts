import { defineStore } from "pinia";
import type { Approval, ExecutionEvent, Message } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export interface LiveState {
  taskId: string | null;
  text: string;
  tools: { name: string; ok: boolean | null }[];
  status: "idle" | "queued" | "running" | "waiting_approval";
}
const idle = (): LiveState => ({ taskId: null, text: "", tools: [], status: "idle" });

/** Conversa principal do DOT selecionado (uma thread por contato). */
export const useChatStore = defineStore("chat", {
  state: () => ({
    dotId: null as string | null,
    conversationId: null as string | null,
    messages: [] as Message[],
    live: idle() as LiveState,
    liveTasks: {} as Record<string, LiveState>,
    sending: false,
    error: null as string | null,
  }),
  getters: {
    busy: (s) => s.live.status !== "idle",
  },
  actions: {
    async open(dotId: string) {
      if (this.dotId === dotId && this.conversationId) {
        await this.reload();
        return;
      }
      this.dotId = dotId;
      this.conversationId = null;
      this.messages = [];
      this.live = idle();
      try {
        const conv = await api.conversations.main(dotId);
        this.conversationId = conv.id;
        await this.reload();
        const active = await api.tasks.list({ conversation_id: conv.id, status: ["QUEUED", "RUNNING", "WAITING_APPROVAL"], limit: 1 });
        if (active[0]) this.live = { ...idle(), taskId: active[0].id, status: active[0].status === "WAITING_APPROVAL" ? "waiting_approval" : active[0].status === "RUNNING" ? "running" : "queued" };
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
    async reload() {
      if (!this.conversationId) return;
      try { this.messages = await api.conversations.messages(this.conversationId); } catch { /* mantem */ }
    },
    async send(content: string, attachments: string[] = []) {
      if (!this.conversationId || !content.trim()) return;
      this.sending = true;
      this.error = null;
      try {
        const { message, task } = await api.conversations.send(this.conversationId, content.trim(), attachments);
        this.messages.push(message);
        this.live = { ...idle(), taskId: task.id, status: "queued" };
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.sending = false;
      }
    },
    async cancel() {
      if (this.live.taskId) await api.tasks.cancel(this.live.taskId);
    },
    onTaskChanged(p: { id: string; dot_id?: string; status: string }) {
      if (p.dot_id === this.dotId && p.status === "RUNNING" && this.live.status === "idle") {
        // Trabalho iniciado por gatilho/horario/outro DOT nesta conversa.
        this.live = { ...idle(), taskId: p.id, status: "running" };
        void this.reload();
      }
      if (p.id !== this.live.taskId) return;
      if (p.status === "RUNNING") this.live.status = "running";
      else if (p.status === "WAITING_APPROVAL") this.live.status = "waiting_approval";
      else if (["COMPLETED", "FAILED", "CANCELLED", "WAITING_USER", "QUEUED"].includes(p.status) && p.status !== "QUEUED") void this.finishLive();
    },
    onExecutionEvent(p: { task_id: string; dot_id: string; event: ExecutionEvent }) {
      if (p.dot_id === this.dotId && this.live.status === "idle") this.live = { ...idle(), taskId: p.task_id, status: "running" };
      if (p.task_id !== this.live.taskId) return;
      const ev = p.event;
      switch (ev.type) {
        case "text_delta": this.live.text += ev.text; break;
        case "text": this.live.text = ev.text; break;
        case "tool_call": this.live.tools.push({ name: ev.name.replace(/^mcp__dots__/, "").replace(/^dots\//, ""), ok: null }); break;
        case "tool_result": { const t = [...this.live.tools].reverse().find((x) => x.ok === null); if (t) t.ok = ev.ok; break; }
        case "permission_requested": this.live.status = "waiting_approval"; break;
        case "permission_resolved": this.live.status = "running"; break;
      }
    },
    async finishLive() {
      this.live = idle();
      await this.reload();
    },
    async onMessageChanged(p: { conversation_id?: string | null; dot_id?: string }) {
      if ((p.conversation_id && p.conversation_id === this.conversationId) || (p.dot_id && p.dot_id === this.dotId)) await this.reload();
    },
    pendingFor(approvals: Approval[]): Approval[] {
      return approvals.filter((a) => a.status === "pending" && (a.conversation_id === this.conversationId || (!a.conversation_id && a.dot_id === this.dotId)));
    },
  },
});
