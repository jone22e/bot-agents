use std::collections::HashMap;

use dots_core::{DotsError, DotsResult, EngineKind, Execution, ExecutionEvent, ExecutionEventRecord, ExecutionStatus, Timestamp, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use super::{json_or_default, parse_opt_ts, parse_ts, ts};
use crate::storage_err;

pub struct ExecutionsRepo<'a> {
    conn: &'a mut Connection,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservedActivityInterval {
    pub dot_id: String,
    pub started_at: Timestamp,
    pub finished_at: Timestamp,
}

const COLS: &str = "id, task_id, dot_id, engine, attempt, status, pid, exit_code, engine_session_id, workspace_path, error,
    events_count, cost_usd, usage_json, started_at, finished_at";

type Raw = (String, String, String, String, u32, String, Option<u32>, Option<i32>, Option<String>, Option<String>, Option<String>, u32, Option<f64>, Option<String>, String, Option<String>);

fn map(r: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((
        r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?,
        r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?, r.get(15)?,
    ))
}

fn to_exec(r: Raw) -> DotsResult<Execution> {
    let (id, task_id, dot_id, engine, attempt, status, pid, exit_code, engine_session_id, workspace_path, error, events_count, cost_usd, usage, started_at, finished_at) = r;
    Ok(Execution {
        id,
        task_id,
        dot_id,
        engine: EngineKind::parse(&engine)?,
        attempt,
        status: ExecutionStatus::parse(&status)?,
        pid,
        exit_code,
        engine_session_id,
        workspace_path,
        error,
        events_count,
        cost_usd,
        usage: json_or_default(usage),
        started_at: parse_ts(&started_at)?,
        finished_at: parse_opt_ts(finished_at)?,
    })
}

