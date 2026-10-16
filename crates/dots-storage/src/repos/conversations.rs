use dots_core::{Conversation, DotsError, DotsResult, EngineKind, Message, MessageRole, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{json_or_default, parse_ts, ts};
use crate::storage_err;

pub struct ConversationsRepo<'a> {
    conn: &'a mut Connection,
}

const COLS: &str = "c.id, c.dot_id, c.title, c.engine_sessions_json, (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id), c.created_at, c.updated_at,
    (SELECT content FROM messages m WHERE m.conversation_id = c.id AND m.content != '' ORDER BY m.created_at DESC, m.rowid DESC LIMIT 1),
    (SELECT role FROM messages m WHERE m.conversation_id = c.id AND m.content != '' ORDER BY m.created_at DESC, m.rowid DESC LIMIT 1)";

type ConvRaw = (String, String, String, String, u32, String, String, Option<String>, Option<String>);

fn map(r: &Row<'_>) -> rusqlite::Result<ConvRaw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?))
}

fn to_conv(r: ConvRaw) -> DotsResult<Conversation> {
    let (id, dot_id, title, sessions, message_count, created_at, updated_at, last_message, last_role) = r;
    Ok(Conversation {
        id,
        dot_id,
        title,
        engine_sessions: serde_json::from_str(&sessions).unwrap_or(serde_json::json!({})),
        message_count,
        last_message: last_message.map(|m| m.chars().take(160).collect()),
        last_role,
        created_at: parse_ts(&created_at)?,
        updated_at: parse_ts(&updated_at)?,
    })
}

const MSG_COLS: &str = "id, conversation_id, role, content, task_id, execution_id, metadata_json, created_at";

fn to_msg(r: &Row<'_>) -> rusqlite::Result<(String, String, String, String, Option<String>, Option<String>, Option<String>, String)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
}

