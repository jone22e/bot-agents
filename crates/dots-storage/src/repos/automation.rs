use dots_core::{Checkpoint, Delegation, DelegationStatus, DotsError, DotsResult, EventRecord, EventSubscription, Schedule, ScheduleCreate, ScheduleKind, ScheduleUpdate, SubscriptionCreate, SubscriptionFilter, SubscriptionUpdate, Timestamp, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{parse_opt_ts, parse_ts, ts};
use crate::storage_err;

// ---------------------------------------------------------------- eventos

pub struct EventsRepo<'a> {
    conn: &'a mut Connection,
}

const EV_COLS: &str = "id, type, source, occurred_at, payload_json, correlation_id, idempotency_key, depth";

fn map_event(r: &Row<'_>) -> rusqlite::Result<(String, String, String, String, String, Option<String>, Option<String>, i64)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
}

fn to_event(r: (String, String, String, String, String, Option<String>, Option<String>, i64)) -> DotsResult<EventRecord> {
    let (id, event_type, source, occurred_at, payload, correlation_id, idempotency_key, depth) = r;
    Ok(EventRecord { id, event_type, source, occurred_at: parse_ts(&occurred_at)?, payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null), correlation_id, idempotency_key, depth: depth.max(0) as u32 })
}

impl<'a> EventsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Insere o evento. Retorna `None` se a chave de idempotencia ja existia (duplicado).
    pub fn insert(&mut self, event_type: &str, source: &str, payload: &serde_json::Value, correlation_id: Option<&str>, idempotency_key: Option<&str>, depth: u32) -> DotsResult<Option<EventRecord>> {
        let id = new_id();
        let now = dots_core::now();
        let n = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO events (id, type, source, occurred_at, payload_json, correlation_id, idempotency_key, depth) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, event_type, source, ts(&now), serde_json::to_string(payload)?, correlation_id, idempotency_key, depth as i64],
            )
            .map_err(storage_err)?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(EventRecord { id, event_type: event_type.into(), source: source.into(), occurred_at: now, payload: payload.clone(), correlation_id: correlation_id.map(String::from), idempotency_key: idempotency_key.map(String::from), depth }))
    }

    pub fn list(&self, event_type: Option<&str>, limit: u32) -> DotsResult<Vec<EventRecord>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {EV_COLS} FROM events WHERE (?1 IS NULL OR type = ?1) ORDER BY occurred_at DESC LIMIT ?2")).map_err(storage_err)?;
        let rows = stmt.query_map(params![event_type, if limit == 0 { 100 } else { limit }], map_event).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_event)).collect()
    }

    pub fn count_since(&self, since: &Timestamp) -> DotsResult<u32> {
        self.conn.query_row("SELECT COUNT(*) FROM events WHERE occurred_at > ?1", params![ts(since)], |r| r.get(0)).map_err(storage_err)
    }

    /// Registra a entrega (evento, gatilho). Retorna `false` se ja havia sido entregue.
    pub fn record_delivery(&mut self, event_id: &str, subscription_id: &str, task_id: Option<&str>, status: &str) -> DotsResult<bool> {
        let n = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO event_deliveries (id, event_id, subscription_id, task_id, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![new_id(), event_id, subscription_id, task_id, status, ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        Ok(n > 0)
    }

    pub fn set_delivery_task(&mut self, event_id: &str, subscription_id: &str, task_id: &str) -> DotsResult<()> {
        self.conn
            .execute("UPDATE event_deliveries SET task_id = ?1 WHERE event_id = ?2 AND subscription_id = ?3", params![task_id, event_id, subscription_id])
            .map_err(storage_err)?;
        Ok(())
    }

    pub fn prune(&mut self, keep: u32) -> DotsResult<u32> {
        let n = self
            .conn
            .execute("DELETE FROM events WHERE id NOT IN (SELECT id FROM events ORDER BY occurred_at DESC LIMIT ?1)", params![keep])
            .map_err(storage_err)?;
        Ok(n as u32)
    }
}

// ---------------------------------------------------------------- gatilhos

pub struct SubscriptionsRepo<'a> {
    conn: &'a mut Connection,
}

