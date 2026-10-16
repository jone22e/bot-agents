import type {
  AgySetupStatus, AppConnection, Approval, CliUpdateInfo, CliUpdates, Dismissal, UsageInfo, DotIntegration, DotTeamInfo, FlexiConnectInput, FlexiRoleTemplate, Team, TeamCreate, TeamUpdate, Contact, Checkpoint, Delegation, EventRecord, EventSubscription, RuntimeStats, Schedule, ScheduleCreate, ScheduleUpdate, SubscriptionCreate, SubscriptionUpdate, AuditLog, Conversation, Dot, DotCreate, DotUpdate, DotWorkspace, EngineConfiguration, EngineDetection,
  EngineKind, Execution, ExecutionEventRecord, GroupMessage, Installation, LearnedPermission, Memory, MemoryCreate, MemoryKind, MemoryUpdate, Message, Overview,
  RuntimeStatus, Setting, Task, TaskCreate, TaskStatus, ToolPolicy, ToolPolicyUpdate, WorkspaceChange,
} from "@dots/shared-types";
import { ipcCall } from "./ipc";

export { ipcCall, mediaApi, runtimeApi, RuntimeError } from "./ipc";

/** Pedido de "Logar com Flexi" (IPC `apps.flexi.browser_start`): abrir `authorize_url` no navegador. */
export interface FlexiBrowserLogin {
  token: string;
  code: string;
  authorize_url: string;
  expires_at: string | null;
  poll_interval_ms: number;
}

/** Estado do navegador proprio de um DOT (IPC `browser.status`). */
export interface BrowserStatus {
  running: boolean;
  watching: boolean;
  url: string;
  title: string;
  binary: string | null;
}
/** Quadro do screencast (evento `browser.frame`). */
export interface BrowserFrame {
  dot_id: string;
  data: string;
  width: number;
  height: number;
}

