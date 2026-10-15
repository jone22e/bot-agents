//! Aplicativos conectados (Fase Flexi): conexoes nomeadas e vinculo por DOT.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    /// Sessao valida.
    Connected,
    /// Sessao expirada ou invalida; e preciso reconectar.
    NeedsLogin,
    /// 2FA pendente (challenge em andamento).
    MfaPending,
    /// Falha de rede ou servidor.
    Error,
}

impl ConnectionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionStatus::Connected => "connected",
            ConnectionStatus::NeedsLogin => "needs_login",
            ConnectionStatus::MfaPending => "mfa_pending",
            ConnectionStatus::Error => "error",
        }
    }
    pub fn parse(v: &str) -> Self {
        match v {
            "connected" => ConnectionStatus::Connected,
            "mfa_pending" => ConnectionStatus::MfaPending,
            "error" => ConnectionStatus::Error,
            _ => ConnectionStatus::NeedsLogin,
        }
    }
}

/// Conexao com um aplicativo externo (hoje: `flexi`). Segredos ficam no Keychain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppConnection {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub base_url: String,
    /// Empresa/servidor informado no login (ex.: "brasiltec").
    pub server: Option<String>,
    pub email: String,
    pub user_name: Option<String>,
    pub user_id: Option<i64>,
    pub empresa: Option<String>,
    pub base: Option<String>,
    pub status: ConnectionStatus,
    pub status_detail: Option<String>,
    /// Senha guardada no Keychain para reconectar sozinho.
    pub remember_password: bool,
    pub last_login_at: Option<Timestamp>,
    pub last_check_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FlexiConnectInput {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub label: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub base_url: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub server: Option<String>,
    pub email: String,
    pub senha: String,
    #[serde(default)]
    pub remember_password: bool,
}

/// Vinculo DOT -> conexao.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DotIntegration {
    pub dot_id: String,
    pub kind: String,
    pub connection_id: String,
    /// Papel (modelo) de origem, ex. `ml_creator`; usado para ligar a hierarquia de delegacao.
    pub role_template: Option<String>,
    /// Area de trabalho: `marketplaces`, `mercadolivre`, `shopee`, `googleads` ou `publicacao`.
    pub work_group: Option<String>,
    pub created_at: Timestamp,
}
