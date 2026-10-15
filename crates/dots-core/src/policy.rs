use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{RiskLevel, Timestamp};

/// Nivel maximo de risco aprovado automaticamente (acima disso pede aprovacao).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AutoApproveLevel {
    None,
    Read,
    Write,
    Exec,
}

impl AutoApproveLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutoApproveLevel::None => "none",
            AutoApproveLevel::Read => "read",
            AutoApproveLevel::Write => "write",
            AutoApproveLevel::Exec => "exec",
        }
    }
    pub fn parse(v: &str) -> Self {
        match v {
            "none" => AutoApproveLevel::None,
            "write" => AutoApproveLevel::Write,
            "exec" => AutoApproveLevel::Exec,
            _ => AutoApproveLevel::Read,
        }
    }
    /// `true` se o risco informado dispensa aprovacao humana.
    pub fn covers(&self, risk: RiskLevel) -> bool {
        match (self, risk) {
            (_, RiskLevel::Critical) => false,
            (AutoApproveLevel::None, _) => false,
            (AutoApproveLevel::Read, r) => r <= RiskLevel::Read,
            (AutoApproveLevel::Write, r) => r <= RiskLevel::Write,
            (AutoApproveLevel::Exec, r) => r <= RiskLevel::Exec,
        }
    }
}

/// Politica de ferramentas de um DOT. Avaliada pelo runtime, nunca pela LLM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolPolicy {
    pub dot_id: String,
    pub auto_approve_level: AutoApproveLevel,
    /// Prefixos de comando permitidos em `dots_run_command` (ex.: "npm test", "git status").
    pub allowed_commands: Vec<String>,
    pub allow_network: bool,
    /// Tamanho maximo de arquivo lido/escrito pelo gateway.
    #[ts(type = "number")]
    pub max_file_bytes: u64,
    /// Permite que o motor use suas ferramentas internas de escrita (sandbox proprio do CLI).
    pub engine_native_write: bool,
    /// Ids de DOTs para os quais pode delegar; `["*"]` = todos. Vazio = nao delega.
    pub can_delegate_to: Vec<String>,
    pub max_delegation_depth: u32,
    pub updated_at: Timestamp,
}

impl ToolPolicy {
    pub fn default_for(dot_id: &str) -> Self {
        Self {
            dot_id: dot_id.to_string(),
            auto_approve_level: AutoApproveLevel::Read,
            allowed_commands: vec![],
            allow_network: false,
            max_file_bytes: 2 * 1024 * 1024,
            engine_native_write: false,
            can_delegate_to: vec![],
            max_delegation_depth: 2,
            updated_at: crate::now(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolPolicyUpdate {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub auto_approve_level: Option<AutoApproveLevel>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub allowed_commands: Option<Vec<String>>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub allow_network: Option<bool>,
    #[serde(default)]
    #[ts(optional = nullable, type = "number | null")]
    pub max_file_bytes: Option<u64>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub engine_native_write: Option<bool>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub can_delegate_to: Option<Vec<String>>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub max_delegation_depth: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
            ApprovalStatus::Expired => "expired",
        }
    }
    pub fn parse(v: &str) -> Self {
        match v {
            "approved" => ApprovalStatus::Approved,
            "rejected" => ApprovalStatus::Rejected,
            "expired" => ApprovalStatus::Expired,
            _ => ApprovalStatus::Pending,
        }
    }
}

/// Solicitacao de aprovacao de uma acao critica.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Approval {
    pub id: String,
    pub kind: crate::ApprovalKind,
    /// Opcoes de resposta (perguntas do Supervisor).
    pub options: Vec<String>,
    pub execution_id: String,
    pub task_id: String,
    pub conversation_id: Option<String>,
    pub dot_id: String,
    pub dot_name: String,
    pub tool: String,
    #[ts(type = "unknown")]
    pub input: serde_json::Value,
    pub risk: RiskLevel,
    /// Recursos afetados (caminhos, comando, repositorio).
    pub resources: Vec<String>,
    pub summary: String,
    pub status: ApprovalStatus,
    pub requested_at: Timestamp,
    pub expires_at: Timestamp,
    pub resolved_at: Option<Timestamp>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceChange {
    pub id: String,
    pub execution_id: String,
    pub dot_id: String,
    pub path: String,
    /// create | modify | delete | command | git
    pub kind: String,
    pub summary: String,
    pub created_at: Timestamp,
}
