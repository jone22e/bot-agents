//! Fase 3: eventos, gatilhos, agendamentos, checkpoints e delegacao.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DotsError, DotsResult, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum DotRole {
    Worker,
    Supervisor,
}

impl DotRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            DotRole::Worker => "worker",
            DotRole::Supervisor => "supervisor",
        }
    }
    pub fn parse(v: &str) -> Self {
        if v == "supervisor" { DotRole::Supervisor } else { DotRole::Worker }
    }
}

impl Default for DotRole {
    fn default() -> Self {
        DotRole::Worker
    }
}

/// Tipos de evento emitidos pelo runtime.
pub mod event_types {
    pub const TASK_COMPLETED: &str = "task.completed";
    pub const TASK_FAILED: &str = "task.failed";
    pub const TOOL_FINISHED: &str = "tool.execution.finished";
    pub const USER_MESSAGE: &str = "user.message.received";
    pub const APPROVAL_GRANTED: &str = "approval.granted";
    pub const APPROVAL_REJECTED: &str = "approval.rejected";
    pub const DOT_TASK_REQUESTED: &str = "dot.task.requested";
    pub const DELEGATION_COMPLETED: &str = "delegation.completed";
    pub const FILE_CHANGED: &str = "file.changed";
    pub const GIT_COMMIT: &str = "git.commit.created";
    pub const SCHEDULE_DUE: &str = "schedule.due";
    /// Tipos que o usuario pode usar em gatilhos.
    pub const SUBSCRIBABLE: &[&str] = &[FILE_CHANGED, GIT_COMMIT, TASK_COMPLETED, TASK_FAILED, APPROVAL_GRANTED, APPROVAL_REJECTED];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EventRecord {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub source: String,
    pub occurred_at: Timestamp,
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
    pub correlation_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub depth: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubscriptionFilter {
    /// Padrao glob do caminho (file.changed): ex. `*.md`, `docs/**`.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub path_glob: Option<String>,
    /// Restringe a eventos originados por este DOT (task.*, approval.*).
    #[serde(default)]
    #[ts(optional = nullable)]
    pub source_dot_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EventSubscription {
    pub id: String,
    pub dot_id: String,
    pub event_type: String,
    pub filter: SubscriptionFilter,
    /// Pedido enviado ao DOT; aceita `{{path}}`, `{{type}}`, `{{payload}}`, `{{source}}`.
    pub prompt_template: String,
    pub retry_safe: bool,
    pub cooldown_seconds: u32,
    pub enabled: bool,
    pub last_triggered_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubscriptionCreate {
    pub dot_id: String,
    pub event_type: String,
    #[serde(default)]
    pub filter: SubscriptionFilter,
    pub prompt_template: String,
    #[serde(default)]
    pub retry_safe: bool,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub cooldown_seconds: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubscriptionUpdate {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub filter: Option<SubscriptionFilter>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub prompt_template: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub retry_safe: Option<bool>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub cooldown_seconds: Option<u32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleKind {
    /// `spec` = minutos entre execucoes.
    Every,
    /// `spec` = "HH:MM".
    Daily,
    /// `spec` = "DOW HH:MM" (0 = domingo).
    Weekly,
    /// `spec` = expressao cron de 5 campos.
    Cron,
}

impl ScheduleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScheduleKind::Every => "every",
            ScheduleKind::Daily => "daily",
            ScheduleKind::Weekly => "weekly",
            ScheduleKind::Cron => "cron",
        }
    }
    pub fn parse(v: &str) -> DotsResult<Self> {
        match v {
            "every" => Ok(ScheduleKind::Every),
            "daily" => Ok(ScheduleKind::Daily),
            "weekly" => Ok(ScheduleKind::Weekly),
            "cron" => Ok(ScheduleKind::Cron),
            o => Err(DotsError::Storage(format!("tipo de agendamento invalido: {o}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Schedule {
    pub id: String,
    pub dot_id: String,
    pub kind: ScheduleKind,
    pub spec: String,
    pub prompt: String,
    pub enabled: bool,
    pub next_run_at: Option<Timestamp>,
    pub last_run_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ScheduleCreate {
    pub dot_id: String,
    pub kind: ScheduleKind,
    pub spec: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ScheduleUpdate {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub kind: Option<ScheduleKind>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub spec: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub prompt: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Checkpoint {
    pub id: String,
    pub task_id: String,
    pub execution_id: String,
    pub step: String,
    #[ts(type = "unknown")]
    pub state: serde_json::Value,
    pub context: String,
    pub results: String,
    pub dependencies: Vec<String>,
    pub next_action: String,
    pub artifacts: Vec<String>,
    /// `auto` (runtime) ou `dot` (explicito pela ferramenta).
    pub source: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DelegationStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl DelegationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DelegationStatus::Pending => "pending",
            DelegationStatus::Running => "running",
            DelegationStatus::Completed => "completed",
            DelegationStatus::Failed => "failed",
            DelegationStatus::Cancelled => "cancelled",
        }
    }
    pub fn parse(v: &str) -> Self {
        match v {
            "running" => DelegationStatus::Running,
            "completed" => DelegationStatus::Completed,
            "failed" => DelegationStatus::Failed,
            "cancelled" => DelegationStatus::Cancelled,
            _ => DelegationStatus::Pending,
        }
    }
    pub fn is_terminal(&self) -> bool {
        matches!(self, DelegationStatus::Completed | DelegationStatus::Failed | DelegationStatus::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Delegation {
    pub id: String,
    pub from_dot_id: String,
    pub from_dot_name: String,
    pub to_dot_id: String,
    pub to_dot_name: String,
    pub parent_task_id: String,
    pub child_task_id: Option<String>,
    pub correlation_id: String,
    pub request: String,
    pub result: Option<String>,
    pub status: DelegationStatus,
    pub created_at: Timestamp,
    pub resolved_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalKind {
    Permission,
    Question,
}

impl ApprovalKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalKind::Permission => "permission",
            ApprovalKind::Question => "question",
        }
    }
    pub fn parse(v: &str) -> Self {
        if v == "question" { ApprovalKind::Question } else { ApprovalKind::Permission }
    }
}

/// Estatisticas da central de operacoes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineUsage {
    pub engine: String,
    pub executions: u32,
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
    pub cost_usd: f64,
    #[ts(type = "number")]
    pub total_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProcessSample {
    pub execution_id: String,
    pub dot_id: String,
    pub pid: u32,
    pub cpu_percent: f64,
    #[ts(type = "number")]
    pub rss_bytes: u64,
    #[ts(type = "number")]
    pub elapsed_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RuntimeStats {
    pub dots_active: u32,
    pub running: u32,
    pub queued: u32,
    pub blocked: u32,
    pub approvals_pending: u32,
    pub questions_pending: u32,
    pub failures_24h: u32,
    pub events_24h: u32,
    pub paused: bool,
    pub engine_usage: Vec<EngineUsage>,
    pub processes: Vec<ProcessSample>,
    pub last_error: Option<String>,
}
