//! Permissoes aprendidas por DOT (tipo de acao aprovado uma vez).

use dots_core::{DotsResult, LearnedPermission, new_id};
use rusqlite::{Connection, OptionalExtension, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct LearnedRepo<'a> {
    conn: &'a mut Connection,
}

impl<'a> LearnedRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Grava (ou mantem, se ja existe) a permissao do tipo de acao para o DOT.
    pub fn grant(&mut self, dot_id: &str, action_key: &str, summary: &str, approval_id: Option<&str>) -> DotsResult<()> {
        self.conn
            .execute(
                "INSERT INTO learned_permissions (id, dot_id, action_key, summary, approval_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(dot_id, action_key) DO NOTHING",
                params![new_id(), dot_id, action_key, summary, approval_id, ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    pub fn has(&self, dot_id: &str, action_key: &str) -> DotsResult<bool> {
        let r: Option<i64> = self
            .conn
            .query_row("SELECT 1 FROM learned_permissions WHERE dot_id = ?1 AND action_key = ?2", params![dot_id, action_key], |r| r.get(0))
            .optional()
            .map_err(storage_err)?;
        Ok(r.is_some())
    }

    pub fn list(&self, dot_id: &str) -> DotsResult<Vec<LearnedPermission>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, dot_id, action_key, summary, approval_id, created_at FROM learned_permissions WHERE dot_id = ?1 ORDER BY created_at DESC")
            .map_err(storage_err)?;
        let rows = stmt
            .query_map(params![dot_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, String>(5)?)))
            .map_err(storage_err)?;
        rows.map(|r| {
            let (id, dot_id, action_key, summary, approval_id, created_at) = r.map_err(storage_err)?;
            Ok(LearnedPermission { id, dot_id, action_key, summary, approval_id, created_at: parse_ts(&created_at)? })
        })
        .collect()
    }

    /// Esquece a permissao. Devolve o DOT dela, se existia.
    pub fn forget(&mut self, id: &str) -> DotsResult<Option<String>> {
        let dot: Option<String> = self.conn.query_row("SELECT dot_id FROM learned_permissions WHERE id = ?1", params![id], |r| r.get(0)).optional().map_err(storage_err)?;
        self.conn.execute("DELETE FROM learned_permissions WHERE id = ?1", params![id]).map_err(storage_err)?;
        Ok(dot)
    }
}
