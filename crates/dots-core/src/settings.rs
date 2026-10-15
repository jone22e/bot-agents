use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Timestamp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Setting {
    pub key: String,
    #[ts(type = "unknown")]
    pub value: serde_json::Value,
    pub updated_at: Timestamp,
}

/// Chaves conhecidas de configuracao da aplicacao.
pub mod keys {
    pub const ONBOARDING_COMPLETED: &str = "onboarding.completed";
    pub const BACKUP_KEEP_COUNT: &str = "backup.keep_count";
    pub const RUNTIME_START_AT_LOGIN: &str = "runtime.start_at_login";
    pub const UI_LANGUAGE: &str = "ui.language";
}
