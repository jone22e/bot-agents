use dots_core::dot::{validate_name, validate_text};
use dots_core::engine::validate_model;
use dots_core::{Dot, DotCreate, DotLimits, DotRole, DotStatus, DotUpdate, DotWorkspace, DotsError, DotsResult, EngineKind, new_id};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct DotsRepo<'a> {
    conn: &'a mut Connection,
}

const SELECT_DOT: &str = "
    SELECT d.id, d.name, d.description, d.objective, d.status, d.limits_json, d.created_at, d.updated_at,
           d.role,
           (SELECT content FROM dot_instructions i WHERE i.dot_id = d.id ORDER BY version DESC LIMIT 1),
           (SELECT engine  FROM dot_engines e WHERE e.dot_id = d.id AND e.is_primary = 1),
           (SELECT path    FROM dot_workspaces w WHERE w.dot_id = d.id AND w.is_primary = 1),
           (SELECT config_json FROM dot_engines e WHERE e.dot_id = d.id AND e.is_primary = 1)
    FROM dots d";

type DotRow = (String, String, String, String, String, String, String, String, String, Option<String>, Option<String>, Option<String>, Option<String>);

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<DotRow> {
    Ok((
        r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?,
        r.get(10)?, r.get(11)?, r.get(12)?,
    ))
}

fn to_dot(row: DotRow) -> DotsResult<Dot> {
    let (id, name, description, objective, status, limits, created_at, updated_at, role, instructions, engine, workspace, engine_config) = row;
    let model = engine_config
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(String::from));
    Ok(Dot {
        id,
        name,
        description,
        objective,
        instructions: instructions.unwrap_or_default(),
        primary_engine: engine.map(|e| EngineKind::parse(&e)).transpose()?,
        model,
        workspace_path: workspace,
        limits: serde_json::from_str(&limits).unwrap_or_default(),
        status: DotStatus::parse(&status)?,
        role: DotRole::parse(&role),
        created_at: parse_ts(&created_at)?,
        updated_at: parse_ts(&updated_at)?,
    })
}

fn audit(tx: &Transaction<'_>, actor: &str, action: &str, dot_id: &str, details: serde_json::Value) -> DotsResult<()> {
    tx.execute(
        "INSERT INTO audit_logs (id, actor, action, entity_type, entity_id, details_json, created_at)
         VALUES (?1, ?2, ?3, 'dot', ?4, ?5, ?6)",
        params![new_id(), actor, action, dot_id, serde_json::to_string(&details)?, ts(&dots_core::now())],
    )
    .map_err(storage_err)?;
    Ok(())
}

fn fetch(conn: &Connection, id: &str) -> DotsResult<Dot> {
    conn.query_row(&format!("{SELECT_DOT} WHERE d.id = ?1"), params![id], map_row)
        .optional()
        .map_err(storage_err)?
        .map(to_dot)
        .transpose()?
        .ok_or_else(|| DotsError::NotFound(format!("DOT {id}")))
}

/// O modelo vive em `config_json` do motor primario: trocar de motor descarta o modelo, que e especifico dele.
fn set_primary_engine(tx: &Transaction<'_>, dot_id: &str, engine: Option<EngineKind>, model: Option<&str>) -> DotsResult<()> {
    tx.execute("DELETE FROM dot_engines WHERE dot_id = ?1 AND is_primary = 1", params![dot_id])
        .map_err(storage_err)?;
    if let Some(e) = engine {
        let config = match model {
            Some(m) => serde_json::json!({ "model": m }),
            None => serde_json::json!({}),
        };
        tx.execute(
            "INSERT INTO dot_engines (id, dot_id, engine, is_primary, config_json, created_at) VALUES (?1, ?2, ?3, 1, ?4, ?5)",
            params![new_id(), dot_id, e.as_str(), config.to_string(), ts(&dots_core::now())],
        )
        .map_err(storage_err)?;
    }
    Ok(())
}

