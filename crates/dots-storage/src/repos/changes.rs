use dots_core::{DotsResult, WorkspaceChange, new_id};
use rusqlite::{Connection, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct ChangesRepo<'a> {
    conn: &'a mut Connection,
}

impl<'a> ChangesRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn record(&mut self, execution_id: &str, dot_id: &str, path: &str, kind: &str, summary: &str) -> DotsResult<WorkspaceChange> {
        let id = new_id();
        let now = dots_core::now();
        self.conn
            .execute(
                "INSERT INTO workspace_changes (id, execution_id, dot_id, path, kind, summary, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, execution_id, dot_id, path, kind, summary, ts(&now)],
            )
            .map_err(storage_err)?;
        Ok(WorkspaceChange { id, execution_id: execution_id.into(), dot_id: dot_id.into(), path: path.into(), kind: kind.into(), summary: summary.into(), created_at: now })
    }

    pub fn list(&self, execution_id: Option<&str>, dot_id: Option<&str>, limit: u32) -> DotsResult<Vec<WorkspaceChange>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, execution_id, dot_id, path, kind, summary, created_at FROM workspace_changes
                 WHERE (?1 IS NULL OR execution_id = ?1) AND (?2 IS NULL OR dot_id = ?2) ORDER BY created_at DESC LIMIT ?3",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map(params![execution_id, dot_id, if limit == 0 { 200 } else { limit }], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?))
            })
            .map_err(storage_err)?;
        rows.map(|r| {
            let (id, execution_id, dot_id, path, kind, summary, created_at) = r.map_err(storage_err)?;
            Ok(WorkspaceChange { id, execution_id, dot_id, path, kind, summary, created_at: parse_ts(&created_at)? })
        })
        .collect()
    }
}
