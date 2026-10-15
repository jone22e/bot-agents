//! IPC local do DOTS: Unix domain socket + NDJSON (subconjunto JSON-RPC).
//!
//! Seguranca: diretorio do socket 0700, socket 0600, verificacao do uid do
//! peer e token (rotacionado a cada start do runtime) enviado uma vez no
//! frame `hello`.

pub mod client;
pub mod paths;
pub mod protocol;
pub mod server;

pub use client::{IpcClient, EventReceiver};
pub use paths::{RuntimeInfo, SocketPaths};
pub use protocol::{ErrorBody, Event, Frame, HelloParams, HelloResult, Request, Response, MAX_FRAME_BYTES};
pub use server::{Handler, HandlerFuture, IpcServer, ServerConfig};
