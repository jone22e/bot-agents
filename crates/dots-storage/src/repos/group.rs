//! Grupo geral: mensagens do usuario para todos os gerentes e as respostas deles.

use dots_core::{DotsResult, GroupMessage, MessageRole, new_id};
use rusqlite::{Connection, params};

use super::{json_or_default, parse_ts, ts};
use crate::storage_err;

pub struct GroupRepo<'a> {
    conn: &'a mut Connection,
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
                    "SELECT m.id, m.content, m.task_id, m.metadata_json, m.created_at, t.dot_id, d.name
                     FROM messages m JOIN tasks t ON t.id = m.task_id JOIN dots d ON d.id = t.dot_id
                     WHERE m.role = 'dot' AND (t.origin LIKE 'group:%'
                        OR (t.origin LIKE 'followup:%' AND t.correlation_id IN (SELECT g.id FROM tasks g WHERE g.origin LIKE 'group:%' AND g.dot_id = t.dot_id)))
                     ORDER BY m.created_at DESC, m.rowid DESC LIMIT ?1",
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
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        let skip = out.len().saturating_sub(limit as usize);
        Ok(out.split_off(skip))
    }
}
