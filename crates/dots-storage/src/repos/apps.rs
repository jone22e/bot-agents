use dots_core::{AppConnection, ConnectionStatus, DotIntegration, DotsError, DotsResult, new_id};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{parse_opt_ts, parse_ts, ts};
use crate::storage_err;

pub struct AppsRepo<'a> {
    conn: &'a mut Connection,
}

const COLS: &str = "id, kind, label, base_url, server, email, user_name, user_id, empresa, base, status, status_detail, remember_password, last_login_at, last_check_at, created_at";

type Raw = (String, String, String, String, Option<String>, String, Option<String>, Option<i64>, Option<String>, Option<String>, String, Option<String>, i64, Option<String>, Option<String>, String);

fn map(r: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?, r.get(15)?))
}

fn to_conn(r: Raw) -> DotsResult<AppConnection> {
    let (id, kind, label, base_url, server, email, user_name, user_id, empresa, base, status, status_detail, remember, last_login, last_check, created_at) = r;
    Ok(AppConnection { id, kind, label, base_url, server, email, user_name, user_id, empresa, base, status: ConnectionStatus::parse(&status), status_detail, remember_password: remember == 1, last_login_at: parse_opt_ts(last_login)?, last_check_at: parse_opt_ts(last_check)?, created_at: parse_ts(&created_at)? })
}

impl<'a> AppsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<AppConnection> {
        self.conn
            .query_row(&format!("SELECT {COLS} FROM app_connections WHERE id = ?1"), params![id], map)
            .optional()
            .map_err(storage_err)?
            .map(to_conn)
            .transpose()?
            .ok_or_else(|| DotsError::NotFound(format!("conexao {id}")))
    }

    pub fn list(&self, kind: Option<&str>) -> DotsResult<Vec<AppConnection>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {COLS} FROM app_connections WHERE (?1 IS NULL OR kind = ?1) ORDER BY created_at")).map_err(storage_err)?;
        let rows = stmt.query_map(params![kind], map).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_conn)).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(&mut self, kind: &str, label: &str, base_url: &str, server: Option<&str>, email: &str, status: ConnectionStatus, remember_password: bool) -> DotsResult<AppConnection> {
        let id = new_id();
        self.conn
            .execute(
                "INSERT INTO app_connections (id, kind, label, base_url, server, email, status, remember_password, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![id, kind, label, base_url.trim_end_matches('/'), server, email, status.as_str(), remember_password as i64, ts(&dots_core::now())],
            )
            .map_err(storage_err)?;
        self.get(&id)
    }

    pub fn set_session(&mut self, id: &str, user_name: Option<&str>, user_id: Option<i64>, empresa: Option<&str>, base: Option<&str>) -> DotsResult<()> {
        let now = ts(&dots_core::now());
        self.conn
            .execute(
                "UPDATE app_connections SET user_name = ?1, user_id = ?2, empresa = ?3, base = ?4, status = 'connected', status_detail = NULL, last_login_at = ?5, last_check_at = ?5 WHERE id = ?6",
                params![user_name, user_id, empresa, base, now, id],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    pub fn set_status(&mut self, id: &str, status: ConnectionStatus, detail: Option<&str>) -> DotsResult<()> {
        self.conn
            .execute("UPDATE app_connections SET status = ?1, status_detail = ?2, last_check_at = ?3 WHERE id = ?4", params![status.as_str(), detail, ts(&dots_core::now()), id])
            .map_err(storage_err)?;
        Ok(())
    }

    pub fn set_remember(&mut self, id: &str, remember: bool) -> DotsResult<()> {
        self.conn.execute("UPDATE app_connections SET remember_password = ?1 WHERE id = ?2", params![remember as i64, id]).map_err(storage_err)?;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> DotsResult<()> {
        let n = self.conn.execute("DELETE FROM app_connections WHERE id = ?1", params![id]).map_err(storage_err)?;
        if n == 0 {
            return Err(DotsError::NotFound(format!("conexao {id}")));
        }
        Ok(())
    }

    // ---- vinculos por DOT
    pub fn integration(&self, dot_id: &str, kind: &str) -> DotsResult<Option<DotIntegration>> {
        self.conn
            .query_row("SELECT dot_id, kind, connection_id, created_at, role_template, work_group FROM dot_integrations WHERE dot_id = ?1 AND kind = ?2", params![dot_id, kind], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?))
            })
            .optional()
            .map_err(storage_err)?
            .map(|(dot_id, kind, connection_id, created_at, role_template, work_group)| Ok(DotIntegration { dot_id, kind, connection_id, role_template, work_group, created_at: parse_ts(&created_at)? }))
            .transpose()
    }

    /// `role_template`/`work_group` em `None` preservam o que ja estava gravado ao trocar so a conexao.
    pub fn set_integration(&mut self, dot_id: &str, kind: &str, connection_id: Option<&str>, role_template: Option<&str>, work_group: Option<&str>) -> DotsResult<Option<DotIntegration>> {
        match connection_id {
            Some(c) => {
                self.get(c)?;
                self.conn
                    .execute(
                        "INSERT INTO dot_integrations (dot_id, kind, connection_id, created_at, role_template, work_group) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(dot_id, kind) DO UPDATE SET connection_id = excluded.connection_id, role_template = COALESCE(excluded.role_template, dot_integrations.role_template), work_group = COALESCE(excluded.work_group, dot_integrations.work_group)",
                        params![dot_id, kind, c, ts(&dots_core::now()), role_template, work_group],
                    )
                    .map_err(storage_err)?;
            }
            None => {
                self.conn.execute("DELETE FROM dot_integrations WHERE dot_id = ?1 AND kind = ?2", params![dot_id, kind]).map_err(storage_err)?;
            }
        }
        self.integration(dot_id, kind)
    }

    /// DOTs nao arquivados com area definida: (dot_id, work_group, role do DOT: "worker" | "supervisor").
    pub fn members(&self, kind: &str) -> DotsResult<Vec<(String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT i.dot_id, i.work_group, d.role FROM dot_integrations i JOIN dots d ON d.id = i.dot_id WHERE i.kind = ?1 AND i.work_group IS NOT NULL AND d.status <> 'archived' ORDER BY d.created_at")
            .map_err(storage_err)?;
        let rows = stmt.query_map(params![kind], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).map_err(storage_err)?;
        rows.collect::<Result<_, _>>().map_err(storage_err)
    }

    pub fn dots_using(&self, connection_id: &str) -> DotsResult<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT dot_id FROM dot_integrations WHERE connection_id = ?1").map_err(storage_err)?;
        let rows = stmt.query_map(params![connection_id], |r| r.get::<_, String>(0)).map_err(storage_err)?;
        rows.collect::<Result<_, _>>().map_err(storage_err)
    }
}
