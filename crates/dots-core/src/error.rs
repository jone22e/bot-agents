use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Erro de dominio. Serializado no IPC como `{code, message, data}`.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "code", content = "message", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DotsError {
    #[error("{0} nao encontrado")]
    NotFound(String),
    #[error("validacao: {0}")]
    Validation(String),
    #[error("conflito: {0}")]
    Conflict(String),
    #[error("armazenamento: {0}")]
    Storage(String),
    #[error("io: {0}")]
    Io(String),
    #[error("motor: {0}")]
    Engine(String),
    #[error("nao autorizado: {0}")]
    Unauthorized(String),
    #[error("runtime indisponivel: {0}")]
    RuntimeUnavailable(String),
    #[error("protocolo: {0}")]
    Protocol(String),
    #[error("interno: {0}")]
    Internal(String),
}

impl DotsError {
    pub fn code(&self) -> &'static str {
        match self {
            DotsError::NotFound(_) => "NOT_FOUND",
            DotsError::Validation(_) => "VALIDATION",
            DotsError::Conflict(_) => "CONFLICT",
            DotsError::Storage(_) => "STORAGE",
            DotsError::Io(_) => "IO",
            DotsError::Engine(_) => "ENGINE",
            DotsError::Unauthorized(_) => "UNAUTHORIZED",
            DotsError::RuntimeUnavailable(_) => "RUNTIME_UNAVAILABLE",
            DotsError::Protocol(_) => "PROTOCOL",
            DotsError::Internal(_) => "INTERNAL",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            DotsError::NotFound(m)
            | DotsError::Validation(m)
            | DotsError::Conflict(m)
            | DotsError::Storage(m)
            | DotsError::Io(m)
            | DotsError::Engine(m)
            | DotsError::Unauthorized(m)
            | DotsError::RuntimeUnavailable(m)
            | DotsError::Protocol(m)
            | DotsError::Internal(m) => m,
        }
    }

    pub fn from_code(code: &str, message: impl Into<String>) -> Self {
        let m = message.into();
        match code {
            "NOT_FOUND" => DotsError::NotFound(m),
            "VALIDATION" => DotsError::Validation(m),
            "CONFLICT" => DotsError::Conflict(m),
            "STORAGE" => DotsError::Storage(m),
            "IO" => DotsError::Io(m),
            "ENGINE" => DotsError::Engine(m),
            "UNAUTHORIZED" => DotsError::Unauthorized(m),
            "RUNTIME_UNAVAILABLE" => DotsError::RuntimeUnavailable(m),
            "PROTOCOL" => DotsError::Protocol(m),
            _ => DotsError::Internal(m),
        }
    }
}

impl From<std::io::Error> for DotsError {
    fn from(e: std::io::Error) -> Self {
        DotsError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for DotsError {
    fn from(e: serde_json::Error) -> Self {
        DotsError::Protocol(e.to_string())
    }
}

pub type DotsResult<T> = Result<T, DotsError>;