impl<'a> ExecutionsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Execution> {
        self.conn
            .query_row(&format!("SELECT {COLS} FROM executions WHERE id = ?1"), params![id], map)
            .optional()
            .map_err(storage_err)?
            .map(to_exec)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("execucao {id}")))
    }

    pub fn list_for_task(&self, task_id: &str) -> DotsResult<Vec<Execution>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM executions WHERE task_id = ?1 ORDER BY attempt"))
            .map_err(storage_err)?;
        let rows = stmt.query_map(params![task_id], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_exec)).collect()
    }

    pub fn list_recent(&self, limit: u32) -> DotsResult<Vec<Execution>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM executions ORDER BY started_at DESC LIMIT ?1"))
            .map_err(storage_err)?;
        let rows = stmt.query_map(params![limit], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_exec)).collect()
    }

    /// Execucoes que cruzam o intervalo [start, end), inclusive as ainda em andamento.
    pub fn list_overlapping(&self, start: &Timestamp, end: &Timestamp) -> DotsResult<Vec<Execution>> {
        let mut stmt = self.conn
            .prepare(&format!("SELECT {COLS} FROM executions WHERE started_at < ?2 AND (finished_at IS NULL OR finished_at > ?1)"))
            .map_err(storage_err)?;
        let rows = stmt.query_map(params![ts(start), ts(end)], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_exec)).collect()
    }

    /// Chamadas de ferramenta registradas no intervalo [start, end), por DOT.
    pub fn tool_calls_by_dot(&self, start: &Timestamp, end: &Timestamp) -> DotsResult<HashMap<String, u64>> {
        let mut stmt = self.conn.prepare(
            "SELECT executions.dot_id, COUNT(*)
             FROM execution_events
             JOIN executions ON executions.id = execution_events.execution_id
             WHERE execution_events.type = 'tool_call'
               AND execution_events.created_at >= ?1
               AND execution_events.created_at < ?2
             GROUP BY executions.dot_id",
        ).map_err(storage_err)?;
        let rows = stmt.query_map(params![ts(start), ts(end)], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64))).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err)).collect()
    }

    /// Intervalos comprovados por chamada e retorno de ferramenta; esperas de aprovacao sao retiradas.
    pub fn observed_activity(&self, start: &Timestamp, end: &Timestamp) -> DotsResult<Vec<ObservedActivityInterval>> {
        let mut stmt = self.conn.prepare(
            "SELECT ev.execution_id, e.dot_id, ev.payload_json, ev.created_at
             FROM executions e JOIN execution_events ev ON ev.execution_id = e.id
             WHERE e.started_at < ?2 AND (e.finished_at IS NULL OR e.finished_at > ?1)
               AND ev.type IN ('tool_call', 'tool_result')
             ORDER BY ev.execution_id, ev.seq",
        ).map_err(storage_err)?;
        let rows = stmt.query_map(params![ts(start), ts(end)], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        }).map_err(storage_err)?;
        let mut open = HashMap::<(String, String), Timestamp>::new();
        let mut completed = Vec::new();
        for row in rows {
            let (execution_id, dot_id, payload, at) = row.map_err(storage_err)?;
            let at = parse_ts(&at)?;
            match serde_json::from_str::<ExecutionEvent>(&payload)? {
                ExecutionEvent::ToolCall { call_id, .. } => {
                    open.entry((execution_id, call_id)).or_insert(at);
                }
                ExecutionEvent::ToolResult { call_id, .. } => {
                    if let Some(from) = open.remove(&(execution_id.clone(), call_id)) {
                        if at > from { completed.push((execution_id, dot_id, from, at)); }
                    }
                }
                _ => {}
            }
        }
        drop(stmt);

        let mut stmt = self.conn.prepare(
            "SELECT a.execution_id, a.requested_at, a.resolved_at
             FROM approvals a JOIN executions e ON e.id = a.execution_id
             WHERE e.started_at < ?2 AND (e.finished_at IS NULL OR e.finished_at > ?1)
               AND a.requested_at < ?2 AND (a.resolved_at IS NULL OR a.resolved_at > ?1)",
        ).map_err(storage_err)?;
        let rows = stmt.query_map(params![ts(start), ts(end)], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        }).map_err(storage_err)?;
        let mut waits = HashMap::<String, Vec<(Timestamp, Timestamp)>>::new();
        let now = dots_core::now();
        for row in rows {
            let (execution_id, requested_at, resolved_at) = row.map_err(storage_err)?;
            let from = parse_ts(&requested_at)?;
            let to = match resolved_at { Some(at) => parse_ts(&at)?, None => now };
            if to > from { waits.entry(execution_id).or_default().push((from, to)); }
        }

        let mut intervals = Vec::new();
        for (execution_id, dot_id, from, to) in completed {
            let from = from.max(*start);
            let to = to.min(*end);
            if to <= from { continue; }
            let mut pieces = vec![(from, to)];
            for (wait_from, wait_to) in waits.get(&execution_id).into_iter().flatten() {
                pieces = pieces.into_iter().flat_map(|(a, b)| {
                    if *wait_to <= a || *wait_from >= b { vec![(a, b)] }
                    else {
                        let mut remaining = Vec::new();
                        if *wait_from > a { remaining.push((a, *wait_from)); }
                        if *wait_to < b { remaining.push((*wait_to, b)); }
                        remaining
                    }
                }).collect();
            }
            intervals.extend(pieces.into_iter().map(|(started_at, finished_at)| ObservedActivityInterval {
                dot_id: dot_id.clone(), started_at, finished_at,
            }));
        }
        Ok(intervals)
    }

    pub fn running(&self) -> DotsResult<Vec<Execution>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM executions WHERE status = 'RUNNING' ORDER BY started_at"))
            .map_err(storage_err)?;
        let rows = stmt.query_map([], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_exec)).collect()
    }

    pub fn create(&mut self, task_id: &str, dot_id: &str, engine: EngineKind, attempt: u32, workspace_path: Option<&str>) -> DotsResult<Execution> {
        let id = new_id();
        self.conn
            .execute(
                "INSERT INTO executions (id, task_id, dot_id, engine, attempt, status, workspace_path, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'RUNNING', ?6, ?7)",
                params![id, task_id, dot_id, engine.as_str(), attempt, workspace_path, ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn set_pid(&mut self, id: &str, pid: u32) -> DotsResult<()> {
        self.conn.execute("UPDATE executions SET pid = ?1 WHERE id = ?2", params![pid, id]).map_err(storage_err)?;
        Ok(())
    }

    pub fn set_session(&mut self, id: &str, session: &str) -> DotsResult<()> {
        self.conn
            .execute("UPDATE executions SET engine_session_id = ?1 WHERE id = ?2", params![session, id])
            .map_err(storage_err)?;
        Ok(())
    }

    pub fn finish(&mut self, id: &str, status: ExecutionStatus, exit_code: Option<i32>, error: Option<&str>, cost_usd: Option<f64>, usage: &serde_json::Value) -> DotsResult<Execution> {
        self.conn
            .execute(
                "UPDATE executions SET status = ?1, exit_code = ?2, error = ?3, cost_usd = COALESCE(?4, cost_usd), usage_json = ?5, finished_at = ?6 WHERE id = ?7",
                params![status.as_str(), exit_code, error, cost_usd, serde_json::to_string(usage)?, ts(&dots_core::now()), id],
            )
            .map_err(storage_err)?;
        self.get(id)
    }

    /// Marca como FAILED execucoes deixadas em RUNNING por um runtime anterior.
    pub fn mark_interrupted(&mut self) -> DotsResult<u32> {
        let n = self
            .conn
            .execute(
                "UPDATE executions SET status = 'FAILED', error = 'interrompida por reinicio do runtime', finished_at = ?1 WHERE status = 'RUNNING'",
                params![ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        Ok(n as u32)
    }

    /// Grava um evento com `seq` sequencial e atualiza o contador. Retorna o registro.
    pub fn add_event(&mut self, execution_id: &str, event: &ExecutionEvent) -> DotsResult<ExecutionEventRecord> {
        let tx = self.conn.transaction().map_err(storage_err)?;
        let seq_i: i64 = tx
            .query_row("SELECT events_count + 1 FROM executions WHERE id = ?1", params![execution_id], |r| r.get(0))
            .map_err(storage_err)?;
        let seq = seq_i.max(0) as u64;
        let id = new_id();
        let now = dots_core::now();
        let type_name = match serde_json::to_value(event)? {
            serde_json::Value::Object(m) => m.get("type").and_then(|t| t.as_str()).unwrap_or("unknown").to_string(),
            _ => "unknown".into(),
        };
        tx.execute(
            "INSERT INTO execution_events (id, execution_id, seq, type, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, execution_id, seq_i, type_name, serde_json::to_string(event)?, ts(&now)],
        )
        .map_err(storage_err)?;
        tx.execute("UPDATE executions SET events_count = ?1 WHERE id = ?2", params![seq_i, execution_id]).map_err(storage_err)?;
        tx.commit().map_err(storage_err)?;
        Ok(ExecutionEventRecord { id, execution_id: execution_id.to_string(), seq, event: event.clone(), created_at: now })
    }

    pub fn events(&self, execution_id: &str, after_seq: u64, limit: u32) -> DotsResult<Vec<ExecutionEventRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, execution_id, seq, payload_json, created_at FROM execution_events WHERE execution_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3")
            .map_err(storage_err)?;
        let rows = stmt
            .query_map(params![execution_id, after_seq as i64, limit], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?))
            })
            .map_err(storage_err)?;
        rows.map(|r| {
            let (id, execution_id, seq, payload, created_at) = r.map_err(storage_err)?;
            Ok(ExecutionEventRecord { id, execution_id, seq: seq.max(0) as u64, event: serde_json::from_str(&payload)?, created_at: parse_ts(&created_at)? })
        })
        .collect()
    }
}