impl<'a> ConversationsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Conversation> {
        self.conn
            .query_row(&format!("SELECT {COLS} FROM conversations c WHERE c.id = ?1"), params![id], map)
            .optional()
            .map_err(storage_err)?
            .map(to_conv)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("conversa {id}")))
    }

    pub fn list(&self, dot_id: Option<&str>) -> DotsResult<Vec<Conversation>> {
        let sql = match dot_id {
            Some(_) => format!("SELECT {COLS} FROM conversations c WHERE c.dot_id = ?1 ORDER BY c.updated_at DESC"),
            None => format!("SELECT {COLS} FROM conversations c ORDER BY c.updated_at DESC"),
        };
        let mut stmt = self.conn.prepare(&sql).map_err(storage_err)?;
        let rows = match dot_id {
            Some(d) => stmt.query_map(params![d], map),
            None => stmt.query_map([], map),
        }
        .map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_conv)).collect()
    }

    /// Conversa principal do DOT (a mais antiga); cria se nao existir.
    pub fn main(&mut self, dot_id: &str) -> DotsResult<Conversation> {
        let existing = self
            .conn
            .query_row(&format!("SELECT {COLS} FROM conversations c WHERE c.dot_id = ?1 ORDER BY c.created_at LIMIT 1"), params![dot_id], map)
            .optional()
            .map_err(storage_err)?
            .map(to_conv)
            .transpose()?;
        match existing {
            Some(c) => Ok(c),
            None => self.create(dot_id, "Conversa"),
        }
    }

    pub fn create(&mut self, dot_id: &str, title: &str) -> DotsResult<Conversation> {
        let id = new_id();
        let now = ts(&dots_core::now());
        self.conn
            .execute(
                "INSERT INTO conversations (id, dot_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![id, dot_id, title, now],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn set_title(&mut self, id: &str, title: &str) -> DotsResult<Conversation> {
        self.conn
            .execute("UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3", params![title, ts(&dots_core::now()), id])
            .map_err(storage_err)?;
        self.get(id)
    }

    /// Apaga todas as conversas (e mensagens, em cascata) do DOT. Devolve quantas conversas foram apagadas.
    pub fn delete_for_dot(&mut self, dot_id: &str) -> DotsResult<u32> {
        let n = self.conn.execute("DELETE FROM conversations WHERE dot_id = ?1", params![dot_id]).map_err(storage_err)?;
        Ok(n as u32)
    }

    pub fn delete(&mut self, id: &str) -> DotsResult<()> {
        let n = self.conn.execute("DELETE FROM conversations WHERE id = ?1", params![id]).map_err(storage_err)?;
        if n == 0 {
            return Err(DotsError::NotFound(format!("conversa {id}")));
        }
        Ok(())
    }

    pub fn engine_session(&self, id: &str, engine: EngineKind) -> DotsResult<Option<String>> {
        let c = self.get(id)?;
        Ok(c.engine_sessions.get(engine.as_str()).and_then(|v| v.as_str().map(String::from)))
    }

    pub fn set_engine_session(&mut self, id: &str, engine: EngineKind, session: Option<&str>) -> DotsResult<()> {
        let c = self.get(id)?;
        let mut sessions = c.engine_sessions;
        if !sessions.is_object() {
            sessions = serde_json::json!({});
        }
        match session {
            Some(s) => {
                sessions[engine.as_str()] = serde_json::Value::String(s.to_string());
            }
            None => {
                if let Some(obj) = sessions.as_object_mut() {
                    obj.remove(engine.as_str());
                }
            }
        }
        self.conn
            .execute(
                "UPDATE conversations SET engine_sessions_json = ?1, updated_at = ?2 WHERE id = ?3",
                params![serde_json::to_string(&sessions)?, ts(&dots_core::now()), id],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    pub fn add_message(&mut self, conversation_id: &str, role: MessageRole, content: &str, task_id: Option<&str>, execution_id: Option<&str>, metadata: serde_json::Value) -> DotsResult<Message> {
        let id = new_id();
        let now = dots_core::now();
        let tx = self.conn.transaction().map_err(storage_err)?;
        tx.execute(
            "INSERT INTO messages (id, conversation_id, role, content, task_id, execution_id, metadata_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, conversation_id, role.as_str(), content, task_id, execution_id, serde_json::to_string(&metadata)?, ts(&now)],
        )
        .map_err(storage_err)?;
        tx.execute("UPDATE conversations SET updated_at = ?1 WHERE id = ?2", params![ts(&now), conversation_id]).map_err(storage_err)?;
        tx.commit().map_err(storage_err)?;
        Ok(Message { id, conversation_id: conversation_id.to_string(), role, content: content.to_string(), task_id: task_id.map(String::from), execution_id: execution_id.map(String::from), metadata, created_at: now })
    }

    pub fn update_message_content(&mut self, message_id: &str, content: &str, metadata: Option<serde_json::Value>) -> DotsResult<()> {
        match metadata {
            Some(m) => self
                .conn
                .execute("UPDATE messages SET content = ?1, metadata_json = ?2 WHERE id = ?3", params![content, serde_json::to_string(&m)?, message_id])
                .map_err(storage_err)?,
            None => self
                .conn
                .execute("UPDATE messages SET content = ?1 WHERE id = ?2", params![content, message_id])
                .map_err(storage_err)?,
        };
        Ok(())
    }

    pub fn messages(&self, conversation_id: &str, limit: u32) -> DotsResult<Vec<Message>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {MSG_COLS} FROM messages WHERE conversation_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2"))
            .map_err(storage_err)?;
        let rows = stmt.query_map(params![conversation_id, if limit == 0 { 500 } else { limit }], to_msg).map_err(storage_err)?;
        let mut out: Vec<Message> = rows
            .map(|r| {
                let (id, conversation_id, role, content, task_id, execution_id, metadata, created_at) = r.map_err(storage_err)?;
                Ok(Message { id, conversation_id, role: MessageRole::parse(&role)?, content, task_id, execution_id, metadata: json_or_default(metadata), created_at: parse_ts(&created_at)? })
            })
            .collect::<DotsResult<_>>()?;
        out.reverse();
        Ok(out)
    }
}
