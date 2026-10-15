use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DotsError, DotsResult, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// Instrucoes permanentes e responsabilidades (derivada do DOT).
    Identity,
    /// Estado da tarefa atual: etapas, decisoes, resultados intermediarios.
    Working,
    /// Conhecimento preservado entre execucoes.
    LongTerm,
    /// Resumo de tarefas, erros e resultados passados.
    History,
}

impl MemoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryKind::Identity => "identity",
            MemoryKind::Working => "working",
            MemoryKind::LongTerm => "long_term",
            MemoryKind::History => "history",
        }
    }
    pub fn parse(v: &str) -> DotsResult<Self> {
        match v {
            "identity" => Ok(MemoryKind::Identity),
            "working" => Ok(MemoryKind::Working),
            "long_term" => Ok(MemoryKind::LongTerm),
            "history" => Ok(MemoryKind::History),
            o => Err(DotsError::Storage(format!("kind de memoria invalido: {o}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Memory {
    pub id: String,
    pub dot_id: String,
    pub kind: MemoryKind,
    pub content: String,
    pub importance: i32,
    pub source: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MemoryCreate {
    pub dot_id: String,
    pub kind: MemoryKind,
    pub content: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub importance: Option<i32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub source: Option<String>,
}

impl Default for MemoryKind {
    fn default() -> Self {
        MemoryKind::LongTerm
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MemoryUpdate {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub content: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub importance: Option<i32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub kind: Option<MemoryKind>,
}
