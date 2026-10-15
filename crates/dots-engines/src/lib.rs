//! Motores CLI (Codex, Claude Code, AGY): deteccao (Fase 1) e execucao (Fase 2).
//!
//! Nada aqui presume recursos: flags e formatos de evento foram verificados nas
//! versoes de referencia (`docs/engines/README.md`) e os parsers sao testados
//! contra capturas reais (`docs/engines/fixtures`).

pub mod adapters;
mod detect;
mod manager;
mod models;
mod process;
mod request;
mod resolve;
mod runner;
pub mod specs;

pub use adapters::{AgentEngine, ParserState, adapter_for, current_username};
pub use detect::{DetectOptions, detect_all, detect_one};
pub use manager::{EngineManager, ExecutionHandle, ExecutionStream};
pub use process::{CommandOutput, run_command};
pub use request::{ExecutionRequest, GatewayConfig, PreparedCommand};
pub use resolve::{candidate_dirs, resolve_executable};
pub use runner::{RunnerEvent, spawn_prepared};
