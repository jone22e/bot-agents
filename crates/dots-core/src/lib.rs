//! Tipos de dominio do DOTS compartilhados por runtime, storage, IPC e desktop.
//!
//! Regras:
//! - Identificadores sao UUID v7 em texto.
//! - Datas sao `DateTime<Utc>` (RFC 3339 na serializacao).
//! - Todo tipo exposto a UI deriva `TS` para gerar `packages/shared-types`.

pub mod apps;
pub mod teams;
pub mod audit;
pub mod automation;
pub mod conversation;
pub mod dot;
pub mod engine;
pub mod error;
pub mod ids;
pub mod installation;
pub mod memory;
pub mod policy;
pub mod runtime;
pub mod settings;
pub mod task;

pub use apps::{AppConnection, ConnectionStatus, DotIntegration, FlexiConnectInput};
pub use teams::{DotTeamInfo, Team, TeamCreate, TeamUpdate};
pub use audit::AuditLog;
pub use automation::{ApprovalKind, Checkpoint, Delegation, DelegationStatus, DotRole, EngineUsage, EventRecord, EventSubscription, ProcessSample, RuntimeStats, Schedule, ScheduleCreate, ScheduleKind, ScheduleUpdate, SubscriptionCreate, SubscriptionFilter, SubscriptionUpdate};
pub use conversation::{Contact, Conversation, Message, MessageRole};
pub use memory::{Memory, MemoryCreate, MemoryKind, MemoryUpdate};
pub use policy::{Approval, ApprovalStatus, AutoApproveLevel, ToolPolicy, ToolPolicyUpdate, WorkspaceChange};
pub use task::{Execution, ExecutionEvent, ExecutionEventRecord, ExecutionStatus, FailureKind, RiskLevel, Task, TaskCreate, TaskKind, TaskStatus};
pub use dot::{Dot, DotCreate, DotLimits, DotStatus, DotUpdate, DotWorkspace};
pub use engine::{AuthStatus, EngineCapability, EngineConfiguration, EngineDetection, EngineKind, EngineModel};
pub use error::{DotsError, DotsResult};
pub use ids::new_id;
pub use installation::Installation;
pub use runtime::{RuntimeMode, RuntimeStatus};
pub use settings::Setting;

pub type Timestamp = chrono::DateTime<chrono::Utc>;

pub fn now() -> Timestamp {
    chrono::Utc::now()
}

pub const APP_NAME: &str = "DOTS";
pub const BUNDLE_IDENTIFIER: &str = "com.c2s.dots";
pub const LAUNCH_AGENT_LABEL: &str = "com.c2s.dots.runtime";
pub const PROTOCOL_VERSION: u32 = 1;

#[cfg(test)]
mod export_bindings {
    //! `cargo test -p dots-core export_bindings` gera os arquivos TS.
    //! O diretorio de saida e controlado por `TS_RS_EXPORT_DIR`
    //! (padrao: `packages/shared-types/src/generated`).
    use ts_rs::TS;

    #[test]
    fn export_bindings() {
        if std::env::var_os("TS_RS_EXPORT_DIR").is_none() {
            let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/shared-types/src/generated");
            // SAFETY: teste single-thread para esta variavel; ts-rs le a env ao exportar.
            unsafe { std::env::set_var("TS_RS_EXPORT_DIR", dir) };
        }
        crate::DotsError::export_all().unwrap();
        crate::Team::export_all().unwrap();
        crate::TeamCreate::export_all().unwrap();
        crate::TeamUpdate::export_all().unwrap();
        crate::DotTeamInfo::export_all().unwrap();
        crate::Dot::export_all().unwrap();
        crate::DotCreate::export_all().unwrap();
        crate::DotUpdate::export_all().unwrap();
        crate::DotWorkspace::export_all().unwrap();
        crate::EngineDetection::export_all().unwrap();
        crate::EngineConfiguration::export_all().unwrap();
        crate::Installation::export_all().unwrap();
        crate::Setting::export_all().unwrap();
        crate::AuditLog::export_all().unwrap();
        crate::RuntimeStatus::export_all().unwrap();
        crate::Task::export_all().unwrap();
        crate::TaskCreate::export_all().unwrap();
        crate::Execution::export_all().unwrap();
        crate::ExecutionEventRecord::export_all().unwrap();
        crate::Conversation::export_all().unwrap();
        crate::Contact::export_all().unwrap();
        crate::Message::export_all().unwrap();
        crate::Memory::export_all().unwrap();
        crate::MemoryCreate::export_all().unwrap();
        crate::MemoryUpdate::export_all().unwrap();
        crate::ToolPolicy::export_all().unwrap();
        crate::ToolPolicyUpdate::export_all().unwrap();
        crate::Approval::export_all().unwrap();
        crate::WorkspaceChange::export_all().unwrap();
        crate::EventRecord::export_all().unwrap();
        crate::EventSubscription::export_all().unwrap();
        crate::SubscriptionCreate::export_all().unwrap();
        crate::SubscriptionUpdate::export_all().unwrap();
        crate::Schedule::export_all().unwrap();
        crate::ScheduleCreate::export_all().unwrap();
        crate::ScheduleUpdate::export_all().unwrap();
        crate::Checkpoint::export_all().unwrap();
        crate::Delegation::export_all().unwrap();
        crate::RuntimeStats::export_all().unwrap();
        crate::AppConnection::export_all().unwrap();
        crate::FlexiConnectInput::export_all().unwrap();
        crate::DotIntegration::export_all().unwrap();
    }
}