fn set_primary_workspace(tx: &Transaction<'_>, dot_id: &str, path: Option<&str>) -> DotsResult<()> {
    tx.execute("UPDATE dot_workspaces SET is_primary = 0 WHERE dot_id = ?1 AND is_primary = 1", params![dot_id])
        .map_err(storage_err)?;
    if let Some(p) = path {
        tx.execute(
            "INSERT INTO dot_workspaces (id, dot_id, path, is_primary, can_read, can_write, created_at)
             VALUES (?1, ?2, ?3, 1, 1, 1, ?4)
             ON CONFLICT(dot_id, path) DO UPDATE SET is_primary = 1",
            params![new_id(), dot_id, p, ts(&dots_core::now())],
        )
        .map_err(storage_err)?;
    }
    Ok(())
}

fn add_instructions_version(tx: &Transaction<'_>, dot_id: &str, content: &str) -> DotsResult<()> {
    tx.execute(
        "INSERT INTO dot_instructions (id, dot_id, version, content, created_at)
         VALUES (?1, ?2, (SELECT COALESCE(MAX(version), 0) + 1 FROM dot_instructions WHERE dot_id = ?2), ?3, ?4)",
        params![new_id(), dot_id, content, ts(&dots_core::now())],
    )
    .map_err(storage_err)?;
    Ok(())
}

