use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DotsError, DotsResult, EngineKind, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Queued,
    Running,
    WaitingUser,
    WaitingApproval,
    WaitingExternal,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Queued => "QUEUED",
            TaskStatus::Running => "RUNNING",
            TaskStatus::WaitingUser => "WAITING_USER",
            TaskStatus::WaitingApproval => "WAITING_APPROVAL",
            TaskStatus::WaitingExternal => "WAITING_EXTERNAL",
            TaskStatus::Paused => "PAUSED",
            TaskStatus::Completed => "COMPLETED",
            TaskStatus::Failed => "FAILED",
            TaskStatus::Cancelled => "CANCELLED",
        }
    }
    pub fn parse(v: &str) -> DotsResult<Self> {
        Ok(match v {
            "QUEUED" => TaskStatus::Queued,
            "RUNNING" => TaskStatus::Running,
            "WAITING_USER" => TaskStatus::WaitingUser,
            "WAITING_APPROVAL" => TaskStatus::WaitingApproval,
            "WAITING_EXTERNAL" => TaskStatus::WaitingExternal,
            "PAUSED" => TaskStatus::Paused,
            "COMPLETED" => TaskStatus::Completed,
            "FAILED" => TaskStatus::Failed,
            "CANCELLED" => TaskStatus::Cancelled,
            o => return Err(DotsError::Storage(format!("status de tarefa invalido: {o}"))),
        })
    }
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
    }
    pub fn is_active(&self) -> bool {
        matches!(self, TaskStatus::Running | TaskStatus::WaitingApproval)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    /// Mensagem de conversa (continuidade com a conversa).
    Chat,
    /// Trabalho autonomo solicitado.
    Task,
}

impl TaskKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Chat => "chat",
            TaskKind::Task => "task",
        }
    }
    pub fn parse(v: &str) -> DotsResult<Self> {
        match v {
            "chat" => Ok(TaskKind::Chat),
            "task" => Ok(TaskKind::Task),
            o => Err(DotsError::Storage(format!("kind de tarefa invalido: {o}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Task {
    pub id: String,
    pub dot_id: String,
    pub conversation_id: Option<String>,
    pub kind: TaskKind,
    pub title: String,
    pub prompt: String,
    pub status: TaskStatus,
    pub priority: i32,
    pub source: String,
    pub correlation_id: Option<String>,
    /// Motor usado (snapshot no momento do enfileiramento).
    pub engine: Option<EngineKind>,
    /// Pode ser reenfileirada automaticamente apos interrupcao do runtime.
    pub retry_safe: bool,
    pub cancel_requested: bool,
    pub result_text: Option<String>,
    pub error: Option<String>,
    pub attempts: u32,
    pub max_attempts: u32,
    /// `user`, `event:<id>`, `delegation:<id>`, `schedule:<id>`, `followup:<task_id>`.
    pub origin: String,
    pub parent_task_id: Option<String>,
    /// DOTs na cadeia de delegacao (do mais externo ao atual).
    pub delegation_chain: Vec<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TaskCreate {
    pub dot_id: String,
    pub prompt: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub title: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub kind: Option<TaskKind>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub priority: Option<i32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub retry_safe: Option<bool>,
    /// Caminhos de arquivos anexados (devem estar em diretorios autorizados).
    #[serde(default)]
    pub attachments: Vec<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub origin: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub delegation_chain: Vec<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub max_attempts: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Timeout,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Running => "RUNNING",
            ExecutionStatus::Completed => "COMPLETED",
            ExecutionStatus::Failed => "FAILED",
            ExecutionStatus::Cancelled => "CANCELLED",
            ExecutionStatus::Timeout => "TIMEOUT",
        }
    }
    pub fn parse(v: &str) -> DotsResult<Self> {
        Ok(match v {
            "RUNNING" => ExecutionStatus::Running,
            "COMPLETED" => ExecutionStatus::Completed,
            "FAILED" => ExecutionStatus::Failed,
            "CANCELLED" => ExecutionStatus::Cancelled,
            "TIMEOUT" => ExecutionStatus::Timeout,
            o => return Err(DotsError::Storage(format!("status de execucao invalido: {o}"))),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Execution {
    pub id: String,
    pub task_id: String,
    pub dot_id: String,
    pub engine: EngineKind,
    pub attempt: u32,
    pub status: ExecutionStatus,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub engine_session_id: Option<String>,
    pub workspace_path: Option<String>,
    pub error: Option<String>,
    pub events_count: u32,
    pub cost_usd: Option<f64>,
    #[ts(type = "unknown")]
    pub usage: serde_json::Value,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}

/// Evento normalizado de execucao, igual para todos os motores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionEvent {
    Started { engine_session_id: Option<String> },
    /// Trecho parcial de texto do assistente (streaming).
    TextDelta { text: String },
    /// Bloco de texto completo do assistente.
    Text { text: String },
    Reasoning { text: String },
    ToolCall { call_id: String, name: String, #[ts(type = "unknown")] input: serde_json::Value },
    ToolResult { call_id: String, name: String, ok: bool, output: String },
    PermissionRequested { approval_id: String, tool: String, risk: RiskLevel },
    PermissionResolved { approval_id: String, approved: bool },
    Usage {
        #[ts(type = "number | null")]
        input_tokens: Option<u64>,
        #[ts(type = "number | null")]
        output_tokens: Option<u64>,
        cost_usd: Option<f64>,
    },
    Notice { level: String, text: String },
    Completed { result_text: String },
    Failed { kind: FailureKind, message: String },
    /// Linha nao reconhecida do motor; preservada para diagnostico.
    Raw { line: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Auth,
    Config,
    Engine,
    Timeout,
    Cancelled,
    Budget,
    Unauthorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Read,
    Write,
    Exec,
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Read => "read",
            RiskLevel::Write => "write",
            RiskLevel::Exec => "exec",
            RiskLevel::Critical => "critical",
        }
    }
    pub fn parse(v: &str) -> Self {
        match v {
            "read" => RiskLevel::Read,
            "write" => RiskLevel::Write,
            "exec" => RiskLevel::Exec,
            _ => RiskLevel::Critical,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionEventRecord {
    pub id: String,
    pub execution_id: String,
    #[ts(type = "number")]
    pub seq: u64,
    pub event: ExecutionEvent,
    pub created_at: Timestamp,
}
