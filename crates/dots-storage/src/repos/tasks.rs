use dots_core::{DotsError, DotsResult, EngineKind, Task, TaskCreate, TaskKind, TaskStatus, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{parse_opt_ts, parse_ts, ts};
use crate::storage_err;

pub struct TasksRepo<'a> {
    conn: &'a mut Connection,
}

const COLS: &str = "id, dot_id, conversation_id, kind, title, prompt, status, priority, source, correlation_id, engine,
    retry_safe, cancel_requested, result_json, error, attempts, created_at, updated_at, started_at, finished_at,
    max_attempts, origin, parent_task_id, delegation_chain_json";

fn map(r: &Row<'_>) -> rusqlite::Result<RawTask> {
    Ok(RawTask {
        id: r.get(0)?, dot_id: r.get(1)?, conversation_id: r.get(2)?, kind: r.get(3)?, title: r.get(4)?, prompt: r.get(5)?,
        status: r.get(6)?, priority: r.get(7)?, source: r.get(8)?, correlation_id: r.get(9)?, engine: r.get(10)?,
        retry_safe: r.get(11)?, cancel_requested: r.get(12)?, result_json: r.get(13)?, error: r.get(14)?, attempts: r.get(15)?,
        created_at: r.get(16)?, updated_at: r.get(17)?, started_at: r.get(18)?, finished_at: r.get(19)?,
        max_attempts: r.get(20)?, origin: r.get(21)?, parent_task_id: r.get(22)?, delegation_chain_json: r.get(23)?,
    })
}

struct RawTask {
    id: String, dot_id: String, conversation_id: Option<String>, kind: String, title: String, prompt: String, status: String,
    priority: i32, source: String, correlation_id: Option<String>, engine: Option<String>, retry_safe: i64, cancel_requested: i64,
    result_json: Option<String>, error: Option<String>, attempts: u32, created_at: String, updated_at: String,
    started_at: Option<String>, finished_at: Option<String>,
    max_attempts: u32, origin: String, parent_task_id: Option<String>, delegation_chain_json: String,
}

fn to_task(r: RawTask) -> DotsResult<Task> {
    Ok(Task {
        id: r.id,
        dot_id: r.dot_id,
        conversation_id: r.conversation_id,
        kind: TaskKind::parse(&r.kind)?,
        title: r.title,
        prompt: r.prompt,
        status: TaskStatus::parse(&r.status)?,
        priority: r.priority,
        source: r.source,
        correlation_id: r.correlation_id,
        engine: r.engine.map(|e| EngineKind::parse(&e)).transpose()?,
        retry_safe: r.retry_safe == 1,
        cancel_requested: r.cancel_requested == 1,
        result_text: r.result_json.and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok()).and_then(|v| v.get("text").and_then(|t| t.as_str().map(String::from))),
        error: r.error,
        attempts: r.attempts,
        max_attempts: r.max_attempts,
        origin: r.origin,
        parent_task_id: r.parent_task_id,
        delegation_chain: serde_json::from_str(&r.delegation_chain_json).unwrap_or_default(),
        created_at: parse_ts(&r.created_at)?,
        updated_at: parse_ts(&r.updated_at)?,
        started_at: parse_opt_ts(r.started_at)?,
        finished_at: parse_opt_ts(r.finished_at)?,
    })
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub dot_id: Option<String>,
    pub status: Option<Vec<TaskStatus>>,
    pub conversation_id: Option<String>,
    pub limit: u32,
}