const SUB_COLS: &str = "id, dot_id, event_type, filter_json, prompt_template, retry_safe, cooldown_seconds, enabled, last_triggered_at, created_at";

type SubRaw = (String, String, String, String, String, i64, i64, i64, Option<String>, String);

fn map_sub(r: &Row<'_>) -> rusqlite::Result<SubRaw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?))
}

fn to_sub(r: SubRaw) -> DotsResult<EventSubscription> {
    let (id, dot_id, event_type, filter, prompt_template, retry_safe, cooldown, enabled, last, created_at) = r;
    Ok(EventSubscription {
        id, dot_id, event_type,
        filter: serde_json::from_str::<SubscriptionFilter>(&filter).unwrap_or_default(),
        prompt_template,
        retry_safe: retry_safe == 1,
        cooldown_seconds: cooldown.max(0) as u32,
        enabled: enabled == 1,
        last_triggered_at: parse_opt_ts(last)?,
        created_at: parse_ts(&created_at)?,
    })
}

impl<'a> SubscriptionsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<EventSubscription> {
        self.conn
            .query_row(&format!("SELECT {SUB_COLS} FROM event_subscriptions WHERE id = ?1"), params![id], map_sub)
            .optional()
            .map_err(storage_err)?
            .map(to_sub)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("gatilho {id}")))
    }

    pub fn list(&self, dot_id: Option<&str>) -> DotsResult<Vec<EventSubscription>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {SUB_COLS} FROM event_subscriptions WHERE (?1 IS NULL OR dot_id = ?1) ORDER BY created_at")).map_err(storage_err)?;
        let rows = stmt.query_map(params![dot_id], map_sub).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_sub)).collect()
    }

    pub fn enabled_for_type(&self, event_type: &str) -> DotsResult<Vec<EventSubscription>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {SUB_COLS} FROM event_subscriptions WHERE event_type = ?1 AND enabled = 1")).map_err(storage_err)?;
        let rows = stmt.query_map(params![event_type], map_sub).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_sub)).collect()
    }

    pub fn create(&mut self, input: SubscriptionCreate) -> DotsResult<EventSubscription> {
        if input.prompt_template.trim().is_empty() {
            return Err(DotsError::Validation("o pedido do gatilho e obrigatorio".into()));
        }
        if !dots_core::automation::event_types::SUBSCRIBABLE.contains(&input.event_type.as_str()) {
            return Err(DotsError::Validation(format!("tipo de evento nao permitido em gatilhos: {}", input.event_type)));
        }
        let id = new_id();
        self.conn
            .execute(
                "INSERT INTO event_subscriptions (id, dot_id, event_type, filter_json, prompt_template, retry_safe, cooldown_seconds, enabled, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
                params![id, input.dot_id, input.event_type, serde_json::to_string(&input.filter)?, input.prompt_template.trim(), input.retry_safe as i64, input.cooldown_seconds.unwrap_or(60) as i64, ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn update(&mut self, id: &str, patch: SubscriptionUpdate) -> DotsResult<EventSubscription> {
        let cur = self.get(id)?;
        let filter = patch.filter.unwrap_or(cur.filter);
        let prompt = patch.prompt_template.unwrap_or(cur.prompt_template);
        self.conn
            .execute(
                "UPDATE event_subscriptions SET filter_json = ?1, prompt_template = ?2, retry_safe = ?3, cooldown_seconds = ?4, enabled = ?5 WHERE id = ?6",
                params![serde_json::to_string(&filter)?, prompt, patch.retry_safe.unwrap_or(cur.retry_safe) as i64, patch.cooldown_seconds.unwrap_or(cur.cooldown_seconds) as i64, patch.enabled.unwrap_or(cur.enabled) as i64, id],
            )
            .map_err(storage_err)?;
        self.get(id)
    }

    pub fn delete(&mut self, id: &str) -> DotsResult<()> {
        let n = self.conn.execute("DELETE FROM event_subscriptions WHERE id = ?1", params![id]).map_err(storage_err)?;
        if n == 0 {
            return Err(DotsError::NotFound(format!("gatilho {id}")));
        }
        Ok(())
    }

    pub fn touch(&mut self, id: &str) -> DotsResult<()> {
        self.conn.execute("UPDATE event_subscriptions SET last_triggered_at = ?1 WHERE id = ?2", params![ts(&dots_core::now()), id]).map_err(storage_err)?;
        Ok(())
    }
}

// ---------------------------------------------------------------- agendamentos

pub struct SchedulesRepo<'a> {
    conn: &'a mut Connection,
}

const SCH_COLS: &str = "id, dot_id, kind, spec, prompt, enabled, next_run_at, last_run_at, created_at";

type SchRaw = (String, String, String, String, String, i64, Option<String>, Option<String>, String);

fn map_sch(r: &Row<'_>) -> rusqlite::Result<SchRaw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?))
}

fn to_sch(r: SchRaw) -> DotsResult<Schedule> {
    let (id, dot_id, kind, spec, prompt, enabled, next, last, created_at) = r;
    Ok(Schedule { id, dot_id, kind: ScheduleKind::parse(&kind)?, spec, prompt, enabled: enabled == 1, next_run_at: parse_opt_ts(next)?, last_run_at: parse_opt_ts(last)?, created_at: parse_ts(&created_at)? })
}

impl<'a> SchedulesRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Schedule> {
        self.conn
            .query_row(&format!("SELECT {SCH_COLS} FROM schedules WHERE id = ?1"), params![id], map_sch)
            .optional()
            .map_err(storage_err)?
            .map(to_sch)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("agendamento {id}")))
    }

    pub fn list(&self, dot_id: Option<&str>) -> DotsResult<Vec<Schedule>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {SCH_COLS} FROM schedules WHERE (?1 IS NULL OR dot_id = ?1) ORDER BY created_at")).map_err(storage_err)?;
        let rows = stmt.query_map(params![dot_id], map_sch).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_sch)).collect()
    }

    /// Agendamentos habilitados vencidos ate `now`.
    pub fn due(&self, now: &Timestamp) -> DotsResult<Vec<Schedule>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {SCH_COLS} FROM schedules WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?1 ORDER BY next_run_at")).map_err(storage_err)?;
        let rows = stmt.query_map(params![ts(now)], map_sch).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_sch)).collect()
    }

    /// Proximo horario entre todos os habilitados (para o loop dormir ate la).
    pub fn next_due(&self) -> DotsResult<Option<Timestamp>> {
        let v: Option<String> = self.conn.query_row("SELECT MIN(next_run_at) FROM schedules WHERE enabled = 1 AND next_run_at IS NOT NULL", [], |r| r.get(0)).map_err(storage_err)?;
        parse_opt_ts(v)
    }

    pub fn create(&mut self, input: ScheduleCreate, next_run_at: Option<&Timestamp>) -> DotsResult<Schedule> {
        if input.prompt.trim().is_empty() {
            return Err(DotsError::Validation("o pedido do agendamento e obrigatorio".into()));
        }
        let id = new_id();
        self.conn
            .execute(
                "INSERT INTO schedules (id, dot_id, kind, spec, prompt, enabled, next_run_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7)",
                params![id, input.dot_id, input.kind.as_str(), input.spec.trim(), input.prompt.trim(), next_run_at.map(ts), ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn update(&mut self, id: &str, patch: ScheduleUpdate, next_run_at: Option<&Timestamp>) -> DotsResult<Schedule> {
        let cur = self.get(id)?;
        self.conn
            .execute(
                "UPDATE schedules SET kind = ?1, spec = ?2, prompt = ?3, enabled = ?4, next_run_at = ?5 WHERE id = ?6",
                params![patch.kind.unwrap_or(cur.kind).as_str(), patch.spec.unwrap_or(cur.spec), patch.prompt.unwrap_or(cur.prompt), patch.enabled.unwrap_or(cur.enabled) as i64, next_run_at.map(ts), id],
            )
            .map_err(storage_err)?;
        self.get(id)
    }

    pub fn mark_run(&mut self, id: &str, next_run_at: Option<&Timestamp>) -> DotsResult<()> {
        self.conn
            .execute("UPDATE schedules SET last_run_at = ?1, next_run_at = ?2 WHERE id = ?3", params![ts(&dots_core::now()), next_run_at.map(ts), id])
            .map_err(storage_err)?;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> DotsResult<()> {
        let n = self.conn.execute("DELETE FROM schedules WHERE id = ?1", params![id]).map_err(storage_err)?;
        if n == 0 {
            return Err(DotsError::NotFound(format!("agendamento {id}")));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- checkpoints e invocacoes

pub struct CheckpointsRepo<'a> {
    conn: &'a mut Connection,
}

const CP_COLS: &str = "id, task_id, execution_id, step, state_json, context, results, dependencies_json, next_action, artifacts_json, source, created_at";

type CpRaw = (String, String, String, String, String, String, String, String, String, String, String, String);

fn map_cp(r: &Row<'_>) -> rusqlite::Result<CpRaw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?))
}

fn to_cp(r: CpRaw) -> DotsResult<Checkpoint> {
    let (id, task_id, execution_id, step, state, context, results, deps, next_action, artifacts, source, created_at) = r;
    Ok(Checkpoint {
        id, task_id, execution_id, step,
        state: serde_json::from_str(&state).unwrap_or(serde_json::Value::Null),
        context, results,
        dependencies: serde_json::from_str(&deps).unwrap_or_default(),
        next_action,
        artifacts: serde_json::from_str(&artifacts).unwrap_or_default(),
        source,
        created_at: parse_ts(&created_at)?,
    })
}

impl<'a> CheckpointsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(&mut self, task_id: &str, execution_id: &str, step: &str, state: &serde_json::Value, context: &str, results: &str, dependencies: &[String], next_action: &str, artifacts: &[String], source: &str) -> DotsResult<Checkpoint> {
        let id = new_id();
        self.conn
            .execute(
                "INSERT INTO checkpoints (id, task_id, execution_id, step, state_json, context, results, dependencies_json, next_action, artifacts_json, source, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![id, task_id, execution_id, step, serde_json::to_string(state)?, context, results, serde_json::to_string(dependencies)?, next_action, serde_json::to_string(artifacts)?, source, ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        // Mantem os 50 mais recentes por tarefa.
        self.conn
            .execute("DELETE FROM checkpoints WHERE task_id = ?1 AND id NOT IN (SELECT id FROM checkpoints WHERE task_id = ?1 ORDER BY created_at DESC LIMIT 50)", params![task_id])
            .map_err(storage_err)?;
        self.latest(task_id)?.ok_or_else(|| DotsError::Internal("checkpoint nao gravado".into()))
    }

    pub fn latest(&self, task_id: &str) -> DotsResult<Option<Checkpoint>> {
        self.conn
            .query_row(&format!("SELECT {CP_COLS} FROM checkpoints WHERE task_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1"), params![task_id], map_cp)
            .optional()
            .map_err(storage_err)?
            .map(to_cp)
            .transpose()
    }

    pub fn list(&self, task_id: &str) -> DotsResult<Vec<Checkpoint>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {CP_COLS} FROM checkpoints WHERE task_id = ?1 ORDER BY created_at, rowid")).map_err(storage_err)?;
        let rows = stmt.query_map(params![task_id], map_cp).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_cp)).collect()
    }

    /// Invocacao identica ja concluida com sucesso nesta tarefa (em qualquer tentativa).
    pub fn find_invocation(&self, task_id: &str, tool: &str, input_hash: &str) -> DotsResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT output FROM tool_invocations WHERE task_id = ?1 AND tool = ?2 AND input_hash = ?3 AND outcome = 'ok' ORDER BY created_at DESC LIMIT 1",
                params![task_id, tool, input_hash],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_err)
    }

    pub fn record_invocation(&mut self, task_id: &str, execution_id: &str, tool: &str, input_hash: &str, outcome: &str, output: &str) -> DotsResult<()> {
        self.conn
            .execute(
                "INSERT INTO tool_invocations (id, task_id, execution_id, tool, input_hash, outcome, output, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![new_id(), task_id, execution_id, tool, input_hash, outcome, output.chars().take(8000).collect::<String>(), ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        Ok(())
    }
}

