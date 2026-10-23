//! Conector Flexi: login por e-mail/senha (com 2FA), sessao por cookie, chamadas as rotas
//! REST do Flexi e catalogo de ferramentas `flexi_*` para o gateway do DOTS.
//!
//! Fatos verificados em `flexi2/backend/src` (2026-10-15): `POST /api/auth/login {email, senha, server?}`
//! -> 200 `{status:1,user,empresa}` + cookie `userdata`; 202 `{status:2,mfa_required,challenge_token}`
//! -> `POST /api/auth/mfa/totp/login/verify {challenge_token, code}`; `GET /api/me` -> `{authenticated,user}`.
//! "Logar com Flexi" (navegador): `POST /api/auth/app-externo/desafios {app}` -> `{token, codigo, authorize_path}`;
//! o usuario autoriza em `authorize_path`; `GET /api/auth/app-externo/desafios/:token` entrega o cookie uma vez.

pub mod catalog;
pub mod client;
pub mod secrets;

pub use catalog::{FlexiTool, ToolRisk, catalog, find_tool, tool_definitions};
pub use client::{BrowserLoginChallenge, BrowserLoginPoll, FlexiClient, FlexiError, LoginOutcome, SessionInfo};
pub use secrets::{FallbackStore, FileStore, KeychainStore, MemoryStore, SecretStore};
