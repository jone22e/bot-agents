// Traduz eventos, aprovacoes e estados tecnicos em frases para o usuario.
import type { Approval, EngineKind, EventSubscription, ExecutionEvent, Schedule, Task, TaskStatus } from "@dots/shared-types";

export const ENGINE_LABEL: Record<EngineKind, string> = { codex: "Codex (OpenAI)", claude: "Claude Code (Anthropic)", agy: "Antigravity (Google)" };

export const TASK_STATUS_LABEL: Record<TaskStatus | "CANCELLING", string> = {
  QUEUED: "Na fila",
  RUNNING: "Trabalhando",
  WAITING_USER: "Precisa de voce",
  WAITING_APPROVAL: "Esperando sua permissao",
  WAITING_EXTERNAL: "Esperando algo externo",
  PAUSED: "Pausado",
  COMPLETED: "Concluido",
  FAILED: "Nao deu certo",
  CANCELLED: "Cancelado",
  CANCELLING: "Cancelando",
};

function fileName(p: string) {
  return p.split("/").filter(Boolean).pop() ?? p;
}

function shortPath(p: string) {
  const parts = p.split("/").filter(Boolean);
  return parts.length > 2 ? "…/" + parts.slice(-2).join("/") : p;
}

/** Nome curto da ferramenta sem prefixo tecnico. */
export function toolName(name: string) {
  return name.replace(/^mcp__dots__/, "").replace(/^dots\//, "").replace(/^dots_/, "");
}

/** Frase para uma chamada de ferramenta (dados de entrada). */
export function describeToolCall(name: string, input: unknown): string {
  const i = (input ?? {}) as Record<string, unknown>;
  const s = (k: string) => (typeof i[k] === "string" ? (i[k] as string) : "");
  const args = (i.Arguments ?? {}) as Record<string, unknown>;
  const a = (k: string) => (typeof args[k] === "string" ? (args[k] as string) : "");
  switch (toolName(name)) {
    case "write_file": return `Escreveu o arquivo ${fileName(s("path") || a("path"))}`;
    case "read_file": return `Leu o arquivo ${fileName(s("path") || a("path"))}`;
    case "list_dir": return `Olhou a pasta ${shortPath(s("path") || a("path") || ".")}`;
    case "search": return `Procurou "${s("pattern") || a("pattern")}" em ${shortPath(s("path") || a("path") || ".")}`;
    case "delete_file": return `Excluiu o arquivo ${fileName(s("path") || a("path"))}`;
    case "run_command": return `Rodou o comando "${s("command") || a("command")}"`;
    case "git": return `Usou git (${s("op") || a("op")})`;
    case "system_info": return "Consultou informacoes do sistema";
    case "remember": return "Guardou uma memoria";
    case "schedule_create": { const k = s("kind") || a("kind"), sp = s("spec") || a("spec"); return `Agendou ${k === "every" ? `a cada ${sp} min` : k === "daily" ? `todo dia às ${sp}` : k === "weekly" ? `toda semana (${sp})` : `cron ${sp}`}`; }
    case "schedule_list": return "Consultou os agendamentos";
    case "schedule_cancel": return "Cancelou um agendamento";
    case "recall": return `Procurou na memoria por "${s("query") || a("query")}"`;
    case "dismiss_employee": return `Demitiu ${s("dot") || a("dot")}: ${s("reason") || a("reason")}`;
    case "hire_employee": return `Contratou ${s("name") || a("name")} como ${s("role") || a("role")}`;
    case "command_execution": return `Rodou o comando "${s("command")}"`;
    case "file_change": return "Alterou arquivos";
    case "Bash": return `Rodou o comando "${s("command")}"`;
    case "Edit": case "Write": case "MultiEdit": return `Editou o arquivo ${fileName(s("file_path"))}`;
    case "Read": return `Leu o arquivo ${fileName(s("file_path"))}`;
    case "view_file": return `Leu o arquivo ${fileName(s("AbsolutePath"))}`;
    case "call_mcp_tool": return describeToolCall(String(i.ToolName ?? ""), args);
    default: return `Usou a ferramenta ${toolName(name)}`;
  }
}

/** Frase para um evento de execucao; `null` quando nao interessa ao usuario. */
export function describeEvent(e: ExecutionEvent): { text: string; tone: "normal" | "ok" | "warn" | "error" } | null {
  switch (e.type) {
    case "started": return { text: "Comecou a trabalhar", tone: "normal" };
    case "tool_call": return { text: describeToolCall(e.name, e.input), tone: "normal" };
    case "tool_result": return e.ok ? null : { text: `Acao negada ou com erro: ${e.output.slice(0, 140)}`, tone: "warn" };
    case "permission_requested": return { text: "Pediu sua permissao", tone: "warn" };
    case "permission_resolved": return { text: e.approved ? "Voce permitiu" : "Voce negou", tone: e.approved ? "ok" : "warn" };
    case "notice": return e.level === "warn" ? { text: e.text, tone: "warn" } : null;
    case "completed": return { text: "Terminou", tone: "ok" };
    case "failed": return { text: describeFailure(e.kind, e.message), tone: "error" };
    default: return null;
  }
}

export function describeFailure(kind: string, message: string) {
  switch (kind) {
    case "auth": return "O assistente de IA nao esta conectado. Abra o Terminal e faca login nele.";
    case "timeout": return "Passou do tempo limite e foi interrompido.";
    case "cancelled": return "Cancelado por voce.";
    case "budget": return "Atingiu o limite de gasto definido para este DOT.";
    case "config": return `Problema de configuracao: ${message}`;
    default: return `Nao deu certo: ${message.slice(0, 200)}`;
  }
}

/** Frase para um pedido de permissao: "quer criar o arquivo x.txt". */
export function describeApproval(a: Approval): string {
  const i = (a.input ?? {}) as Record<string, unknown>;
  const s = (k: string) => (typeof i[k] === "string" ? (i[k] as string) : "");
  const res = a.resources[0] ?? "";
  switch (toolName(a.tool)) {
    case "write_file": return `quer escrever o arquivo ${fileName(res || s("path"))}`;
    case "delete_file": return `quer excluir o arquivo ${fileName(res || s("path"))}`;
    case "run_command": case "Bash": return `quer rodar o comando "${s("command")}"`;
    case "git": return `quer executar git ${s("op")} ${((i.args as string[]) ?? []).join(" ")}`.trim();
    case "Edit": case "Write": case "MultiEdit": return `quer editar o arquivo ${fileName(s("file_path"))}`;
    case "WebFetch": case "WebSearch": return `quer acessar a internet (${s("url") || s("query")})`;
    case "dismiss_employee": return `quer demitir ${s("dot")} (ele sera excluido; depois o gerente contrata outro). Motivo: ${s("reason")}`;
    default: return a.summary;
  }
}

export const RISK_LABEL: Record<string, string> = { read: "so leitura", write: "altera arquivos", exec: "executa comandos", critical: "acao irreversivel" };

/** Linha de estado de um DOT a partir das tarefas ativas. */
export function dotStateLine(active: Task[]): { text: string; tone: "idle" | "busy" | "attention" } {
  const waiting = active.find((t) => t.status === "WAITING_APPROVAL" || t.status === "WAITING_USER");
  if (waiting) return { text: waiting.status === "WAITING_APPROVAL" ? `Esperando sua permissao: ${waiting.title}` : `Precisa de voce: ${waiting.title}`, tone: "attention" };
  const running = active.find((t) => t.status === "RUNNING");
  if (running) return { text: `Trabalhando em: ${running.title}`, tone: "busy" };
  const queued = active.filter((t) => t.status === "QUEUED").length;
  if (queued) return { text: `${queued} na fila`, tone: "busy" };
  return { text: "Livre", tone: "idle" };
}

/** Separa "o que ele faz" em objetivo (primeira frase) e instrucoes completas. */
export function splitPurpose(text: string): { objective: string; instructions: string } {
  const t = text.trim();
  const first = t.split(/(?<=[.!?\n])\s+/)[0] ?? t;
  return { objective: first.slice(0, 160), instructions: t };
}

/** De onde veio um trabalho, em frase. */
export function describeOrigin(t: Task, dotName?: (id: string) => string): string {
  const [kind] = t.origin.split(":");
  switch (kind) {
    case "user": return t.kind === "chat" ? "Voce, na conversa" : "Voce pediu";
    case "event": return "Um gatilho disparou";
    case "schedule": return "Horario agendado";
    case "delegation": return t.delegation_chain.length && dotName ? `Pedido de ${dotName(t.delegation_chain[t.delegation_chain.length - 1])}` : "Pedido de outro DOT";
    case "followup": return "Continuacao (resultados ou resposta chegaram)";
    default: return t.origin;
  }
}

export const EVENT_TYPE_LABEL: Record<string, string> = {
  "file.changed": "um arquivo mudar",
  "git.commit.created": "um commit for criado",
  "task.completed": "outro DOT concluir um trabalho",
  "task.failed": "outro DOT falhar em um trabalho",
  "approval.granted": "voce permitir uma acao",
  "approval.rejected": "voce negar uma acao",
};

export function describeSubscription(s: EventSubscription, dotName?: (id: string) => string): string {
  let when = `Quando ${EVENT_TYPE_LABEL[s.event_type] ?? s.event_type}`;
  if (s.filter.path_glob) when += ` (${s.filter.path_glob})`;
  if (s.filter.source_dot_id) when += ` de ${dotName ? dotName(s.filter.source_dot_id) : "um DOT"}`;
  return `${when}, pedir: "${s.prompt_template}"`;
}

export function describeSchedule(s: Schedule): string {
  const names = ["domingo", "segunda", "terca", "quarta", "quinta", "sexta", "sabado"];
  switch (s.kind) {
    case "every": return `A cada ${s.spec} min`;
    case "daily": return `Todo dia as ${s.spec}`;
    case "weekly": { const [d, h] = s.spec.split(" "); return `Toda ${names[Number(d)] ?? "?"} as ${h}`; }
    case "cron": return `Cron ${s.spec}`;
  }
}