impl<'a> TasksRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Task> {
        self.conn
            .query_row(&format!("SELECT {COLS} FROM tasks WHERE id = ?1"), params![id], map)
            .optional()
            .map_err(storage_err)?
            .map(to_task)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("tarefa {id}")))
    }

    pub fn exists_origin(&self, origin: &str) -> DotsResult<bool> {
        let count: i64 = self.conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE origin = ?1", params![origin], |r| r.get(0))
            .map_err(storage_err)?;
        Ok(count > 0)
    }

    pub fn has_any_for_dot(&self, dot_id: &str) -> DotsResult<bool> {
        let count: i64 = self.conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE dot_id = ?1", params![dot_id], |r| r.get(0))
            .map_err(storage_err)?;
        Ok(count > 0)
    }

    pub fn has_open_for_dot(&self, dot_id: &str) -> DotsResult<bool> {
        let count: i64 = self.conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE dot_id = ?1 AND status NOT IN ('COMPLETED', 'FAILED', 'CANCELLED')",
                params![dot_id],
                |r| r.get(0),
            )
            .map_err(storage_err)?;
        Ok(count > 0)
    }

    pub fn list(&self, f: &TaskFilter) -> DotsResult<Vec<Task>> {
        let mut sql = format!("SELECT {COLS} FROM tasks WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(d) = &f.dot_id {
            sql.push_str(" AND dot_id = ?");
            args.push(Box::new(d.clone()));
        }
        if let Some(c) = &f.conversation_id {
            sql.push_str(" AND conversation_id = ?");
            args.push(Box::new(c.clone()));
        }
        if let Some(st) = &f.status {
            if !st.is_empty() {
                let ph = st.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                sql.push_str(&format!(" AND status IN ({ph})"));
                for s in st {
                    args.push(Box::new(s.as_str().to_string()));
                }
            }
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        args.push(Box::new(if f.limit == 0 { 200 } else { f.limit.min(2000) }));
        let mut stmt = self.conn.prepare(&sql).map_err(storage_err)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), map)
            .map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_task)).collect()
    }

    /// Proximas tarefas enfileiradas, por prioridade e ordem de chegada.
    pub fn queued(&self, limit: u32) -> DotsResult<Vec<Task>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM tasks WHERE status = 'QUEUED' ORDER BY priority DESC, created_at LIMIT ?1"))
            .map_err(storage_err)?;
        let rows = stmt.query_map(params![limit], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_task)).collect()
    }

    pub fn count_by_status(&self) -> DotsResult<Vec<(TaskStatus, u32)>> {
        let mut stmt = self.conn.prepare("SELECT status, COUNT(*) FROM tasks GROUP BY status").map_err(storage_err)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))).map_err(storage_err)?;
        rows.map(|r| {
            let (s, n) = r.map_err(storage_err)?;
            Ok((TaskStatus::parse(&s)?, n))
        })
        .collect()
    }

    pub fn running_count_for_dot(&self, dot_id: &str) -> DotsResult<u32> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE dot_id = ?1 AND status IN ('RUNNING','WAITING_APPROVAL')",
                params![dot_id],
                |r| r.get(0),
            )
            .map_err(storage_err)
    }

    pub fn create(&mut self, input: TaskCreate, engine: Option<EngineKind>) -> DotsResult<Task> {
        let prompt = input.prompt.trim().to_string();
        if prompt.is_empty() {
            return Err(DotsError::Validation("prompt e obrigatorio".into()));
        }
        let kind = input.kind.unwrap_or(TaskKind::Task);
        let title = input.title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
            let t: String = prompt.chars().take(80).collect();
            if prompt.chars().count() > 80 { format!("{t}…") } else { t }
        });
        let id = new_id();
        let now = ts(&dots_core::now());
        let origin = input.origin.clone().unwrap_or_else(|| "user".into());
        let source = origin.split(':').next().unwrap_or("user").to_string();
        let retry_safe = input.retry_safe.unwrap_or(false);
        let max_attempts = input.max_attempts.unwrap_or(if retry_safe { 2 } else { 1 }).max(1);
        self.conn
            .execute(
                "INSERT INTO tasks (id, dot_id, conversation_id, kind, title, prompt, status, priority, source, correlation_id, engine, retry_safe, attachments_json,
                    max_attempts, origin, parent_task_id, delegation_chain_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'QUEUED', ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)",
                params![
                    id,
                    input.dot_id,
                    input.conversation_id,
                    kind.as_str(),
                    title,
                    prompt,
                    input.priority.unwrap_or(0),
                    source,
                    input.correlation_id,
                    engine.map(|e| e.as_str().to_string()),
                    retry_safe as i64,
                    serde_json::to_string(&input.attachments)?,
                    max_attempts,
                    origin,
                    input.parent_task_id,
                    serde_json::to_string(&input.delegation_chain)?,
                    now
                ],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn attachments(&self, id: &str) -> DotsResult<Vec<String>> {
        let raw: String = self
            .conn
            .query_row("SELECT attachments_json FROM tasks WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(storage_err)?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    pub fn set_status(&mut self, id: &str, status: TaskStatus, error: Option<&str>) -> DotsResult<Task> {
        let now = ts(&dots_core::now());
        let started = if status == TaskStatus::Running { Some(now.clone()) } else { None };
        let finished = if status.is_terminal() { Some(now.clone()) } else { None };
        self.conn
            .execute(
                "UPDATE tasks SET status = ?1, error = COALESCE(?2, error), updated_at = ?3,
                    started_at = COALESCE(started_at, ?4),
                    finished_at = CASE WHEN ?5 IS NOT NULL THEN ?5 ELSE finished_at END,
                    attempts = CASE WHEN ?1 = 'RUNNING' THEN attempts + 1 ELSE attempts END
                 WHERE id = ?6",
                params![status.as_str(), error, now, started, finished, id],
            )
            .map_err(storage_err)?;
        self.get(id)
    }

    pub fn set_result(&mut self, id: &str, text: &str) -> DotsResult<()> {
        self.conn
            .execute(
                "UPDATE tasks SET result_json = ?1, updated_at = ?2 WHERE id = ?3",
                params![serde_json::to_string(&serde_json::json!({ "text": text }))?, ts(&dots_core::now()), id],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    pub fn request_cancel(&mut self, id: &str) -> DotsResult<Task> {
        self.conn
            .execute("UPDATE tasks SET cancel_requested = 1, updated_at = ?1 WHERE id = ?2", params![ts(&dots_core::now()), id])
            .map_err(storage_err)?;
        self.get(id)
    }

    /// Reenfileira uma tarefa terminal (nova tentativa manual).
    pub fn requeue(&mut self, id: &str) -> DotsResult<Task> {
        self.conn
            .execute(
                "UPDATE tasks SET status = 'QUEUED', cancel_requested = 0, error = NULL, finished_at = NULL, updated_at = ?1 WHERE id = ?2",
                params![ts(&dots_core::now()), id],
            )
            .map_err(storage_err)?;
        self.get(id)
    }

    pub fn set_origin(&mut self, id: &str, origin: &str) -> DotsResult<()> {
        self.conn.execute("UPDATE tasks SET origin = ?1 WHERE id = ?2", params![origin, id]).map_err(storage_err)?;
        Ok(())
    }

    pub fn children(&self, parent_task_id: &str) -> DotsResult<Vec<Task>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {COLS} FROM tasks WHERE parent_task_id = ?1 ORDER BY created_at")).map_err(storage_err)?;
        let rows = stmt.query_map(params![parent_task_id], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_task)).collect()
    }

    /// Na inicializacao: tarefas deixadas em RUNNING/WAITING_APPROVAL por um runtime morto.
    /// `retry_safe` com tentativas restantes volta a fila; as demais aguardam o usuario.
    pub fn recover_interrupted(&mut self) -> DotsResult<Vec<(Task, TaskStatus)>> {
        let stuck = self.list(&TaskFilter { status: Some(vec![TaskStatus::Running, TaskStatus::WaitingApproval]), limit: 1000, ..Default::default() })?;
        let mut out = vec![];
        for t in stuck {
            // Um reinicio do runtime nao mata o trabalho: a tarefa volta a fila (os pontos de controle evitam repetir
            // acoes). So fica aguardando o usuario quando os reinicios excedem as tentativas previstas.
            let can_retry = t.attempts <= t.max_attempts;
            let target = if can_retry { TaskStatus::Queued } else { TaskStatus::WaitingUser };
            let msg = "execucao interrompida por reinicio do runtime";
            let updated = self.set_status(&t.id, target, Some(msg))?;
            out.push((updated, target));
        }
        Ok(out)
    }
}
