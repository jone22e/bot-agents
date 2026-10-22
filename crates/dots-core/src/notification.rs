//! Notificacoes: um DOT pede a atencao do usuario (`dots_notify_user`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Timestamp;

/// Aviso de um DOT para o usuario. Fica visivel ate o usuario clicar (abrir a conversa ou limpar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Notification {
    pub id: String,
    pub dot_id: String,
    pub dot_name: String,
    pub task_id: Option<String>,
    pub conversation_id: Option<String>,
    /// Frase curta do que aconteceu (ex.: "3 notas nao transmitidas").
    pub title: String,
    /// Complemento opcional (ex.: "R$ 14.820,00 · aguardando decisao").
    pub detail: String,
    /// Rotulo do botao que abre a conversa (ex.: "Ver notas"); vazio = "Ver".
    pub action: String,
    /// `pending` (na tela) ou `cleared` (o usuario clicou).
    pub status: String,
    pub created_at: Timestamp,
    pub cleared_at: Option<Timestamp>,
}
