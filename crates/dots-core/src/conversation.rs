use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DotsError, DotsResult, Timestamp};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Conversation {
    pub id: String,
    pub dot_id: String,
    pub title: String,
    /// Id de sessao por motor (`{"claude":"uuid","codex":"thread","agy":"conv"}`).
    #[ts(type = "Record<string, string>")]
    pub engine_sessions: serde_json::Value,
    pub message_count: u32,
    pub last_message: Option<String>,
    pub last_role: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Contato da lista do mensageiro: DOT + conversa principal + estado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Contact {
    pub dot: crate::Dot,
    pub conversation_id: String,
    pub last_message: Option<String>,
    pub last_role: Option<String>,
    pub last_at: Timestamp,
    /// idle | busy | attention
    pub state: String,
    pub state_text: String,
    pub pending_requests: u32,
    /// Cargo: nome do papel de origem (ex.: "Gerente de Google Ads"); nulo para DOTs sem papel.
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Dot,
    System,
    Tool,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Dot => "dot",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
        }
    }
    pub fn parse(v: &str) -> DotsResult<Self> {
        match v {
            "user" => Ok(MessageRole::User),
            "dot" => Ok(MessageRole::Dot),
            "system" => Ok(MessageRole::System),
            "tool" => Ok(MessageRole::Tool),
            o => Err(DotsError::Storage(format!("role invalido: {o}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,
    pub task_id: Option<String>,
    pub execution_id: Option<String>,
    #[ts(type = "unknown")]
    pub metadata: serde_json::Value,
    pub created_at: Timestamp,
}