// ---------------------------------------------------------------- delegacoes

pub struct DelegationsRepo<'a> {
    conn: &'a mut Connection,
}

const DL_COLS: &str = "x.id, x.from_dot_id, COALESCE(f.name, '?'), x.to_dot_id, COALESCE(t.name, '?'), x.parent_task_id, x.child_task_id, x.correlation_id, x.request, x.result, x.status, x.created_at, x.resolved_at";
const DL_FROM: &str = "delegations x LEFT JOIN dots f ON f.id = x.from_dot_id LEFT JOIN dots t ON t.id = x.to_dot_id";

type DlRaw = (String, String, String, String, String, String, Option<String>, String, String, Option<String>, String, String, Option<String>);

fn map_dl(r: &Row<'_>) -> rusqlite::Result<DlRaw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?))
}

fn to_dl(r: DlRaw) -> DotsResult<Delegation> {
    let (id, from_dot_id, from_dot_name, to_dot_id, to_dot_name, parent_task_id, child_task_id, correlation_id, request, result, status, created_at, resolved_at) = r;
    Ok(Delegation { id, from_dot_id, from_dot_name, to_dot_id, to_dot_name, parent_task_id, child_task_id, correlation_id, request, result, status: DelegationStatus::parse(&status), created_at: parse_ts(&created_at)?, resolved_at: parse_opt_ts(resolved_at)? })
}

