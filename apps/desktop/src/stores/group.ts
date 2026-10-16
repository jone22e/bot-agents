import { defineStore } from "pinia";
import type { Dot, ExecutionEvent, GroupMessage } from "@dots/shared-types";
import { api, RuntimeError } from "@/api";

/** Grupo geral: o usuario escreve uma vez e todos os gerentes recebem; as respostas deles chegam aqui. */
export const useGroupStore = defineStore("group", {
  state: () => ({
    members: [] as Dot[],
    messages: [] as GroupMessage[],
    /** Texto parcial das respostas em andamento, por tarefa. */
    live: {} as Record<string, string>,
    /** Tarefas com eventos ja conferidas (se nao sao do grupo, nao recarregam de novo). */
    checked: {} as Record<string, true>,
    /** A tela do grupo esta aberta (so entao o texto parcial das respostas e acompanhado). */
    open: false,
    sending: false,
    reloading: false,
    error: null as string | null,
  }),
  getters: {
    isMember: (s) => (dotId: string | undefined) => !!dotId && s.members.some((d) => d.id === dotId),
    /** Ultima mensagem do grupo (para a linha na lista de contatos). */
    last: (s) => [...s.messages].reverse().find((m) => !(m.metadata as { streaming?: boolean } | null)?.streaming) ?? null,
  },
  actions: {
    async loadMembers() {
      try { this.members = await api.group.members(); } catch { /* mantem */ }
    },
    async load() {
      try {
        const [members, messages] = await Promise.all([api.group.members(), api.group.messages()]);
        this.members = members;
        this.messages = messages;
        const streaming = new Set(messages.filter((m) => (m.metadata as { streaming?: boolean } | null)?.streaming && m.task_id).map((m) => m.task_id!));
        for (const id of Object.keys(this.live)) if (!streaming.has(id)) delete this.live[id];
        this.error = null;
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      }
    },
    /** `mentions`: ids dos gerentes marcados com @ (so eles respondem; os outros recebem para ciencia). */
    async send(content: string, mentions: string[] = []) {
      if (!content.trim()) return;
      this.sending = true;
      this.error = null;
      try {
        const { message } = await api.group.send(content.trim(), mentions);
        this.messages.push(message);
      } catch (e) {
        this.error = e instanceof RuntimeError ? e.message : String(e);
      } finally {
        this.sending = false;
      }
    },
    onExecutionEvent(p: { task_id: string; dot_id: string; event: ExecutionEvent }) {
      if (!this.open || !this.isMember(p.dot_id)) return;
      // Resposta que ainda nao esta na lista (comecou agora): recarrega uma vez por tarefa para ela aparecer
      // (trabalhos do gerente fora do grupo continuam fora).
      if (!this.messages.some((m) => m.task_id === p.task_id)) {
        if (!this.checked[p.task_id]) { this.checked[p.task_id] = true; void this.onChanged(); }
        return;
      }
      if (p.event.type === "text_delta") this.live[p.task_id] = (this.live[p.task_id] ?? "") + p.event.text;
      else if (p.event.type === "text") this.live[p.task_id] = p.event.text;
    },
    /** Mensagem, tarefa ou grupo mudou: recarrega (sem sobrepor recargas; a ultima mensagem aparece na lista de contatos). */
    async onChanged() {
      if (this.reloading) return;
      this.reloading = true;
      try { await this.load(); } finally { this.reloading = false; }
    },
  },
});
