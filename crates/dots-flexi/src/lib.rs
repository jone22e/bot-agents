//! Conector Flexi: login por e-mail/senha (com 2FA), sessao por cookie, chamadas as rotas
//! REST do Flexi e catalogo de ferramentas `flexi_*` para o gateway do DOTS.
//!
//! Fatos verificados em `flexi2/backend/src` (2026-10-15): `POST /api/auth/login {email, senha, server?}`
//! -> 200 `{status:1,user,empresa}` + cookie `userdata`; 202 `{status:2,mfa_required,challenge_token}`
//! -> `POST /api/auth/mfa/totp/login/verify {challenge_token, code}`; `GET /api/me` -> `{authenticated,user}`.

pub mod catalog;
pub mod client;
pub mod secrets;

pub use catalog::{FlexiTool, ToolRisk, catalog, find_tool, tool_definitions};
pub use client::{FlexiClient, FlexiError, LoginOutcome, SessionInfo};
pub use secrets::{SecretStore, KeychainStore, MemoryStore};