export const api = {
  runtime: {
    status: () => ipcCall<RuntimeStatus>("runtime.status"),
    shutdown: () => ipcCall<{ ok: boolean }>("runtime.shutdown"),
    backup: () => ipcCall<{ path: string }>("runtime.backup"),
    backups: () => ipcCall<string[]>("runtime.backups"),
    pauseAll: (paused: boolean) => ipcCall<{ paused: boolean }>("runtime.pause_all", { paused }),
    logs: (lines = 200) => ipcCall<string[]>("runtime.logs", { lines }),
  },
  stats: { get: () => ipcCall<RuntimeStats>("stats.get") },
  events: { list: (event_type?: string, limit = 100) => ipcCall<EventRecord[]>("events.list", { event_type: event_type ?? null, limit }) },
  subscriptions: {
    list: (dot_id?: string) => ipcCall<EventSubscription[]>("subscriptions.list", dot_id ? { dot_id } : {}),
    create: (input: SubscriptionCreate) => ipcCall<EventSubscription>("subscriptions.create", input),
    update: (id: string, patch: SubscriptionUpdate) => ipcCall<EventSubscription>("subscriptions.update", { id, patch }),
    delete: (id: string) => ipcCall<{ ok: boolean }>("subscriptions.delete", { id }),
  },
  schedules: {
    list: (dot_id?: string) => ipcCall<Schedule[]>("schedules.list", dot_id ? { dot_id } : {}),
    create: (input: ScheduleCreate) => ipcCall<Schedule>("schedules.create", input),
    update: (id: string, patch: ScheduleUpdate) => ipcCall<Schedule>("schedules.update", { id, patch }),
    delete: (id: string) => ipcCall<{ ok: boolean }>("schedules.delete", { id }),
  },
  delegations: { list: (limit = 100) => ipcCall<Delegation[]>("delegations.list", { limit }) },
  installation: { get: () => ipcCall<Installation | null>("installation.get") },
  overview: { get: () => ipcCall<Overview>("overview.get") },
  settings: {
    list: () => ipcCall<Setting[]>("settings.list"),
    get: (key: string) => ipcCall<Setting | null>("settings.get", { key }),
    set: (key: string, value: unknown) => ipcCall<Setting>("settings.set", { key, value }),
  },
  browser: {
    status: (dot_id: string) => ipcCall<BrowserStatus>("browser.status", { dot_id }),
    watch: (dot_id: string) => ipcCall<BrowserStatus & { started: boolean }>("browser.watch", { dot_id }),
    unwatch: (dot_id: string) => ipcCall<{ ok: boolean }>("browser.unwatch", { dot_id }),
  },
  dots: {
    list: (includeArchived = false) => ipcCall<Dot[]>("dots.list", { include_archived: includeArchived }),
    get: (id: string) => ipcCall<Dot>("dots.get", { id }),
    startAutonomy: (id: string) => ipcCall<{ ok: boolean }>("dots.start_autonomy", { id }),
    create: (input: DotCreate) => ipcCall<Dot>("dots.create", input),
    update: (id: string, patch: DotUpdate) => ipcCall<Dot>("dots.update", { id, patch }),
    duplicate: (id: string, name?: string) => ipcCall<Dot>("dots.duplicate", { id, name: name ?? null }),
    pause: (id: string) => ipcCall<Dot>("dots.pause", { id }),
    resume: (id: string) => ipcCall<Dot>("dots.resume", { id }),
    archive: (id: string) => ipcCall<Dot>("dots.archive", { id }),
    delete: (id: string) => ipcCall<{ ok: boolean }>("dots.delete", { id }),
    workspaces: (id: string) => ipcCall<DotWorkspace[]>("dots.workspaces", { id }),
  },
  workspaces: {
    add: (dot_id: string, path: string, can_read: boolean, can_write: boolean) => ipcCall<DotWorkspace>("workspaces.add", { dot_id, path, can_read, can_write }),
    remove: (dot_id: string, path: string) => ipcCall<{ ok: boolean }>("workspaces.remove", { dot_id, path }),
  },
  engines: {
    list: () => ipcCall<EngineConfiguration[]>("engines.list"),
    detect: (kind?: EngineKind) => ipcCall<EngineDetection[]>("engines.detect", kind ? { kind } : {}),
    setOverride: (kind: EngineKind, path: string | null) => ipcCall<EngineDetection[]>("engines.set_override", { kind, path }),
    setEnabled: (kind: EngineKind, enabled: boolean) => ipcCall<EngineConfiguration>("engines.set_enabled", { kind, enabled }),
    setModel: (kind: EngineKind, model: string | null) => ipcCall<EngineConfiguration>("engines.set_model", { kind, model }),
    agyStatus: () => ipcCall<AgySetupStatus>("engines.agy_status"),
    agySetup: () => ipcCall<AgySetupStatus>("engines.agy_setup"),
    agyRemove: () => ipcCall<AgySetupStatus>("engines.agy_remove"),
  },
  tasks: {
    list: (f: { dot_id?: string; status?: TaskStatus[]; conversation_id?: string; limit?: number } = {}) => ipcCall<Task[]>("tasks.list", f),
    get: (id: string) => ipcCall<{ task: Task; executions: Execution[] }>("tasks.get", { id }),
    create: (input: TaskCreate) => ipcCall<Task>("tasks.create", input),
    cancel: (id: string) => ipcCall<{ ok: boolean; signalled: boolean }>("tasks.cancel", { id }),
    retry: (id: string) => ipcCall<Task>("tasks.retry", { id }),
    children: (id: string) => ipcCall<{ children: Task[]; delegations: Delegation[]; checkpoints: Checkpoint[] }>("tasks.children", { id }),
  },
  executions: {
    get: (id: string) => ipcCall<Execution>("executions.get", { id }),
    recent: (limit = 50) => ipcCall<Execution[]>("executions.recent", { limit }),
    events: (execution_id: string, after_seq = 0, limit = 2000) => ipcCall<ExecutionEventRecord[]>("executions.events", { execution_id, after_seq, limit }),
    changes: (id: string) => ipcCall<WorkspaceChange[]>("executions.changes", { id }),
  },
  contacts: { list: () => ipcCall<Contact[]>("contacts.list") },
  conversations: {
    main: (dot_id: string) => ipcCall<Conversation>("conversations.main", { dot_id }),
    list: (dot_id?: string) => ipcCall<Conversation[]>("conversations.list", dot_id ? { dot_id } : {}),
    create: (dot_id: string, title?: string) => ipcCall<Conversation>("conversations.create", { dot_id, title: title ?? null }),
    get: (id: string) => ipcCall<Conversation>("conversations.get", { id }),
    delete: (id: string) => ipcCall<{ ok: boolean }>("conversations.delete", { id }),
    messages: (id: string, limit = 500) => ipcCall<Message[]>("conversations.messages", { id, limit }),
    send: (conversation_id: string, content: string, attachments: string[] = []) => ipcCall<{ message: Message; task: Task }>("conversations.send", { conversation_id, content, attachments }),
    /** Guarda um anexo onde o DOT pode ler: arquivo do computador (`path`) ou bytes colados (`data`, base64). */
    attach: (conversation_id: string, name: string, src: { path: string } | { data: string }) => ipcCall<{ path: string; name: string; size: number }>("conversations.attach", { conversation_id, name, ...src }),
  },
  /** Grupo geral: o usuario com todos os gerentes. */
  group: {
    members: () => ipcCall<Dot[]>("group.members"),
    messages: (limit = 300) => ipcCall<GroupMessage[]>("group.messages", { limit }),
    send: (content: string, mentions: string[] = []) => ipcCall<{ message: GroupMessage; tasks: Task[] }>("group.send", { content, mentions }),
  },
  approvals: {
    list: (pendingOnly = false, limit = 200) => ipcCall<Approval[]>("approvals.list", { pending_only: pendingOnly, limit }),
    decide: (id: string, approved: boolean, reason?: string) => ipcCall<Approval>("approvals.decide", { id, approved, reason: reason ?? null }),
  },
  memories: {
    list: (dot_id: string, kind?: MemoryKind, query?: string, limit = 200) => ipcCall<Memory[]>("memories.list", { dot_id, kind: kind ?? null, query: query ?? null, limit }),
    create: (input: MemoryCreate) => ipcCall<Memory>("memories.create", input),
    update: (id: string, patch: MemoryUpdate) => ipcCall<Memory>("memories.update", { id, patch }),
    delete: (id: string) => ipcCall<{ ok: boolean }>("memories.delete", { id }),
  },
  policies: {
    get: (dot_id: string) => ipcCall<ToolPolicy>("policies.get", { dot_id }),
    set: (dot_id: string, patch: ToolPolicyUpdate) => ipcCall<ToolPolicy>("policies.set", { dot_id, patch }),
    /** Tipos de acao aprovados uma vez, que o DOT nao pede de novo. */
    learned: (dot_id: string) => ipcCall<LearnedPermission[]>("policies.learned", { dot_id }),
    forget: (id: string) => ipcCall<{ ok: boolean }>("policies.forget", { id }),
  },
  audit: { list: (limit = 100) => ipcCall<AuditLog[]>("audit.list", { limit }) },
  dismissals: {
    /** Demissoes: por equipe, por DOT demitido, por gerente ou as mais recentes. */
    list: (f: { team_id?: string; dot_id?: string; manager_dot_id?: string; limit?: number } = {}) => ipcCall<Dismissal[]>("dismissals.list", f),
  },
  /** Uso das assinaturas e versoes dos CLIs (como no ovrgit). */
  usage: {
    get: (force = false) => ipcCall<UsageInfo>("usage.get", { force }),
    cliUpdates: (force = false) => ipcCall<CliUpdates>("cli.updates", { force }),
    cliUpdate: (kind: EngineKind) => ipcCall<CliUpdateInfo | null>("cli.update", { kind }),
    cliInstall: (kind: EngineKind) => ipcCall<CliUpdateInfo | null>("cli.install", { kind }),
  },
  teams: {
    list: () => ipcCall<Team[]>("teams.list"),
    create: (input: TeamCreate) => ipcCall<Team>("teams.create", input),
    update: (id: string, patch: TeamUpdate) => ipcCall<Team>("teams.update", { id, patch }),
    remove: (id: string) => ipcCall<{ ok: boolean }>("teams.delete", { id }),
    setManager: (id: string, dot_id: string | null) => ipcCall<Team>("teams.set_manager", { id, dot_id }),
    addMember: (id: string, dot_id: string) => ipcCall<Team>("teams.add_member", { id, dot_id }),
    removeMember: (dot_id: string) => ipcCall<Team | null>("teams.remove_member", { dot_id }),
    ofDot: (dot_id: string) => ipcCall<DotTeamInfo>("dots.team", { dot_id }),
  },
  apps: {
    list: () => ipcCall<AppConnection[]>("apps.list"),
    flexiConnect: (input: FlexiConnectInput) => ipcCall<AppConnection>("apps.flexi.connect", input),
    flexiReconnect: (id: string, senha?: string, remember_password?: boolean) => ipcCall<AppConnection>("apps.flexi.reconnect", { id, senha: senha ?? null, remember_password: remember_password ?? null }),
    flexiMfa: (id: string, code: string) => ipcCall<AppConnection>("apps.flexi.mfa", { id, code }),
    flexiCheck: (id: string) => ipcCall<AppConnection>("apps.flexi.check", { id }),
    flexiRemove: (id: string) => ipcCall<{ ok: boolean }>("apps.flexi.remove", { id }),
    flexiBrowserStart: (base_url?: string | null) => ipcCall<FlexiBrowserLogin>("apps.flexi.browser_start", { base_url: base_url ?? null }),
    flexiBrowserPoll: (token: string) => ipcCall<{ status: string; connection?: AppConnection }>("apps.flexi.browser_poll", { token }),
    flexiBrowserCancel: (token: string) => ipcCall<{ ok: boolean }>("apps.flexi.browser_cancel", { token }),
    flexiPhoto: (id: string) => ipcCall<{ data_url: string | null }>("apps.flexi.photo", { id }),
    flexiTemplates: () => ipcCall<FlexiRoleTemplate[]>("apps.flexi.templates"),
    flexiCreateTeam: (id: string) => ipcCall<{ created: { template: string; dot_id: string }[] }>("apps.flexi.create_team", { id }),
    integration: (dot_id: string, kind = "flexi") => ipcCall<DotIntegration | null>("dots.integration", { dot_id, kind }),
    assignRole: (dot_id: string, connection_id: string, opts: { template_id?: string | null; group: string | null; role: "worker" | "supervisor" }) =>
      ipcCall<DotIntegration>("apps.flexi.assign_role", { dot_id, connection_id, template_id: opts.template_id ?? null, group: opts.group, role: opts.role }),
    setIntegration: (dot_id: string, connection_id: string | null, kind = "flexi") => ipcCall<DotIntegration | null>("dots.set_integration", { dot_id, kind, connection_id }),
  },
};
