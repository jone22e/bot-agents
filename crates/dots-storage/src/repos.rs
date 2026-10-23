//! Repositorios: traduzem linhas SQLite em tipos de `dots-core`.

mod approvals;
mod apps;
mod audit;
mod automation;
mod changes;
mod conversations;
mod dismissals;
mod dots;
mod engines;
mod executions;
mod goals;
mod group;
mod installation;
mod learned;
mod memories;
mod notifications;
mod policies;
mod settings;
mod tasks;
mod teams;

pub use approvals::ApprovalsRepo;
pub use apps::AppsRepo;
pub use automation::{CheckpointsRepo, DelegationsRepo, EventsRepo, SchedulesRepo, SubscriptionsRepo};
pub use audit::AuditRepo;
pub use changes::ChangesRepo;
pub use conversations::ConversationsRepo;
pub use dismissals::{DismissalInput, DismissalsRepo};
pub use dots::DotsRepo;
pub use engines::EnginesRepo;
pub use executions::{ExecutionsRepo, ObservedActivityInterval};
pub use goals::{GoalEdit, GoalsRepo};
pub use group::{GROUP_ORIGIN_PREFIX, GroupRepo};
pub use installation::InstallationRepo;
pub use learned::LearnedRepo;
pub use memories::MemoriesRepo;
pub use notifications::NotificationsRepo;
pub use policies::PoliciesRepo;
pub use settings::SettingsRepo;
pub use teams::TeamsRepo;
pub use tasks::{TaskFilter, TasksRepo};

use dots_core::{DotsError, DotsResult, Timestamp};

pub(crate) fn parse_ts(value: &str) -> DotsResult<Timestamp> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|d| d.with_timezone(&chrono::Utc))
        .map_err(|e| DotsError::Storage(format!("data invalida '{value}': {e}")))
}

pub(crate) fn parse_opt_ts(value: Option<String>) -> DotsResult<Option<Timestamp>> {
    value.map(|v| parse_ts(&v)).transpose()
}

pub(crate) fn ts(value: &Timestamp) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn json_or_default(value: Option<String>) -> serde_json::Value {
    value.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or(serde_json::Value::Null)
}