impl<'a> DotsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, id: &str) -> DotsResult<Dot> {
        fetch(self.conn, id)
    }

    pub fn list(&self, include_archived: bool) -> DotsResult<Vec<Dot>> {
        let sql = if include_archived {
            format!("{SELECT_DOT} ORDER BY d.created_at")
        } else {
            format!("{SELECT_DOT} WHERE d.status != 'archived' ORDER BY d.created_at")
        };
        let mut stmt = self.conn.prepare(&sql).map_err(storage_err)?;
        let rows = stmt.query_map([], map_row).map_err(storage_err)?;
        rows.map(|r| r.map_err(storage_err).and_then(to_dot)).collect()
    }

    pub fn count_by_status(&self) -> DotsResult<Vec<(DotStatus, u32)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT status, COUNT(*) FROM dots GROUP BY status")
            .map_err(storage_err)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))
            .map_err(storage_err)?;
        rows.map(|r| {
            let (s, n) = r.map_err(storage_err)?;
            Ok((DotStatus::parse(&s)?, n))
        })
        .collect()
    }

    /// Cria o DOT com instrucoes v1, motor primario e workspace primario (transacao unica).
    pub fn create(&mut self, actor: &str, input: DotCreate, default_workspace: Option<String>) -> DotsResult<Dot> {
        let name = validate_name(&input.name)?;
        let description = validate_text("descricao", &input.description)?;
        let objective = validate_text("objetivo", &input.objective)?;
        let instructions = validate_text("instrucoes", &input.instructions)?;
        let limits = input.limits.unwrap_or_default();
        limits.validate()?;
        let model = validate_model(input.model.as_deref())?;
        let workspace = input.workspace_path.or(default_workspace);

        let id = new_id();
        let now = dots_core::now();
        let tx = self.conn.transaction().map_err(storage_err)?;
        tx.execute(
            "INSERT INTO dots (id, name, description, objective, status, limits_json, role, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, ?7)",
            params![id, name, description, objective, serde_json::to_string(&limits)?, input.role.unwrap_or_default().as_str(), ts(&now)],
        )
        .map_err(storage_err)?;
        add_instructions_version(&tx, &id, &instructions)?;
        set_primary_engine(&tx, &id, input.primary_engine, model.as_deref())?;
        set_primary_workspace(&tx, &id, workspace.as_deref())?;
        audit(&tx, actor, "dot.create", &id, serde_json::json!({ "name": name }))?;
        tx.commit().map_err(storage_err)?;
        fetch(self.conn, &id)
    }

    pub fn update(&mut self, actor: &str, id: &str, input: DotUpdate) -> DotsResult<Dot> {
        let current = fetch(self.conn, id)?;
        let mut changed: Vec<&str> = vec![];
        let name = match &input.name {
            Some(n) => {
                changed.push("name");
                validate_name(n)?
            }
            None => current.name.clone(),
        };
        let description = match &input.description {
            Some(v) => {
                changed.push("description");
                validate_text("descricao", v)?
            }
            None => current.description.clone(),
        };
        let objective = match &input.objective {
            Some(v) => {
                changed.push("objective");
                validate_text("objetivo", v)?
            }
            None => current.objective.clone(),
        };
        let limits: DotLimits = match &input.limits {
            Some(l) => {
                l.validate()?;
                changed.push("limits");
                l.clone()
            }
            None => current.limits.clone(),
        };
        let role = match input.role {
            Some(r) if r != current.role => {
                changed.push("role");
                r
            }
            _ => current.role,
        };
        let now = dots_core::now();
        let tx = self.conn.transaction().map_err(storage_err)?;
        tx.execute(
            "UPDATE dots SET name = ?1, description = ?2, objective = ?3, limits_json = ?4, updated_at = ?5, role = ?7 WHERE id = ?6",
            params![name, description, objective, serde_json::to_string(&limits)?, ts(&now), id, role.as_str()],
        )
        .map_err(storage_err)?;
        if let Some(instr) = &input.instructions {
            let instr = validate_text("instrucoes", instr)?;
            if instr != current.instructions {
                add_instructions_version(&tx, id, &instr)?;
                changed.push("instructions");
            }
        }
        let engine = input.primary_engine.unwrap_or(current.primary_engine);
        let engine_changed = engine != current.primary_engine;
        let model = match input.model {
            Some(m) => validate_model(m.as_deref())?,
            // Sem modelo no pedido: mantem o atual, salvo se o motor mudou (o modelo era do motor antigo).
            None if engine_changed => None,
            None => current.model.clone(),
        };
        if engine_changed || model != current.model {
            set_primary_engine(&tx, id, engine, model.as_deref())?;
            if engine_changed {
                changed.push("primary_engine");
            }
            if model != current.model {
                changed.push("model");
            }
        }
        if let Some(ws) = &input.workspace_path {
            if *ws != current.workspace_path {
                set_primary_workspace(&tx, id, ws.as_deref())?;
                changed.push("workspace_path");
            }
        }
        audit(&tx, actor, "dot.update", id, serde_json::json!({ "fields": changed }))?;
        tx.commit().map_err(storage_err)?;
        fetch(self.conn, id)
    }

    pub fn set_status(&mut self, actor: &str, id: &str, status: DotStatus) -> DotsResult<Dot> {
        let current = fetch(self.conn, id)?;
        if current.status == status {
            return Ok(current);
        }
        let tx = self.conn.transaction().map_err(storage_err)?;
        tx.execute(
            "UPDATE dots SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.as_str(), ts(&dots_core::now()), id],
        )
        .map_err(storage_err)?;
        let action = match status {
            DotStatus::Active => "dot.resume",
            DotStatus::Paused => "dot.pause",
            DotStatus::Archived => "dot.archive",
        };
        audit(&tx, actor, action, id, serde_json::json!({ "from": current.status.as_str(), "to": status.as_str() }))?;
        tx.commit().map_err(storage_err)?;
        fetch(self.conn, id)
    }

    /// Duplica identidade, instrucoes atuais, motor e workspace (status ativo).
    pub fn duplicate(&mut self, actor: &str, id: &str, new_name: Option<String>, default_workspace: Option<String>) -> DotsResult<Dot> {
        let source = fetch(self.conn, id)?;
        let name = new_name.unwrap_or_else(|| format!("{} (copia)", source.name));
        let copy = self.create(
            actor,
            DotCreate {
                name,
                description: source.description,
                objective: source.objective,
                instructions: source.instructions,
                primary_engine: source.primary_engine,
                model: source.model,
                workspace_path: None,
                limits: Some(source.limits),
                role: Some(source.role),
            },
            default_workspace,
        )?;
        let tx = self.conn.transaction().map_err(storage_err)?;
        audit(&tx, actor, "dot.duplicate", &copy.id, serde_json::json!({ "source_id": id }))?;
        tx.commit().map_err(storage_err)?;
        Ok(copy)
    }

    /// Exclusao definitiva. A confirmacao e responsabilidade da interface.
    pub fn delete(&mut self, actor: &str, id: &str) -> DotsResult<()> {
        let current = fetch(self.conn, id)?;
        let tx = self.conn.transaction().map_err(storage_err)?;
        tx.execute("DELETE FROM dots WHERE id = ?1", params![id]).map_err(storage_err)?;
        audit(&tx, actor, "dot.delete", id, serde_json::json!({ "name": current.name }))?;
        tx.commit().map_err(storage_err)?;
        Ok(())
    }

    /// Adiciona (ou atualiza) um diretorio externo autorizado.
    pub fn add_workspace(&mut self, actor: &str, dot_id: &str, path: &str, can_read: bool, can_write: bool) -> DotsResult<DotWorkspace> {
        fetch(self.conn, dot_id)?;
        let tx = self.conn.transaction().map_err(storage_err)?;
        tx.execute(
            "INSERT INTO dot_workspaces (id, dot_id, path, is_primary, can_read, can_write, is_external, created_at) VALUES (?1, ?2, ?3, 0, ?4, ?5, 1, ?6)
             ON CONFLICT(dot_id, path) DO UPDATE SET can_read = excluded.can_read, can_write = excluded.can_write",
            params![new_id(), dot_id, path, can_read as i64, can_write as i64, ts(&dots_core::now())],
        )
        .map_err(storage_err)?;
        audit(&tx, actor, "dot.workspace.add", dot_id, serde_json::json!({ "path": path, "can_read": can_read, "can_write": can_write }))?;
        tx.commit().map_err(storage_err)?;
        self.workspaces(dot_id)?.into_iter().find(|w| w.path == path).ok_or_else(|| DotsError::Internal("workspace nao gravado".into()))
    }

    pub fn remove_workspace(&mut self, actor: &str, dot_id: &str, path: &str) -> DotsResult<()> {
        let tx = self.conn.transaction().map_err(storage_err)?;
        let n = tx
            .execute("DELETE FROM dot_workspaces WHERE dot_id = ?1 AND path = ?2 AND is_primary = 0", params![dot_id, path])
            .map_err(storage_err)?;
        if n == 0 {
            return Err(DotsError::NotFound(format!("diretorio {path} (ou e o workspace primario)")));
        }
        audit(&tx, actor, "dot.workspace.remove", dot_id, serde_json::json!({ "path": path }))?;
        tx.commit().map_err(storage_err)?;
        Ok(())
    }

    pub fn workspaces(&self, dot_id: &str) -> DotsResult<Vec<DotWorkspace>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, dot_id, path, is_primary, can_read, can_write, created_at, is_external
                 FROM dot_workspaces WHERE dot_id = ?1 ORDER BY is_primary DESC, created_at",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map(params![dot_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, i64>(7)?,
                ))
            })
            .map_err(storage_err)?;
        rows.map(|r| {
            let (id, dot_id, path, is_primary, can_read, can_write, created_at, is_external) = r.map_err(storage_err)?;
            Ok(DotWorkspace {
                id,
                dot_id,
                path,
                is_primary: is_primary == 1,
                can_read: can_read == 1,
                can_write: can_write == 1,
                is_external: is_external == 1,
                created_at: parse_ts(&created_at)?,
            })
        })
        .collect()
    }

    pub fn instructions_history(&self, dot_id: &str) -> DotsResult<Vec<(u32, String, dots_core::Timestamp)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT version, content, created_at FROM dot_instructions WHERE dot_id = ?1 ORDER BY version DESC")
            .map_err(storage_err)?;
        let rows = stmt
            .query_map(params![dot_id], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
            .map_err(storage_err)?;
        rows.map(|r| {
            let (v, c, t) = r.map_err(storage_err)?;
            Ok((v, c, parse_ts(&t)?))
        })
        .collect()
    }
}