impl<'a> DelegationsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Delegation> {
        self.conn
            .query_row(&format!("SELECT {DL_COLS} FROM {DL_FROM} WHERE x.id = ?1"), params![id], map_dl)
            .optional()
            .map_err(storage_err)?
            .map(to_dl)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("delegacao {id}")))
    }

    pub fn create(&mut self, from_dot_id: &str, to_dot_id: &str, parent_task_id: &str, child_task_id: &str, correlation_id: &str, request: &str) -> DotsResult<Delegation> {
        let id = new_id();
        self.conn
            .execute(
                "INSERT INTO delegations (id, from_dot_id, to_dot_id, parent_task_id, child_task_id, correlation_id, request, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
                params![id, from_dot_id, to_dot_id, parent_task_id, child_task_id, correlation_id, request, ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn by_child(&self, child_task_id: &str) -> DotsResult<Option<Delegation>> {
        self.conn
            .query_row(&format!("SELECT {DL_COLS} FROM {DL_FROM} WHERE x.child_task_id = ?1"), params![child_task_id], map_dl)
            .optional()
            .map_err(storage_err)?
            .map(to_dl)
            .transpose()
    }

    pub fn for_parent(&self, parent_task_id: &str) -> DotsResult<Vec<Delegation>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {DL_COLS} FROM {DL_FROM} WHERE x.parent_task_id = ?1 ORDER BY x.created_at")).map_err(storage_err)?;
        let rows = stmt.query_map(params![parent_task_id], map_dl).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_dl)).collect()
    }

    pub fn list_recent(&self, limit: u32) -> DotsResult<Vec<Delegation>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {DL_COLS} FROM {DL_FROM} ORDER BY x.created_at DESC LIMIT ?1")).map_err(storage_err)?;
        let rows = stmt.query_map(params![if limit == 0 { 100 } else { limit }], map_dl).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_dl)).collect()
    }

    /// Troca a tarefa filha pela continuacao (apos pergunta respondida) e reabre a delegacao.
    pub fn set_child(&mut self, id: &str, child_task_id: &str) -> DotsResult<Delegation> {
        self.conn
            .execute("UPDATE delegations SET child_task_id = ?1, status = 'pending', resolved_at = NULL WHERE id = ?2", params![child_task_id, id])
            .map_err(storage_err)?;
        self.get(id)
    }

    pub fn set_status(&mut self, id: &str, status: DelegationStatus, result: Option<&str>) -> DotsResult<Delegation> {
        let resolved = if status.is_terminal() { Some(ts(&dots_core::now())) } else { None };
        self.conn
            .execute("UPDATE delegations SET status = ?1, result = COALESCE(?2, result), resolved_at = COALESCE(?3, resolved_at) WHERE id = ?4", params![status.as_str(), result, resolved, id])
            .map_err(storage_err)?;
        self.get(id)
    }
}
