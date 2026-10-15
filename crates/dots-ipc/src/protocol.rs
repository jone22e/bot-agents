use dots_core::DotsError;
use serde::{Deserialize, Serialize};

/// Limite por linha (4 MB). Linhas maiores encerram a conexao.
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub data: serde_json::Value,
}

impl From<DotsError> for ErrorBody {
    fn from(e: DotsError) -> Self {
        ErrorBody { code: e.code().to_string(), message: e.message().to_string(), data: serde_json::Value::Null }
    }
}

impl From<ErrorBody> for DotsError {
    fn from(e: ErrorBody) -> Self {
        DotsError::from_code(&e.code, e.message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Response {
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        Response { id, result: Some(result), error: None }
    }
    pub fn err(id: u64, e: DotsError) -> Self {
        Response { id, result: None, error: Some(e.into()) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    pub seq: u64,
}

/// Qualquer linha recebida. A ordem importa: `Event` tem `event`,
/// `Request` tem `method`, `Response` tem `id` sem `method`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Frame {
    Event(Event),
    Request(Request),
    Response(Response),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloParams {
    pub token: String,
    pub client: String,
    pub proto: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloResult {
    pub pid: u32,
    pub version: String,
    pub proto: u32,
    pub uptime_seconds: u64,
}

pub const METHOD_HELLO: &str = "hello";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_discrimination() {
        let f: Frame = serde_json::from_str(r#"{"event":"x","payload":{},"seq":1}"#).unwrap();
        assert!(matches!(f, Frame::Event(_)));
        let f: Frame = serde_json::from_str(r#"{"id":1,"method":"m","params":{}}"#).unwrap();
        assert!(matches!(f, Frame::Request(_)));
        let f: Frame = serde_json::from_str(r#"{"id":1,"result":{}}"#).unwrap();
        assert!(matches!(f, Frame::Response(_)));
        let f: Frame = serde_json::from_str(r#"{"id":1,"error":{"code":"NOT_FOUND","message":"x"}}"#).unwrap();
        assert!(matches!(f, Frame::Response(Response { error: Some(_), .. })));
    }
}
