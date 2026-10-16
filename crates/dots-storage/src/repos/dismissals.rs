use dots_core::{Dismissal, DotsError, DotsResult, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{parse_ts, ts};
use crate::storage_err;

/// Demissoes: registro permanente (motivo e licao) de cada vez que um gerente demitiu um funcionario.
pub struct DismissalsRepo<'a> {
    conn: &'a mut Connection,
}

const COLS: &str = "id, team_id, team_name, manager_dot_id, manager_name, dot_id, dot_name, reason, lesson, memories_wiped, created_at";

type Raw = (String, Option<String>, String, Option<String>, String, Option<String>, String, String, String, u32, String);

fn map(r: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?))
}

fn build(r: Raw) -> DotsResult<Dismissal> {
    let (id, team_id, team_name, manager_dot_id, manager_name, dot_id, dot_name, reason, lesson, memories_wiped, created_at) = r;
    Ok(Dismissal { id, team_id, team_name, manager_dot_id, manager_name, dot_id, dot_name, reason, lesson, memories_wiped, created_at: parse_ts(&created_at)? })
}

pub struct DismissalInput<'s> {
    pub team_id: Option<&'s str>,
    pub team_name: &'s str,
    pub manager_dot_id: &'s str,
    pub manager_name: &'s str,
    pub dot_id: &'s str,
    pub dot_name: &'s str,
    pub reason: &'s str,
    pub lesson: &'s str,
    pub memories_wiped: u32,
}

impl<'a> DismissalsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Dismissal> {
        self.conn
            .query_row(&format!("SELECT {COLS} FROM dismissals WHERE id = ?1"), params![id], map)
            .optional()
            .map_err(storage_err)?
            .map(build)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("demissao {id}")))
    }

    pub fn create(&mut self, input: DismissalInput<'_>) -> DotsResult<Dismissal> {
        let reason = input.reason.trim();
        if reason.is_empty() {
            return Err(DotsError::Validation("a demissao precisa de um motivo".into()));
        }
        let lesson = input.lesson.trim();
        let id = new_id();
        self.conn
            .execute(
                &format!("INSERT INTO dismissals ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"),
                params![id, input.team_id, input.team_name, input.manager_dot_id, input.manager_name, input.dot_id, input.dot_name, reason, lesson, input.memories_wiped, ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    fn query(&self, where_sql: &str, arg: &str, limit: u32) -> DotsResult<Vec<Dismissal>> {
        let limit = if limit == 0 { 100 } else { limit };
        let mut stmt = self.conn.prepare(&format!("SELECT {COLS} FROM dismissals WHERE {where_sql} ORDER BY created_at DESC LIMIT ?2")).map_err(storage_err)?;
        let rows = stmt.query_map(params![arg, limit], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(build)).collect()
    }

    /// Demissoes feitas na equipe (as licoes que todos os funcionarios dela recebem).
    pub fn for_team(&self, team_id: &str, limit: u32) -> DotsResult<Vec<Dismissal>> {
        self.query("team_id = ?1", team_id, limit)
    }

    /// Quantas vezes (e por que) este DOT ja foi demitido.
    pub fn for_dot(&self, dot_id: &str, limit: u32) -> DotsResult<Vec<Dismissal>> {
        self.query("dot_id = ?1", dot_id, limit)
    }

    /// Demissoes feitas por este gerente (em qualquer equipe).
    pub fn by_manager(&self, manager_dot_id: &str, limit: u32) -> DotsResult<Vec<Dismissal>> {
        self.query("manager_dot_id = ?1", manager_dot_id, limit)
    }

    pub fn recent(&self, limit: u32) -> DotsResult<Vec<Dismissal>> {
        let limit = if limit == 0 { 100 } else { limit };
        let mut stmt = self.conn.prepare(&format!("SELECT {COLS} FROM dismissals ORDER BY created_at DESC LIMIT ?1")).map_err(storage_err)?;
        let rows = stmt.query_map(params![limit], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(build)).collect()
    }
}
