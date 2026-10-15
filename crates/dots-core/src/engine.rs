use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DotsError, DotsResult, Timestamp};

/// Motores de IA suportados. A identidade do DOT nao depende do motor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Codex,
    Claude,
    Agy,
}

impl EngineKind {
    pub const ALL: [EngineKind; 3] = [EngineKind::Codex, EngineKind::Claude, EngineKind::Agy];

    pub fn as_str(&self) -> &'static str {
        match self {
            EngineKind::Codex => "codex",
            EngineKind::Claude => "claude",
            EngineKind::Agy => "agy",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            EngineKind::Codex => "OpenAI Codex CLI",
            EngineKind::Claude => "Anthropic Claude Code",
            EngineKind::Agy => "Google Antigravity (AGY)",
        }
    }

    /// Nome do executavel no sistema.
    pub fn executable_name(&self) -> &'static str {
        self.as_str()
    }

    pub fn parse(value: &str) -> DotsResult<Self> {
        match value {
            "codex" => Ok(EngineKind::Codex),
            "claude" => Ok(EngineKind::Claude),
            "agy" => Ok(EngineKind::Agy),
            other => Err(DotsError::Validation(format!("motor desconhecido: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    Authenticated,
    Unauthenticated,
    /// O CLI nao oferece meio nao interativo de verificar.
    Unknown,
}

impl AuthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthStatus::Authenticated => "authenticated",
            AuthStatus::Unauthenticated => "unauthenticated",
            AuthStatus::Unknown => "unknown",
        }
    }
    pub fn parse(v: &str) -> Self {
        match v {
            "authenticated" => AuthStatus::Authenticated,
            "unauthenticated" => AuthStatus::Unauthenticated,
            _ => AuthStatus::Unknown,
        }
    }
}

/// Capacidade verificada no `--help` da versao instalada. Nunca presumida.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineCapability {
    pub id: String,
    pub label: String,
    /// Flag ou subcomando que comprova a capacidade.
    pub evidence: String,
}

/// Modelo que o proprio CLI oferece: catalogo local do Codex (`~/.codex/models_cache.json`)
/// ou a saida de `agy models`. O Claude Code nao lista modelos; a lista dele fica na interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineModel {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

/// Resultado de uma deteccao do CLI no sistema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineDetection {
    pub kind: EngineKind,
    pub display_name: String,
    pub installed: bool,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    pub auth_status: AuthStatus,
    /// Explicacao legivel do status de autenticacao (ex.: "Logged in using ChatGPT").
    pub auth_detail: Option<String>,
    pub capabilities: Vec<EngineCapability>,
    /// Modelos informados pelo CLI na deteccao (vazio quando o CLI nao lista).
    #[serde(default)]
    pub models: Vec<EngineModel>,
    pub issues: Vec<String>,
    /// Orientacao ao usuario quando ausente ou com problema.
    pub guidance: Option<String>,
    pub checked_at: Timestamp,
}

/// Configuracao persistida de um motor (deteccao + override do usuario).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineConfiguration {
    pub kind: EngineKind,
    pub enabled: bool,
    /// Caminho fixado manualmente pelo usuario; sobrepoe a deteccao.
    pub executable_override: Option<String>,
    /// Modelo padrao passado ao CLI (`--model`); `None` usa o padrao do proprio CLI.
    pub model: Option<String>,
    pub last_detection: Option<EngineDetection>,
    pub updated_at: Timestamp,
}

pub const MAX_MODEL_LEN: usize = 100;

/// Normaliza um nome de modelo vindo da interface: vazio vira `None`; espacos e tamanho sao rejeitados
/// porque o valor vai direto como argumento de linha de comando.
pub fn validate_model(model: Option<&str>) -> DotsResult<Option<String>> {
    let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) else { return Ok(None) };
    if m.chars().count() > MAX_MODEL_LEN {
        return Err(DotsError::Validation(format!("modelo excede {MAX_MODEL_LEN} caracteres")));
    }
    if m.chars().any(char::is_whitespace) {
        return Err(DotsError::Validation("modelo nao pode conter espacos".into()));
    }
    Ok(Some(m.to_string()))
}

#[cfg(test)]
mod tests {
    use super::validate_model;

    #[test]
    fn model_is_trimmed_and_rejects_whitespace() {
        assert_eq!(validate_model(None).unwrap(), None);
        assert_eq!(validate_model(Some("   ")).unwrap(), None);
        assert_eq!(validate_model(Some(" gpt-5 ")).unwrap().as_deref(), Some("gpt-5"));
        assert!(validate_model(Some("a b")).is_err());
        assert!(validate_model(Some(&"x".repeat(101))).is_err());
    }
}
