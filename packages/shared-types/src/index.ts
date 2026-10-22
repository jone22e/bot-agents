// Tipos gerados do Rust (ts-rs) + envelopes do IPC escritos a mao.
export type { AppConnection } from "./generated/AppConnection";
export type { Approval } from "./generated/Approval";
export type { ApprovalKind } from "./generated/ApprovalKind";
export type { ApprovalStatus } from "./generated/ApprovalStatus";
export type { AuditLog } from "./generated/AuditLog";
export type { AuthStatus } from "./generated/AuthStatus";
export type { AutoApproveLevel } from "./generated/AutoApproveLevel";
export type { Checkpoint } from "./generated/Checkpoint";
export type { CliUpdateInfo } from "./generated/CliUpdateInfo";
export type { CliUpdates } from "./generated/CliUpdates";
export type { ConnectionStatus } from "./generated/ConnectionStatus";
export type { Contact } from "./generated/Contact";
export type { Conversation } from "./generated/Conversation";
export type { Delegation } from "./generated/Delegation";
export type { Dismissal } from "./generated/Dismissal";
export type { DelegationStatus } from "./generated/DelegationStatus";
export type { Dot } from "./generated/Dot";
export type { DotCreate } from "./generated/DotCreate";
export type { DotIntegration } from "./generated/DotIntegration";
export type { DotLimits } from "./generated/DotLimits";
export type { DotRole } from "./generated/DotRole";
export type { DotStatus } from "./generated/DotStatus";
export type { DotTeamInfo } from "./generated/DotTeamInfo";
export type { DotUpdate } from "./generated/DotUpdate";
export type { DotWorkspace } from "./generated/DotWorkspace";
export type { DotsError } from "./generated/DotsError";
export type { EngineCapability } from "./generated/EngineCapability";
export type { EngineConfiguration } from "./generated/EngineConfiguration";
export type { EngineDetection } from "./generated/EngineDetection";
export type { EngineKind } from "./generated/EngineKind";
export type { EngineModel } from "./generated/EngineModel";
export type { EngineUsage } from "./generated/EngineUsage";
export type { EventRecord } from "./generated/EventRecord";
export type { EventSubscription } from "./generated/EventSubscription";
export type { Execution } from "./generated/Execution";
export type { ExecutionEvent } from "./generated/ExecutionEvent";
export type { ExecutionEventRecord } from "./generated/ExecutionEventRecord";
export type { ExecutionStatus } from "./generated/ExecutionStatus";
export type { FailureKind } from "./generated/FailureKind";
export type { GroupMessage } from "./generated/GroupMessage";
export type { LearnedPermission } from "./generated/LearnedPermission";
export type { FlexiConnectInput } from "./generated/FlexiConnectInput";
export type { Installation } from "./generated/Installation";
export type { Memory } from "./generated/Memory";
export type { MemoryCreate } from "./generated/MemoryCreate";
export type { MemoryKind } from "./generated/MemoryKind";
export type { MemoryUpdate } from "./generated/MemoryUpdate";
export type { Notification } from "./generated/Notification";
export type { Message } from "./generated/Message";
export type { MessageRole } from "./generated/MessageRole";
export type { ProcessSample } from "./generated/ProcessSample";
export type { ProviderUsage } from "./generated/ProviderUsage";
export type { RiskLevel } from "./generated/RiskLevel";
export type { RuntimeMode } from "./generated/RuntimeMode";
export type { RuntimeStats } from "./generated/RuntimeStats";
export type { RuntimeStatus } from "./generated/RuntimeStatus";
export type { Schedule } from "./generated/Schedule";
export type { ScheduleCreate } from "./generated/ScheduleCreate";
export type { ScheduleKind } from "./generated/ScheduleKind";
export type { ScheduleUpdate } from "./generated/ScheduleUpdate";
export type { Setting } from "./generated/Setting";
export type { SubscriptionCreate } from "./generated/SubscriptionCreate";
export type { SubscriptionFilter } from "./generated/SubscriptionFilter";
export type { SubscriptionUpdate } from "./generated/SubscriptionUpdate";
export type { Task } from "./generated/Task";
export type { TaskCreate } from "./generated/TaskCreate";
export type { TaskKind } from "./generated/TaskKind";
export type { TaskStatus } from "./generated/TaskStatus";
export type { Team } from "./generated/Team";
export type { TeamCreate } from "./generated/TeamCreate";
export type { TeamUpdate } from "./generated/TeamUpdate";
export type { ToolPolicy } from "./generated/ToolPolicy";
export type { ToolPolicyUpdate } from "./generated/ToolPolicyUpdate";
export type { UsageInfo } from "./generated/UsageInfo";
export type { UsageWindow } from "./generated/UsageWindow";
export type { WorkspaceChange } from "./generated/WorkspaceChange";

/** Evento push do runtime (frame `{event, payload, seq}`). */
export interface RuntimeEvent {
  event: string;
  payload: unknown;
  seq: number;
}

/** Conteudo de `<data_dir>/runtime.json`, publicado pelo dotsd. */
export interface RuntimeInfo {
  pid: number;
  version: string;
  proto: number;
  socket_path: string;
  token_path: string;
  started_at: string;
  mode: "launchd" | "detached" | "manual" | string;
}

export interface LaunchdStatus {
  supported: boolean;
  plist_installed: boolean;
  loaded: boolean;
  pid: number | null;
  last_exit_status: number | null;
}

export interface AppPaths {
  start_at_login: boolean;
  data_dir: string;
  logs_dir: string;
  bin_dir: string;
  launch_agent_plist: string;
  bundled_dotsd: string | null;
  dev_mode: boolean;
}

/** Resposta de `overview.get`. */
export interface Overview {
  dots_total: number;
  dots_active: number;
  dots_paused: number;
  dots_archived: number;
  dots_running: number;
  tasks_pending: number;
  tasks_running: number;
  executions_recent: import("./generated/Execution").Execution[];
  failures: number;
  approvals_pending: number;
  engines: import("./generated/EngineConfiguration").EngineConfiguration[];
  runtime: import("./generated/RuntimeStatus").RuntimeStatus;
  paused: boolean;
}

/** Payload do evento `execution.event`. */
export interface ExecutionEventPayload {
  execution_id: string;
  task_id: string;
  dot_id: string;
  seq: number;
  event: import("./generated/ExecutionEvent").ExecutionEvent;
}

/** Resposta de `engines.agy_status|agy_setup|agy_remove`. */
export interface AgySetupStatus {
  settings_path: string;
  server_registered: boolean;
  server_command: string | null;
  rules_present: string[];
  rules_missing: string[];
  configured: boolean;
}

/** Especialidade de gerente do Flexi (`apps.flexi.templates`): o gerente contrata os funcionarios e define o que cada um faz. */
export interface FlexiRoleTemplate {
  id: string;
  name: string;
  /** Sempre "supervisor": especialidade e de gerente. */
  role: "worker" | "supervisor";
  trust: "none" | "read" | "write" | "exec";
  purpose: string;
  /** Setor do sistema no Flexi (ex.: mercadolivre, compras). */
  group: string;
  /** Nome do setor (ex.: "Mercado Livre"). */
  group_name: string;
  /** Nome da equipe que este gerente gerencia (o nome do setor). */
  manages: string | null;
}

