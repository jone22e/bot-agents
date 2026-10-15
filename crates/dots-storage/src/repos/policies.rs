use dots_core::{AutoApproveLevel, DotsResult, ToolPolicy, ToolPolicyUpdate};
use rusqlite::{Connection, OptionalExtension, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct PoliciesRepo<'a> {
    conn: &'a mut Connection,
}

impl<'a> PoliciesRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, dot_id: &str) -> DotsResult<ToolPolicy> {
        let row = self
            .conn
            .query_row(
                "SELECT auto_approve_level, allowed_commands_json, allow_network, max_file_bytes, engine_native_write, updated_at, can_delegate_to_json, max_delegation_depth FROM tool_policies WHERE dot_id = ?1",
                params![dot_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?, r.get::<_, i64>(7)?)),
            )
            .optional()
            .map_err(storage_err)?;
        match row {
            Some((level, cmds, net, max, native, updated, delegate, depth)) => Ok(ToolPolicy {
                dot_id: dot_id.into(),
                auto_approve_level: AutoApproveLevel::parse(&level),
                allowed_commands: serde_json::from_str(&cmds).unwrap_or_default(),
                allow_network: net == 1,
                max_file_bytes: max.max(0) as u64,
                engine_native_write: native == 1,
                can_delegate_to: serde_json::from_str(&delegate).unwrap_or_default(),
                max_delegation_depth: depth.max(0) as u32,
                updated_at: parse_ts(&updated)?,
            }),
            None => Ok(ToolPolicy::default_for(dot_id)),
        }
    }

    pub fn update(&mut self, dot_id: &str, patch: ToolPolicyUpdate) -> DotsResult<ToolPolicy> {
        let cur = self.get(dot_id)?;
        let next = ToolPolicy {
            dot_id: dot_id.into(),
            auto_approve_level: patch.auto_approve_level.unwrap_or(cur.auto_approve_level),
            allowed_commands: patch
                .allowed_commands
                .map(|c| c.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or(cur.allowed_commands),
            allow_network: patch.allow_network.unwrap_or(cur.allow_network),
            max_file_bytes: patch.max_file_bytes.unwrap_or(cur.max_file_bytes),
            engine_native_write: patch.engine_native_write.unwrap_or(cur.engine_native_write),
            can_delegate_to: patch.can_delegate_to.unwrap_or(cur.can_delegate_to),
            max_delegation_depth: patch.max_delegation_depth.unwrap_or(cur.max_delegation_depth).clamp(1, 5),
            updated_at: dots_core::now(),
        };
        self.conn
            .execute(
                "INSERT INTO tool_policies (dot_id, auto_approve_level, allowed_commands_json, allow_network, max_file_bytes, engine_native_write, updated_at, can_delegate_to_json, max_delegation_depth)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(dot_id) DO UPDATE SET auto_approve_level = excluded.auto_approve_level, allowed_commands_json = excluded.allowed_commands_json,
                    allow_network = excluded.allow_network, max_file_bytes = excluded.max_file_bytes, engine_native_write = excluded.engine_native_write, updated_at = excluded.updated_at,
                    can_delegate_to_json = excluded.can_delegate_to_json, max_delegation_depth = excluded.max_delegation_depth",
                params![dot_id, next.auto_approve_level.as_str(), serde_json::to_string(&next.allowed_commands)?, next.allow_network as i64, next.max_file_bytes as i64, next.engine_native_write as i64, ts(&next.updated_at), serde_json::to_string(&next.can_delegate_to)?, next.max_delegation_depth as i64],
            )
            .map_err(storage_err)?;
        Ok(next)
    }
}
