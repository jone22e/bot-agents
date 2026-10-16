//! Grupo geral: mensagens do usuario para todos os gerentes e as respostas deles.

use dots_core::{DotsResult, GroupMessage, MessageRole, new_id};
use rusqlite::{Connection, params};

use super::{json_or_default, parse_ts, ts};
use crate::storage_err;

pub struct GroupRepo<'a> {
    conn: &'a mut Connection,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_follow_delivery_order_instead_of_start_order() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE group_messages (id TEXT, content TEXT, metadata_json TEXT, created_at TEXT);
             CREATE TABLE messages (id TEXT, content TEXT, task_id TEXT, metadata_json TEXT, created_at TEXT, role TEXT, execution_id TEXT);
             CREATE TABLE tasks (id TEXT, dot_id TEXT, origin TEXT, correlation_id TEXT);
             CREATE TABLE dots (id TEXT, name TEXT);
             CREATE TABLE executions (id TEXT, finished_at TEXT);
             INSERT INTO dots VALUES ('a', 'Ana');
             INSERT INTO tasks VALUES ('t1', 'a', 'group:u1', NULL);
             INSERT INTO tasks VALUES ('t2', 'a', 'group:u2', NULL);
             INSERT INTO group_messages VALUES ('u1', 'primeira', '{}', '2026-10-16T10:00:00.000Z');
             INSERT INTO group_messages VALUES ('u2', 'segunda', '{}', '2026-10-16T10:00:03.000Z');
             INSERT INTO executions VALUES ('e1', '2026-10-16T10:00:05.000Z');
             INSERT INTO executions VALUES ('e2', '2026-10-16T10:00:04.000Z');
             INSERT INTO messages VALUES ('r1', 'resposta lenta', 't1', '{}', '2026-10-16T10:00:01.000Z', 'dot', 'e1');
             INSERT INTO messages VALUES ('r2', 'resposta rapida', 't2', '{}', '2026-10-16T10:00:03.500Z', 'dot', 'e2');",
        )
        .unwrap();

        let messages = GroupRepo::new(&mut conn).messages(20).unwrap();
        assert_eq!(messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["u1", "u2", "r2", "r1"]);
        assert_eq!(messages[3].created_at.to_rfc3339(), "2026-10-16T10:00:05+00:00");
    }
}

/// Prefixo da origem das tarefas criadas por uma mensagem do grupo (`group:<id da mensagem>`).
pub const GROUP_ORIGIN_PREFIX: &str = "group:";

impl<'a> GroupRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn add_user_message(&mut self, content: &str, metadata: serde_json::Value) -> DotsResult<GroupMessage> {
        let id = new_id();
        let now = dots_core::now();
        self.conn
            .execute("INSERT INTO group_messages (id, content, metadata_json, created_at) VALUES (?1, ?2, ?3, ?4)", params![id, content, serde_json::to_string(&metadata)?, ts(&now)])
            .map_err(storage_err)?;
        Ok(GroupMessage { id, role: MessageRole::User, content: content.to_string(), dot_id: None, dot_name: None, task_id: None, metadata, created_at: now })
    }

    /// Ultimas `limit` mensagens do grupo, em ordem cronologica: as do usuario e as respostas dos gerentes
    /// (tarefas com origem `group:*` e as continuacoes delas, que chegam quando os funcionarios terminam).
    pub fn messages(&self, limit: u32) -> DotsResult<Vec<GroupMessage>> {
        let limit = if limit == 0 { 300 } else { limit };
        let mut out: Vec<GroupMessage> = vec![];
        {
            let mut stmt = self.conn.prepare("SELECT id, content, metadata_json, created_at FROM group_messages ORDER BY created_at DESC LIMIT ?1").map_err(storage_err)?;
            let rows = stmt
                .query_map(params![limit], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?)))
                .map_err(storage_err)?;
            for row in rows {
                let (id, content, meta, created_at) = row.map_err(storage_err)?;
                out.push(GroupMessage { id, role: MessageRole::User, content, dot_id: None, dot_name: None, task_id: None, metadata: json_or_default(meta), created_at: parse_ts(&created_at)? });
            }
        }
        {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT m.id, m.content, m.task_id, m.metadata_json, COALESCE(e.finished_at, m.created_at), t.dot_id, d.name
                     FROM messages m JOIN tasks t ON t.id = m.task_id JOIN dots d ON d.id = t.dot_id
                     LEFT JOIN executions e ON e.id = m.execution_id
                     WHERE m.role = 'dot' AND (t.origin LIKE 'group:%'
                        OR (t.origin LIKE 'followup:%' AND t.correlation_id IN (SELECT g.id FROM tasks g WHERE g.origin LIKE 'group:%' AND g.dot_id = t.dot_id)))
                     ORDER BY COALESCE(e.finished_at, m.created_at) DESC, m.rowid DESC LIMIT ?1",
                )
                .map_err(storage_err)?;
            let rows = stmt
                .query_map(params![limit], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?))
                })
                .map_err(storage_err)?;
            for row in rows {
                let (id, content, task_id, meta, created_at, dot_id, dot_name) = row.map_err(storage_err)?;
                out.push(GroupMessage { id, role: MessageRole::Dot, content, dot_id: Some(dot_id), dot_name: Some(dot_name), task_id, metadata: json_or_default(meta), created_at: parse_ts(&created_at)? });
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
        let skip = out.len().saturating_sub(limit as usize);
        Ok(out.split_off(skip))
    }
}
