use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// Supervisionado pelo launchd (LaunchAgent).
    Launchd,
    /// Processo iniciado diretamente (dev ou fallback).
    Detached,
    /// Iniciado manualmente pelo usuario no terminal.
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RuntimeStatus {
    pub pid: u32,
    pub version: String,
    pub protocol_version: u32,
    pub started_at: Timestamp,
    #[ts(type = "number")]
    pub uptime_seconds: u64,
    pub data_dir: String,
    pub socket_path: String,
    pub schema_version: u32,
    pub mode: RuntimeMode,
    pub connected_clients: u32,
}
