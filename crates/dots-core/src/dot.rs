use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DotRole, DotsError, DotsResult, EngineKind, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum DotStatus {
    Active,
    Paused,
    Archived,
}

impl DotStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DotStatus::Active => "active",
            DotStatus::Paused => "paused",
            DotStatus::Archived => "archived",
        }
    }

    pub fn parse(value: &str) -> DotsResult<Self> {
        match value {
            "active" => Ok(DotStatus::Active),
            "paused" => Ok(DotStatus::Paused),
            "archived" => Ok(DotStatus::Archived),
            other => Err(DotsError::Storage(format!("status de DOT invalido: {other}"))),
        }
    }
}

/// Limites operacionais de um DOT. Aplicados pelo runtime na Fase 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DotLimits {
    /// Tempo maximo de uma execucao, em segundos.
    pub max_execution_seconds: u32,
    /// Quantidade maxima de tarefas em execucao simultanea.
    pub max_concurrent_tasks: u32,
    /// Limite opcional de tokens por tarefa (quando o motor informar).
    #[ts(type = "number | null")]
    pub max_tokens_per_task: Option<u64>,
    /// Limite opcional de custo (USD) por tarefa (quando o motor informar).
    pub max_cost_usd_per_task: Option<f64>,
}

impl Default for DotLimits {
    fn default() -> Self {
        Self {
            max_execution_seconds: 1800,
            max_concurrent_tasks: 1,
            max_tokens_per_task: None,
            max_cost_usd_per_task: None,
        }
    }
}

impl DotLimits {
    pub fn validate(&self) -> DotsResult<()> {
        if self.max_execution_seconds == 0 {
            return Err(DotsError::Validation("max_execution_seconds deve ser maior que zero".into()));
        }
        if self.max_concurrent_tasks == 0 {
            return Err(DotsError::Validation("max_concurrent_tasks deve ser maior que zero".into()));
        }
        if let Some(c) = self.max_cost_usd_per_task {
            if !(c > 0.0) {
                return Err(DotsError::Validation("max_cost_usd_per_task deve ser positivo".into()));
            }
        }
        Ok(())
    }
}

/// Diretorio de trabalho associado a um DOT.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DotWorkspace {
    pub id: String,
    pub dot_id: String,
    pub path: String,
    pub is_primary: bool,
    pub can_read: bool,
    pub can_write: bool,
    pub is_external: bool,
    pub created_at: Timestamp,
}

/// Identidade persistente de um trabalhador digital.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Dot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub objective: String,
    /// Instrucoes permanentes (identidade). Versao atual.
    pub instructions: String,
    pub primary_engine: Option<EngineKind>,
    /// Modelo do motor principal (sobrepoe o padrao do motor); `None` usa o padrao.
    pub model: Option<String>,
    /// Caminho do workspace primario.
    pub workspace_path: Option<String>,
    pub limits: DotLimits,
    pub status: DotStatus,
    pub role: DotRole,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DotCreate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub objective: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub primary_engine: Option<EngineKind>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub model: Option<String>,
    /// Se omitido, o runtime cria `workspaces/<id>` no diretorio de dados.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub workspace_path: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub limits: Option<DotLimits>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub role: Option<DotRole>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DotUpdate {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub description: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub objective: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub instructions: Option<String>,
    /// `Some(None)` limpa o motor; `None` mantem.
    #[serde(default, with = "double_option")]
    #[ts(optional = nullable, as = "Option<EngineKind>")]
    pub primary_engine: Option<Option<EngineKind>>,
    /// `Some(None)` volta ao padrao do motor; `None` mantem (ou limpa, se o motor mudar).
    #[serde(default, with = "double_option")]
    #[ts(optional = nullable, as = "Option<String>")]
    pub model: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    #[ts(optional = nullable, as = "Option<String>")]
    pub workspace_path: Option<Option<String>>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub limits: Option<DotLimits>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub role: Option<DotRole>,
}

pub const MAX_NAME_LEN: usize = 80;
pub const MAX_TEXT_LEN: usize = 20_000;

pub fn validate_name(name: &str) -> DotsResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DotsError::Validation("nome e obrigatorio".into()));
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(DotsError::Validation(format!("nome excede {MAX_NAME_LEN} caracteres")));
    }
    Ok(trimmed.to_string())
}

pub fn validate_text(field: &str, value: &str) -> DotsResult<String> {
    if value.chars().count() > MAX_TEXT_LEN {
        return Err(DotsError::Validation(format!("{field} excede {MAX_TEXT_LEN} caracteres")));
    }
    Ok(value.to_string())
}

/// Suporte a `Option<Option<T>>` no serde: ausente = None, `null` = Some(None).
pub mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T, S>(value: &Option<Option<T>>, s: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        match value {
            Some(inner) => inner.serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(d).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validation() {
        assert!(validate_name("  ").is_err());
        assert_eq!(validate_name("  Revisor ").unwrap(), "Revisor");
        assert!(validate_name(&"x".repeat(81)).is_err());
    }

    #[test]
    fn update_double_option_roundtrip() {
        let absent: DotUpdate = serde_json::from_str(r#"{"name":"a"}"#).unwrap();
        assert_eq!(absent.primary_engine, None);
        let cleared: DotUpdate = serde_json::from_str(r#"{"primary_engine":null}"#).unwrap();
        assert_eq!(cleared.primary_engine, Some(None));
        let set: DotUpdate = serde_json::from_str(r#"{"primary_engine":"codex"}"#).unwrap();
        assert_eq!(set.primary_engine, Some(Some(EngineKind::Codex)));
    }

    #[test]
    fn limits_default_is_valid() {
        assert!(DotLimits::default().validate().is_ok());
        let bad = DotLimits { max_execution_seconds: 0, ..Default::default() };
        assert!(bad.validate().is_err());
    }
}
