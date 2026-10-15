use dots_core::{DotsResult, EngineConfiguration, EngineDetection, EngineKind};
use rusqlite::{Connection, OptionalExtension, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct EnginesRepo<'a> {
    conn: &'a mut Connection,
}

type Row = (String, i64, Option<String>, Option<String>, Option<String>, String);

fn to_config(row: Row) -> DotsResult<EngineConfiguration> {
    let (engine, enabled, exe, model, detection, updated_at) = row;
    Ok(EngineConfiguration {
        kind: EngineKind::parse(&engine)?,
        enabled: enabled == 1,
        executable_override: exe,
        model,
        last_detection: detection.map(|d| serde_json::from_str::<EngineDetection>(&d)).transpose()?,
        updated_at: parse_ts(&updated_at)?,
    })
}

impl<'a> EnginesRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, kind: EngineKind) -> DotsResult<Option<EngineConfiguration>> {
        self.conn
            .query_row(
                "SELECT engine, enabled, executable_override, model, last_detection_json, updated_at
                 FROM engine_configurations WHERE engine = ?1",
                params![kind.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()
            .map_err(storage_err)?
            .map(to_config)
            .transpose()
    }

    /// Lista os tres motores; os ainda nao detectados vem com valores padrao.
    pub fn list(&self) -> DotsResult<Vec<EngineConfiguration>> {
        let mut out = Vec::with_capacity(3);
        for kind in EngineKind::ALL {
            out.push(self.get(kind)?.unwrap_or(EngineConfiguration {
                kind,
                enabled: true,
                executable_override: None,
                model: None,
                last_detection: None,
                updated_at: dots_core::now(),
            }));
        }
        Ok(out)
    }

    pub fn save_detection(&mut self, detection: &EngineDetection) -> DotsResult<EngineConfiguration> {
        let now = dots_core::now();
        self.conn
            .execute(
                "INSERT INTO engine_configurations (engine, enabled, executable_override, last_detection_json, updated_at)
                 VALUES (?1, 1, NULL, ?2, ?3)
                 ON CONFLICT(engine) DO UPDATE SET last_detection_json = excluded.last_detection_json,
                                                   updated_at = excluded.updated_at",
                params![detection.kind.as_str(), serde_json::to_string(detection)?, ts(&now)],
            )
            .map_err(storage_err)?;
        self.get(detection.kind)?
            .ok_or_else(|| dots_core::DotsError::Internal("configuracao do motor nao gravada".into()))
    }

    pub fn set_enabled(&mut self, kind: EngineKind, enabled: bool) -> DotsResult<EngineConfiguration> {
        self.upsert_flag(kind, |conn, now| {
            conn.execute(
                "INSERT INTO engine_configurations (engine, enabled, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(engine) DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at",
                params![kind.as_str(), enabled as i64, now],
            )
        })
    }

    pub fn set_executable_override(&mut self, kind: EngineKind, path: Option<&str>) -> DotsResult<EngineConfiguration> {
        self.upsert_flag(kind, |conn, now| {
            conn.execute(
                "INSERT INTO engine_configurations (engine, enabled, executable_override, updated_at) VALUES (?1, 1, ?2, ?3)
                 ON CONFLICT(engine) DO UPDATE SET executable_override = excluded.executable_override, updated_at = excluded.updated_at",
                params![kind.as_str(), path, now],
            )
        })
    }

    /// Modelo padrao do motor; `None` volta ao padrao do proprio CLI.
    pub fn set_model(&mut self, kind: EngineKind, model: Option<&str>) -> DotsResult<EngineConfiguration> {
        let model = dots_core::engine::validate_model(model)?;
        self.upsert_flag(kind, |conn, now| {
            conn.execute(
                "INSERT INTO engine_configurations (engine, enabled, model, updated_at) VALUES (?1, 1, ?2, ?3)
                 ON CONFLICT(engine) DO UPDATE SET model = excluded.model, updated_at = excluded.updated_at",
                params![kind.as_str(), model, now],
            )
        })
    }

    fn upsert_flag<F>(&mut self, kind: EngineKind, f: F) -> DotsResult<EngineConfiguration>
    where
        F: FnOnce(&Connection, String) -> rusqlite::Result<usize>,
    {
        let now = ts(&dots_core::now());
        f(self.conn, now).map_err(storage_err)?;
        self.get(kind)?
            .ok_or_else(|| dots_core::DotsError::Internal("configuracao do motor nao gravada".into()))
    }
}
