//! Equipes: um gerente (DOT) delega aos funcionarios (DOTs). Equipes se encaixam: o gerente de uma pode ser
//! funcionario de outra. A politica `can_delegate_to` do gerente e derivada dos membros.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Timestamp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub description: String,
    pub manager_dot_id: Option<String>,
    pub member_ids: Vec<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TeamCreate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub manager_dot_id: Option<String>,
    #[serde(default)]
    pub member_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TeamUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Situacao de um DOT nas equipes: de qual e funcionario e qual gerencia.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DotTeamInfo {
    pub member_of: Option<Team>,
    pub manages: Option<Team>,
}

/// Demissao de um funcionario pelo gerente. O DOT continua existindo (mesmo id) com a memoria zerada;
/// motivo e licao ficam registrados para que a equipe nao repita o erro.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Dismissal {
    pub id: String,
    pub team_id: Option<String>,
    pub team_name: String,
    pub manager_dot_id: Option<String>,
    pub manager_name: String,
    pub dot_id: Option<String>,
    pub dot_name: String,
    pub reason: String,
    /// O que o proximo funcionario deve fazer diferente (derivado do motivo quando o gerente nao informa).
    pub lesson: String,
    /// Quantas memorias foram apagadas na demissao.
    pub memories_wiped: u32,
    pub created_at: Timestamp,
}
