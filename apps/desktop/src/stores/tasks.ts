import { defineStore } from "pinia";
import type { Checkpoint, Delegation, Execution, ExecutionEventRecord, Task, TaskStatus, WorkspaceChange } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

export const ACTIVE_STATUSES: TaskStatus[] = ["QUEUED", "RUNNING", "WAITING_APPROVAL", "WAITING_USER", "WAITING_EXTERNAL", "PAUSED"];

export const useTasksStore = defineStore("tasks", {
  state: () => ({
    items: [] as Task[],
    filterDot: null as string | null,
    onlyActive: false,
    loading: false,
    error: null as string | null,
    selectedId: null as string | null,
    selected: null as { task: Task; executions: Execution[] } | null,
    events: [] as ExecutionEventRecord[],
    changes: [] as WorkspaceChange[],
    children: [] as Task[],
    delegations: [] as Delegation[],
    checkpoints: [] as Checkpoint[],
    liveText: "",
    /** Tarefas ativas de todos os DOTs (para linhas de estado nos cartoes). */
    activeAll: [] as Task[],
  }),
  getters: {
    byId: (s) => (id: string) => s.items.find((t) => t.id === id) ?? null,
    activeForDot: (s) => (dotId: string) => s.activeAll.filter((t) => t.dot_id === dotId),
    active: (s) => s.items.filter((t) => ACTIVE_STATUSES.includes(t.status)),
  },
  actions: {
    async load() {
      this.loading = true;
      this.error = null;
      try {
        this.items = await api.tasks.list({ dot_id: this.filterDot ?? undefined, status: this.onlyActive ? ACTIVE_STATUSES : undefined, limit: 300 });
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.loading = false;
      }
    },
    async loadActive() {
      try { this.activeAll = await api.tasks.list({ status: ACTIVE_STATUSES, limit: 500 }); } catch { /* informativo */ }
    },
    async select(id: string | null) {
      this.selectedId = id;
      this.selected = null;
      this.events = [];
      this.changes = [];
      this.children = [];
      this.delegations = [];
      this.checkpoints = [];
      this.liveText = "";
      if (!id) return;
      try {
        this.selected = await api.tasks.get(id);
        const rel = await api.tasks.children(id);
        this.children = rel.children;
        this.delegations = rel.delegations;
        this.checkpoints = rel.checkpoints;
        const last = this.selected.executions.at(-1);
        if (last) {
          this.events = await api.executions.events(last.id);
          this.changes = await api.executions.changes(last.id);
        }
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
    async refreshSelected() {
      if (this.selectedId) await this.select(this.selectedId);
    },
    /** Evento ao vivo de uma execucao da tarefa selecionada. */
    onExecutionEvent(p: { execution_id: string; task_id: string; seq: number; event: ExecutionEventRecord["event"] }) {
      if (p.task_id !== this.selectedId) return;
      if (p.event.type === "text_delta") {
        this.liveText += p.event.text;
        return;
      }
      if (p.event.type === "text") this.liveText = p.event.text;
      if (p.seq > 0 && !this.events.some((e) => e.seq === p.seq && e.execution_id === p.execution_id)) {
        this.events.push({ id: `live-${p.execution_id}-${p.seq}`, execution_id: p.execution_id, seq: p.seq, event: p.event, created_at: new Date().toISOString() });
      }
    },
    async create(dotId: string, prompt: string, title?: string, retrySafe = false) {
      const t = await api.tasks.create({ dot_id: dotId, prompt, title: title ?? null, kind: "task", retry_safe: retrySafe, attachments: [], delegation_chain: [] });
      this.items.unshift(t);
      return t;
    },
    async cancel(id: string) {
      await api.tasks.cancel(id);
    },
    async retry(id: string) {
      const t = await api.tasks.retry(id);
      const i = this.items.findIndex((x) => x.id === id);
      if (i >= 0) this.items[i] = t;
    },
  },
});
