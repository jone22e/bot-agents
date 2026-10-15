use dots_core::{AuditLog, DotsResult, new_id};
use rusqlite::{Connection, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct AuditRepo<'a> {
    conn: &'a mut Connection,
}

impl<'a> AuditRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn record(
        &mut self,
        actor: &str,
        action: &str,
        entity_type: &str,
        entity_id: Option<&str>,
        details: serde_json::Value,
    ) -> DotsResult<AuditLog> {
        let log = AuditLog {
            id: new_id(),
            actor: actor.to_string(),
            action: action.to_string(),
            entity_type: entity_type.to_string(),
            entity_id: entity_id.map(|s| s.to_string()),
            details,
            created_at: dots_core::now(),
        };
        self.conn
            .execute(
                "INSERT INTO audit_logs (id, actor, action, entity_type, entity_id, details_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    log.id,
                    log.actor,
                    log.action,
                    log.entity_type,
                    log.entity_id,
                    serde_json::to_string(&log.details)?,
                    ts(&log.created_at)
                ],
            )
            .map_err(storage_err)?;
        Ok(log)
    }

    pub fn list_recent(&self, limit: u32) -> DotsResult<Vec<AuditLog>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, actor, action, entity_type, entity_id, details_json, created_at
                 FROM audit_logs ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map(params![limit], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })
            .map_err(storage_err)?;
        let mut out = vec![];
        for row in rows {
            let (id, actor, action, entity_type, entity_id, details, created_at) = row.map_err(storage_err)?;
            out.push(AuditLog {
                id,
                actor,
                action,
                entity_type,
                entity_id,
                details: serde_json::from_str(&details)?,
                created_at: parse_ts(&created_at)?,
            });
        }
        Ok(out)
    }
}
