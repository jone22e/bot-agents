use dots_core::{DotsError, DotsResult, Team, TeamCreate, TeamUpdate, new_id};
use rusqlite::{Connection, OptionalExtension, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct TeamsRepo<'a> {
    conn: &'a mut Connection,
}

impl<'a> TeamsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    fn members_of(&self, team_id: &str) -> DotsResult<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT dot_id FROM team_members WHERE team_id = ?1 ORDER BY created_at").map_err(storage_err)?;
        let rows = stmt.query_map(params![team_id], |r| r.get::<_, String>(0)).map_err(storage_err)?;
        rows.collect::<Result<_, _>>().map_err(storage_err)
    }

    fn row(&self, r: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String, Option<String>, String, String)> {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
    }

    fn build(&self, raw: (String, String, String, Option<String>, String, String)) -> DotsResult<Team> {
        let (id, name, description, manager_dot_id, created_at, updated_at) = raw;
        let member_ids = self.members_of(&id)?;
        Ok(Team { id, name, description, manager_dot_id, member_ids, created_at: parse_ts(&created_at)?, updated_at: parse_ts(&updated_at)? })
    }

    pub fn list(&self) -> DotsResult<Vec<Team>> {
        let raws: Vec<_> = {
            let mut stmt = self.conn.prepare("SELECT id, name, description, manager_dot_id, created_at, updated_at FROM teams ORDER BY name COLLATE NOCASE").map_err(storage_err)?;
            let rows = stmt.query_map([], |r| self.row(r)).map_err(storage_err)?;
            rows.collect::<Result<_, _>>().map_err(storage_err)?
        };
        raws.into_iter().map(|r| self.build(r)).collect()
    }

    pub fn get(&self, id: &str) -> DotsResult<Team> {
        let raw = self
            .conn
            .query_row("SELECT id, name, description, manager_dot_id, created_at, updated_at FROM teams WHERE id = ?1", params![id], |r| self.row(r))
            .optional()
            .map_err(storage_err)?
            .ok_or_else(|| DotsError::NotFound(format!("equipe {id}")))?;
        self.build(raw)
    }

    fn dot_exists(&self, dot_id: &str) -> DotsResult<()> {
        let ok: Option<i64> = self.conn.query_row("SELECT 1 FROM dots WHERE id = ?1", params![dot_id], |r| r.get(0)).optional().map_err(storage_err)?;
        ok.map(|_| ()).ok_or_else(|| DotsError::NotFound(format!("dot {dot_id}")))
    }

    /// Equipe da qual o DOT e funcionario.
    pub fn member_of(&self, dot_id: &str) -> DotsResult<Option<Team>> {
        let id: Option<String> = self.conn.query_row("SELECT team_id FROM team_members WHERE dot_id = ?1", params![dot_id], |r| r.get(0)).optional().map_err(storage_err)?;
        id.map(|i| self.get(&i)).transpose()
    }

    /// Equipe que o DOT gerencia.
    pub fn managed_by(&self, dot_id: &str) -> DotsResult<Option<Team>> {
        let id: Option<String> = self.conn.query_row("SELECT id FROM teams WHERE manager_dot_id = ?1", params![dot_id], |r| r.get(0)).optional().map_err(storage_err)?;
        id.map(|i| self.get(&i)).transpose()
    }

    /// Cadeia de chefes acima de um DOT (gerente da equipe dele, gerente da equipe desse gerente, ...).
    fn boss_chain(&self, dot_id: &str) -> DotsResult<Vec<String>> {
        let mut out = vec![];
        let mut cur = dot_id.to_string();
        for _ in 0..16 {
            match self.member_of(&cur)?.and_then(|t| t.manager_dot_id) {
                Some(m) if !out.contains(&m) => { out.push(m.clone()); cur = m; }
                _ => break,
            }
        }
        Ok(out)
    }

    pub fn create(&mut self, input: TeamCreate) -> DotsResult<Team> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(DotsError::Validation("a equipe precisa de um nome".into()));
        }
        let id = new_id();
        let now = ts(&dots_core::now());
        self.conn
            .execute("INSERT INTO teams (id, name, description, manager_dot_id, created_at, updated_at) VALUES (?1, ?2, ?3, NULL, ?4, ?4)", params![id, name, input.description, now])
            .map_err(storage_err)?;
        if let Some(m) = input.manager_dot_id.as_deref() {
            self.set_manager(&id, Some(m))?;
        }
        for d in &input.member_ids {
            self.add_member(&id, d)?;
        }
        self.get(&id)
    }

    pub fn update(&mut self, id: &str, patch: TeamUpdate) -> DotsResult<Team> {
        let cur = self.get(id)?;
        let name = patch.name.map(|n| n.trim().to_string()).unwrap_or(cur.name);
        if name.is_empty() {
            return Err(DotsError::Validation("a equipe precisa de um nome".into()));
        }
        let description = patch.description.unwrap_or(cur.description);
        self.conn
            .execute("UPDATE teams SET name = ?2, description = ?3, updated_at = ?4 WHERE id = ?1", params![id, name, description, ts(&dots_core::now())])
            .map_err(storage_err)?;
        self.get(id)
    }

    pub fn delete(&mut self, id: &str) -> DotsResult<Team> {
        let t = self.get(id)?;
        self.conn.execute("DELETE FROM teams WHERE id = ?1", params![id]).map_err(storage_err)?;
        Ok(t)
    }

    /// Define (ou remove) o gerente. O gerente sai da lista de funcionarios da propria equipe.
    pub fn set_manager(&mut self, team_id: &str, dot_id: Option<&str>) -> DotsResult<Team> {
        let team = self.get(team_id)?;
        if let Some(d) = dot_id {
            self.dot_exists(d)?;
            if let Some(other) = self.managed_by(d)? {
                if other.id != team.id {
                    return Err(DotsError::Validation(format!("esse DOT ja e gerente da equipe \"{}\"", other.name)));
                }
            }
            // Ciclo: um chefe (direto ou indireto) de um funcionario nao pode virar subordinado deste gerente.
            let chain = self.boss_chain(d)?;
            if team.member_ids.iter().any(|m| chain.contains(m)) {
                return Err(DotsError::Validation("isso criaria um ciclo: um funcionario desta equipe ja e chefe desse DOT".into()));
            }
            self.conn.execute("DELETE FROM team_members WHERE team_id = ?1 AND dot_id = ?2", params![team_id, d]).map_err(storage_err)?;
        }
        self.conn
            .execute("UPDATE teams SET manager_dot_id = ?2, updated_at = ?3 WHERE id = ?1", params![team_id, dot_id, ts(&dots_core::now())])
            .map_err(storage_err)?;
        self.get(team_id)
    }

    /// Torna o DOT funcionario da equipe (saindo da equipe anterior, se houver).
    pub fn add_member(&mut self, team_id: &str, dot_id: &str) -> DotsResult<Team> {
        let team = self.get(team_id)?;
        self.dot_exists(dot_id)?;
        if team.manager_dot_id.as_deref() == Some(dot_id) {
            return Err(DotsError::Validation("o gerente da equipe nao pode ser funcionario dela".into()));
        }
        if let Some(m) = team.manager_dot_id.as_deref() {
            if m == dot_id || self.boss_chain(m)?.contains(&dot_id.to_string()) {
                return Err(DotsError::Validation("isso criaria um ciclo: esse DOT ja e chefe do gerente desta equipe".into()));
            }
        }
        self.conn.execute("DELETE FROM team_members WHERE dot_id = ?1", params![dot_id]).map_err(storage_err)?;
        self.conn
            .execute("INSERT INTO team_members (team_id, dot_id, created_at) VALUES (?1, ?2, ?3)", params![team_id, dot_id, ts(&dots_core::now())])
            .map_err(storage_err)?;
        self.get(team_id)
    }

    /// Tira o DOT da equipe em que e funcionario. Devolve a equipe afetada, se havia.
    pub fn remove_member(&mut self, dot_id: &str) -> DotsResult<Option<Team>> {
        let t = self.member_of(dot_id)?;
        self.conn.execute("DELETE FROM team_members WHERE dot_id = ?1", params![dot_id]).map_err(storage_err)?;
        t.map(|t| self.get(&t.id)).transpose()
    }
}
