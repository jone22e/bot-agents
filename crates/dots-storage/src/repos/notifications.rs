use dots_core::{DotsError, DotsResult, Notification, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{parse_opt_ts, parse_ts, ts};
use crate::storage_err;

/// Notificacoes de DOTs para o usuario: ficam `pending` ate ele clicar.
pub struct NotificationsRepo<'a> {
    conn: &'a mut Connection,
}

const COLS: &str = "n.id, n.dot_id, COALESCE(d.name, '(removido)'), n.task_id, t.conversation_id, n.title, n.detail, n.action, n.status, n.created_at, n.cleared_at";
const FROM: &str = "notifications n LEFT JOIN dots d ON d.id = n.dot_id LEFT JOIN tasks t ON t.id = n.task_id";

type Raw = (String, String, String, Option<String>, Option<String>, String, String, String, String, String, Option<String>);

fn map(r: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?))
}

fn build(r: Raw) -> DotsResult<Notification> {
    let (id, dot_id, dot_name, task_id, conversation_id, title, detail, action, status, created_at, cleared_at) = r;
    Ok(Notification { id, dot_id, dot_name, task_id, conversation_id, title, detail, action, status, created_at: parse_ts(&created_at)?, cleared_at: parse_opt_ts(cleared_at)? })
}

impl<'a> NotificationsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Notification> {
        self.conn
            .query_row(&format!("SELECT {COLS} FROM {FROM} WHERE n.id = ?1"), params![id], map)
            .optional()
            .map_err(storage_err)?
            .map(build)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("notificacao {id}")))
    }

    /// Cria uma notificacao pendente. Titulo obrigatorio; detalhe e rotulo da acao sao opcionais.
    pub fn create(&mut self, dot_id: &str, task_id: Option<&str>, title: &str, detail: &str, action: &str) -> DotsResult<Notification> {
        let title = title.trim();
        if title.is_empty() {
            return Err(DotsError::Validation("a notificacao precisa de um titulo".into()));
        }
        let id = new_id();
        self.conn
            .execute(
                "INSERT INTO notifications (id, dot_id, task_id, title, detail, action, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)",
                params![id, dot_id, task_id, title, detail.trim(), action.trim(), ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    /// Pendentes (as mais antigas primeiro: a primeira e a que o usuario ve no balao fechado).
    pub fn list(&self, only_pending: bool, limit: u32) -> DotsResult<Vec<Notification>> {
        let limit = if limit == 0 { 100 } else { limit };
        let where_sql = if only_pending { "WHERE n.status = 'pending'" } else { "" };
        let mut stmt = self.conn.prepare(&format!("SELECT {COLS} FROM {FROM} {where_sql} ORDER BY n.created_at ASC LIMIT ?1")).map_err(storage_err)?;
        let rows = stmt.query_map(params![limit], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(build)).collect()
    }

    /// O usuario clicou: `id` = uma notificacao, `None` = todas as pendentes. Devolve quantas mudaram.
    pub fn clear(&mut self, id: Option<&str>) -> DotsResult<usize> {
        let now = ts(&dots_core::now());
        let n = match id {
            Some(id) => self.conn.execute("UPDATE notifications SET status = 'cleared', cleared_at = ?1 WHERE id = ?2 AND status = 'pending'", params![now, id]),
            None => self.conn.execute("UPDATE notifications SET status = 'cleared', cleared_at = ?1 WHERE status = 'pending'", params![now]),
        }
        .map_err(storage_err)?;
        Ok(n)
    }
}
