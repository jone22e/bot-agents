//! Metas: o usuario declara um alvo numerico na conversa ("10 envios por dia ate sexta"); o gerente registra a
//! meta (`dots_goal_create`) e lanca o progresso a cada rodada (`dots_goal_progress`). O Flexi web acompanha.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DotsError, DotsResult, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum GoalStatus {
    /// Em andamento.
    Active,
    /// Alvo atingido ou encerrada como cumprida.
    Done,
    /// Encerrada sem atingir o alvo.
    Cancelled,
}

impl GoalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoalStatus::Active => "active",
            GoalStatus::Done => "done",
            GoalStatus::Cancelled => "cancelled",
        }
    }
    pub fn parse(v: &str) -> DotsResult<Self> {
        match v {
            "active" => Ok(GoalStatus::Active),
            "done" => Ok(GoalStatus::Done),
            "cancelled" => Ok(GoalStatus::Cancelled),
            o => Err(DotsError::Storage(format!("status de meta invalido: {o}"))),
        }
    }
}

/// Uma meta de um DOT (normalmente o gerente da equipe).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Goal {
    pub id: String,
    pub dot_id: String,
    /// Frase curta do que se quer (ex.: "Envios ao Merchant Center").
    pub title: String,
    /// Unidade do numero (ex.: "envios", "R$", "%"); vazio = sem unidade.
    pub unit: String,
    /// Alvo a atingir.
    pub target: f64,
    /// Valor atual (ultimo lancamento).
    pub current: f64,
    pub period_start: Timestamp,
    /// Prazo; `None` = sem prazo.
    pub period_end: Option<Timestamp>,
    pub status: GoalStatus,
    /// Tarefa em que a meta foi registrada.
    pub task_id: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub closed_at: Option<Timestamp>,
}

/// Um lancamento de progresso.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GoalUpdate {
    pub id: String,
    pub goal_id: String,
    /// Valor acumulado depois do lancamento.
    pub value: f64,
    /// Variacao em relacao ao lancamento anterior.
    pub delta: f64,
    pub note: String,
    pub task_id: Option<String>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GoalCreate {
    pub dot_id: String,
    pub title: String,
    #[serde(default)]
    pub unit: String,
    pub target: f64,
    #[serde(default)]
    pub current: f64,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub period_end: Option<Timestamp>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub task_id: Option<String>,
}
