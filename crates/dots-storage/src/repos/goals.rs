use dots_core::{DotsError, DotsResult, Goal, GoalCreate, GoalStatus, GoalUpdate, Timestamp, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{parse_opt_ts, parse_ts, ts};
use crate::storage_err;

/// Metas de um DOT e os lancamentos de progresso.
pub struct GoalsRepo<'a> {
    conn: &'a mut Connection,
}

const COLS: &str = "id, dot_id, title, unit, target, current, period_start, period_end, status, task_id, created_at, updated_at, closed_at";

type Raw = (String, String, String, String, f64, f64, String, Option<String>, String, Option<String>, String, String, Option<String>);

fn map(r: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?))
}

fn build(r: Raw) -> DotsResult<Goal> {
    let (id, dot_id, title, unit, target, current, period_start, period_end, status, task_id, created_at, updated_at, closed_at) = r;
    Ok(Goal {
        id,
        dot_id,
        title,
        unit,
        target,
        current,
        period_start: parse_ts(&period_start)?,
        period_end: parse_opt_ts(period_end)?,
        status: GoalStatus::parse(&status)?,
        task_id,
        created_at: parse_ts(&created_at)?,
        updated_at: parse_ts(&updated_at)?,
        closed_at: parse_opt_ts(closed_at)?,
    })
}

const UPD_COLS: &str = "id, goal_id, value, delta, note, task_id, created_at";
type UpdRaw = (String, String, f64, f64, String, Option<String>, String);

fn map_upd(r: &Row<'_>) -> rusqlite::Result<UpdRaw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
}

fn build_upd(r: UpdRaw) -> DotsResult<GoalUpdate> {
    let (id, goal_id, value, delta, note, task_id, created_at) = r;
    Ok(GoalUpdate { id, goal_id, value, delta, note, task_id, created_at: parse_ts(&created_at)? })
}

/// Campos que podem mudar numa meta ativa; `None` mantem o valor atual.
#[derive(Debug, Default, Clone)]
pub struct GoalEdit<'a> {
    pub title: Option<&'a str>,
    pub unit: Option<&'a str>,
    pub target: Option<f64>,
    /// `Some(None)` remove o prazo.
    pub period_end: Option<Option<Timestamp>>,
}

