use dots_core::{Approval, ApprovalKind, ApprovalStatus, DotsError, DotsResult, RiskLevel, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{parse_opt_ts, parse_ts, ts};
use crate::storage_err;

pub struct ApprovalsRepo<'a> {
    conn: &'a mut Connection,
}

const COLS: &str = "a.id, a.execution_id, a.task_id, a.dot_id, COALESCE(d.name, '(removido)'), a.tool, a.input_json, a.risk, a.resources_json, a.summary, a.status, a.requested_at, a.expires_at, a.resolved_at, a.reason, a.kind, a.options_json, t.conversation_id";

type Raw = (String, String, String, String, String, String, String, String, String, String, String, String, String, Option<String>, Option<String>, String, String, Option<String>);

fn map(r: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?, r.get(15)?, r.get(16)?, r.get(17)?))
}

fn to_approval(r: Raw) -> DotsResult<Approval> {
    let (id, execution_id, task_id, dot_id, dot_name, tool, input, risk, resources, summary, status, requested_at, expires_at, resolved_at, reason, kind, options, conversation_id) = r;
    Ok(Approval {
        id, execution_id, task_id, conversation_id, dot_id, dot_name, tool,
        kind: ApprovalKind::parse(&kind),
        options: serde_json::from_str(&options).unwrap_or_default(),
        input: serde_json::from_str(&input)?,
        risk: RiskLevel::parse(&risk),
        resources: serde_json::from_str(&resources).unwrap_or_default(),
        summary,
        status: ApprovalStatus::parse(&status),
        requested_at: parse_ts(&requested_at)?,
        expires_at: parse_ts(&expires_at)?,
        resolved_at: parse_opt_ts(resolved_at)?,
        reason,
    })
}

impl<'a> ApprovalsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Approval> {
        self.conn
            .query_row(&format!("SELECT {COLS} FROM approvals a LEFT JOIN dots d ON d.id = a.dot_id LEFT JOIN tasks t ON t.id = a.task_id WHERE a.id = ?1"), params![id], map)
            .optional()
            .map_err(storage_err)?
            .map(to_approval)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("aprovacao {id}")))
    }

    pub fn list(&self, only_pending: bool, limit: u32) -> DotsResult<Vec<Approval>> {
        let filter = if only_pending { " WHERE a.status = 'pending'" } else { "" };
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM approvals a LEFT JOIN dots d ON d.id = a.dot_id LEFT JOIN tasks t ON t.id = a.task_id{filter} ORDER BY a.requested_at DESC LIMIT ?1"))
            .map_err(storage_err)?;
        let rows = stmt.query_map(params![if limit == 0 { 200 } else { limit }], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_approval)).collect()
    }

    pub fn pending_count(&self) -> DotsResult<u32> {
        self.conn.query_row("SELECT COUNT(*) FROM approvals WHERE status = 'pending' AND kind = 'permission'", [], |r| r.get(0)).map_err(storage_err)
    }

    pub fn pending_questions_count(&self) -> DotsResult<u32> {
        self.conn.query_row("SELECT COUNT(*) FROM approvals WHERE status = 'pending' AND kind = 'question'", [], |r| r.get(0)).map_err(storage_err)
    }

    /// Pergunta ao usuario (Supervisor). A resposta e gravada em `reason` ao resolver.
    pub fn create_question(&mut self, execution_id: &str, task_id: &str, dot_id: &str, question: &str, options: &[String], ttl_seconds: i64) -> DotsResult<Approval> {
        let id = new_id();
        let now = dots_core::now();
        let expires = now + chrono::Duration::seconds(ttl_seconds);
        self.conn
            .execute(
                "INSERT INTO approvals (id, execution_id, task_id, dot_id, tool, input_json, risk, resources_json, summary, status, requested_at, expires_at, kind, options_json)
                 VALUES (?1, ?2, ?3, ?4, 'ask_user', ?5, 'read', '[]', ?6, 'pending', ?7, ?8, 'question', ?9)",
                params![id, execution_id, task_id, dot_id, serde_json::to_string(&serde_json::json!({ "question": question, "options": options }))?, question, ts(&now), ts(&expires), serde_json::to_string(options)?],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn create(&mut self, execution_id: &str, task_id: &str, dot_id: &str, tool: &str, input: &serde_json::Value, risk: RiskLevel, resources: &[String], summary: &str, ttl_seconds: i64) -> DotsResult<Approval> {
        let id = new_id();
        let now = dots_core::now();
        let expires = now + chrono::Duration::seconds(ttl_seconds);
        self.conn
            .execute(
                "INSERT INTO approvals (id, execution_id, task_id, dot_id, tool, input_json, risk, resources_json, summary, status, requested_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10, ?11)",
                params![id, execution_id, task_id, dot_id, tool, serde_json::to_string(input)?, risk.as_str(), serde_json::to_string(resources)?, summary, ts(&now), ts(&expires)],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn resolve(&mut self, id: &str, approved: bool, reason: Option<&str>) -> DotsResult<Approval> {
        let current = self.get(id)?;
        if current.status != ApprovalStatus::Pending {
            return Err(DotsError::Conflict(format!("aprovacao ja resolvida ({})", current.status.as_str())));
        }
        if current.expires_at < dots_core::now() {
            self.conn
                .execute("UPDATE approvals SET status = 'expired', resolved_at = ?1 WHERE id = ?2", params![ts(&dots_core::now()), id])
                .map_err(storage_err)?;
            return Err(DotsError::Conflict("aprovacao expirada".into()));
        }
        self.conn
            .execute(
                "UPDATE approvals SET status = ?1, resolved_at = ?2, reason = ?3 WHERE id = ?4",
                params![if approved { "approved" } else { "rejected" }, ts(&dots_core::now()), reason, id],
            )
            .map_err(storage_err)?;
        self.get(id)
    }

    pub fn expire_due(&mut self) -> DotsResult<Vec<String>> {
        let now = ts(&dots_core::now());
        let mut stmt = self.conn.prepare("SELECT id FROM approvals WHERE status = 'pending' AND expires_at < ?1").map_err(storage_err)?;
        let ids: Vec<String> = stmt.query_map(params![now], |r| r.get(0)).map_err(storage_err)?.collect::<Result<_, _>>().map_err(storage_err)?;
        drop(stmt);
        for id in &ids {
            self.conn
                .execute("UPDATE approvals SET status = 'expired', resolved_at = ?1 WHERE id = ?2", params![now, id])
                .map_err(storage_err)?;
        }
        Ok(ids)
    }
}
