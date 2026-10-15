//! Persistencia local do DOTS em SQLite (rusqlite embutido, WAL).
//!
//! O runtime e o unico processo que abre o banco. A interface nunca toca o
//! arquivo diretamente. O acesso concorrente e serializado por um ator em
//! thread dedicada ([`StorageHandle`]).

mod actor;
mod migrations;
pub mod repos;
mod storage;

pub use actor::StorageHandle;
pub use migrations::{Migration, MIGRATIONS, latest_version};
pub use storage::{Storage, StorageOptions};

pub(crate) fn storage_err(e: rusqlite::Error) -> dots_core::DotsError {
    dots_core::DotsError::Storage(e.to_string())
}