impl<'a> GoalsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Goal> {
        self.conn
            .query_row(&format!("SELECT {COLS} FROM goals WHERE id = ?1"), params![id], map)
            .optional()
            .map_err(storage_err)?
            .map(build)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("meta {id}")))
    }

    /// Metas de um DOT (ou de todos), as ativas primeiro e depois as encerradas, mais recentes antes.
    pub fn list(&self, dot_id: Option<&str>, only_active: bool) -> DotsResult<Vec<Goal>> {
        let status_sql = if only_active { "AND status = 'active'" } else { "" };
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM goals WHERE (?1 IS NULL OR dot_id = ?1) {status_sql} ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END, created_at DESC"))
            .map_err(storage_err)?;
        let rows = stmt.query_map(params![dot_id], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(build)).collect()
    }

    pub fn create(&mut self, input: GoalCreate) -> DotsResult<Goal> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(DotsError::Validation("a meta precisa de um titulo".into()));
        }
        if !input.target.is_finite() || input.target <= 0.0 {
            return Err(DotsError::Validation("o alvo da meta precisa ser um numero maior que zero".into()));
        }
        if !input.current.is_finite() || input.current < 0.0 {
            return Err(DotsError::Validation("o valor atual da meta nao pode ser negativo".into()));
        }
        let now = dots_core::now();
        if input.period_end.is_some_and(|end| end <= now) {
            return Err(DotsError::Validation("o prazo da meta ja passou".into()));
        }
        let id = new_id();
        let tx = self.conn.transaction().map_err(storage_err)?;
        tx.execute(
                "INSERT INTO goals (id, dot_id, title, unit, target, current, period_start, period_end, status, task_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', ?9, ?7, ?7)",
                params![id, input.dot_id, title, input.unit.trim(), input.target, input.current, ts(&now), input.period_end.as_ref().map(ts), input.task_id],
            )
            .map_err(storage_err)?;
        if input.current > 0.0 {
            tx.execute(
                    "INSERT INTO goal_updates (id, goal_id, value, delta, note, task_id, created_at) VALUES (?1, ?2, ?3, ?3, 'valor inicial', ?4, ?5)",
                    params![new_id(), id, input.current, input.task_id, ts(&now)],
                )
                .map_err(storage_err)?;
        }
        tx.commit().map_err(storage_err)?;
        self.get(&id)
    }

    /// Lanca o progresso: `value` e o acumulado novo. Devolve a meta atualizada e o lancamento.
    pub fn progress(&mut self, id: &str, value: f64, note: &str, task_id: Option<&str>) -> DotsResult<(Goal, GoalUpdate)> {
        if !value.is_finite() || value < 0.0 {
            return Err(DotsError::Validation("o valor do progresso precisa ser um numero nao negativo".into()));
        }
        let goal = self.get(id)?;
        if goal.status != GoalStatus::Active {
            return Err(DotsError::Validation(format!("a meta '{}' ja foi encerrada", goal.title)));
        }
        let now = dots_core::now();
        let delta = value - goal.current;
        let uid = new_id();
        let tx = self.conn.transaction().map_err(storage_err)?;
        tx.execute(
                "INSERT INTO goal_updates (id, goal_id, value, delta, note, task_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![uid, id, value, delta, note.trim(), task_id, ts(&now)],
            )
            .map_err(storage_err)?;
        tx.execute("UPDATE goals SET current = ?1, updated_at = ?2 WHERE id = ?3", params![value, ts(&now), id])
            .map_err(storage_err)?;
        tx.commit().map_err(storage_err)?;
        let upd = self
            .conn
            .query_row(&format!("SELECT {UPD_COLS} FROM goal_updates WHERE id = ?1"), params![uid], map_upd)
            .map_err(storage_err)
            .and_then(build_upd)?;
        Ok((self.get(id)?, upd))
    }

    /// Altera titulo, unidade, alvo ou prazo de uma meta ativa, sem mexer no progresso nem no historico.
    pub fn update(&mut self, id: &str, edit: GoalEdit<'_>) -> DotsResult<Goal> {
        let goal = self.get(id)?;
        if goal.status != GoalStatus::Active {
            return Err(DotsError::Validation(format!("a meta '{}' ja foi encerrada", goal.title)));
        }
        let title = edit.title.map(str::trim).unwrap_or(&goal.title);
        if title.is_empty() {
            return Err(DotsError::Validation("a meta precisa de um titulo".into()));
        }
        let unit = edit.unit.map(str::trim).unwrap_or(&goal.unit);
        let target = edit.target.unwrap_or(goal.target);
        if !target.is_finite() || target <= 0.0 {
            return Err(DotsError::Validation("o alvo da meta precisa ser um numero maior que zero".into()));
        }
        let now = dots_core::now();
        let period_end = edit.period_end.unwrap_or(goal.period_end);
        if period_end.is_some_and(|end| end <= now) {
            return Err(DotsError::Validation("o prazo da meta ja passou".into()));
        }
        self.conn
            .execute(
                "UPDATE goals SET title = ?1, unit = ?2, target = ?3, period_end = ?4, updated_at = ?5 WHERE id = ?6",
                params![title, unit, target, period_end.as_ref().map(ts), ts(&now), id],
            )
            .map_err(storage_err)?;
        self.get(id)
    }

    /// Encerra a meta como cumprida ou cancelada.
    pub fn close(&mut self, id: &str, status: GoalStatus) -> DotsResult<Goal> {
        if status == GoalStatus::Active {
            return Err(DotsError::Validation("use done ou cancelled para encerrar".into()));
        }
        let goal = self.get(id)?;
        if goal.status != GoalStatus::Active {
            return Err(DotsError::Validation(format!("a meta '{}' ja foi encerrada", goal.title)));
        }
        let now = ts(&dots_core::now());
        self.conn
            .execute("UPDATE goals SET status = ?1, closed_at = ?2, updated_at = ?2 WHERE id = ?3", params![status.as_str(), now, id])
            .map_err(storage_err)?;
        self.get(id)
    }

    /// Lancamentos de uma meta, os mais recentes primeiro.
    pub fn history(&self, goal_id: &str, limit: u32) -> DotsResult<Vec<GoalUpdate>> {
        let limit = if limit == 0 { 50 } else { limit };
        let mut stmt = self.conn.prepare(&format!("SELECT {UPD_COLS} FROM goal_updates WHERE goal_id = ?1 ORDER BY created_at DESC LIMIT ?2")).map_err(storage_err)?;
        let rows = stmt.query_map(params![goal_id, limit], map_upd).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(build_upd)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Storage;

    fn open() -> Storage {
        Storage::open_in_memory().unwrap()
    }

    #[test]
    fn cria_lanca_progresso_e_encerra() {
        let mut st = open();
        let dot = st.dots().create("user", dots_core::DotCreate { name: "Gerente".into(), ..Default::default() }, None).unwrap();
        let g = st.goals().create(GoalCreate { dot_id: dot.id.clone(), title: " Envios ".into(), unit: "envios".into(), target: 10.0, current: 2.0, period_end: None, task_id: Some("t1".into()) }).unwrap();
        assert_eq!(g.title, "Envios");
        assert_eq!(g.current, 2.0);
        assert_eq!(st.goals().history(&g.id, 10).unwrap().len(), 1, "valor inicial vira lancamento");
        let (g2, upd) = st.goals().progress(&g.id, 5.0, "rodada das 10h", Some("t2")).unwrap();
        assert_eq!(g2.current, 5.0);
        assert_eq!(upd.delta, 3.0);
        assert_eq!(st.goals().list(Some(&dot.id), true).unwrap().len(), 1);
        let closed = st.goals().close(&g.id, GoalStatus::Done).unwrap();
        assert_eq!(closed.status, GoalStatus::Done);
        assert!(closed.closed_at.is_some());
        assert!(st.goals().progress(&g.id, 6.0, "", None).is_err(), "meta encerrada nao recebe progresso");
        assert!(st.goals().list(Some(&dot.id), true).unwrap().is_empty());
        assert_eq!(st.goals().list(None, false).unwrap().len(), 1);
        assert!(st.goals().create(GoalCreate { dot_id: dot.id.clone(), title: "x".into(), unit: String::new(), target: 0.0, current: 0.0, period_end: None, task_id: None }).is_err());
    }

    #[test]
    fn altera_alvo_titulo_e_prazo_sem_perder_progresso() {
        let mut st = open();
        let dot = st.dots().create("user", dots_core::DotCreate { name: "Gerente".into(), ..Default::default() }, None).unwrap();
        let g = st.goals().create(GoalCreate { dot_id: dot.id.clone(), title: "Republicacao".into(), unit: "anuncios".into(), target: 50.0, current: 1.0, period_end: None, task_id: None }).unwrap();
        let prazo = dots_core::now() + chrono::Duration::days(1);
        let g2 = st.goals().update(&g.id, GoalEdit { target: Some(200.0), period_end: Some(Some(prazo)), ..Default::default() }).unwrap();
        assert_eq!((g2.target, g2.current, g2.title.as_str(), g2.unit.as_str()), (200.0, 1.0, "Republicacao", "anuncios"));
        assert_eq!(g2.period_end.map(|t| t.timestamp()), Some(prazo.timestamp()));
        assert_eq!(st.goals().history(&g.id, 10).unwrap().len(), 1, "editar nao cria lancamento");
        let g3 = st.goals().update(&g.id, GoalEdit { title: Some(" Republicacao 200/dia "), period_end: Some(None), ..Default::default() }).unwrap();
        assert_eq!((g3.title.as_str(), g3.period_end, g3.target), ("Republicacao 200/dia", None, 200.0));
        assert!(st.goals().update(&g.id, GoalEdit { target: Some(0.0), ..Default::default() }).is_err(), "alvo zero e invalido");
        assert!(st.goals().update(&g.id, GoalEdit { title: Some("  "), ..Default::default() }).is_err(), "titulo vazio e invalido");
        st.goals().close(&g.id, GoalStatus::Cancelled).unwrap();
        assert!(st.goals().update(&g.id, GoalEdit { target: Some(10.0), ..Default::default() }).is_err(), "meta encerrada nao muda");
    }
}
