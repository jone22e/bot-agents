use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Timestamp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Installation {
    pub id: String,
    pub created_at: Timestamp,
    pub app_version: String,
    pub data_dir: String,
    pub platform: String,
}
