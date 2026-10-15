use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Timestamp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AuditLog {
    pub id: String,
    /// Quem executou: "user", "runtime", "dot:<id>".
    pub actor: String,
    /// Acao em formato `entidade.verbo` (ex.: `dot.create`).
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    #[ts(type = "unknown")]
    pub details: serde_json::Value,
    pub created_at: Timestamp,
}
