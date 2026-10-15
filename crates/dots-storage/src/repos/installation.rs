use dots_core::{DotsResult, Installation, new_id};
use rusqlite::{Connection, OptionalExtension, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct InstallationRepo<'a> {
    conn: &'a mut Connection,
}

impl<'a> InstallationRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self) -> DotsResult<Option<Installation>> {
        self.conn
            .query_row(
                "SELECT id, created_at, app_version, data_dir, platform FROM installations ORDER BY created_at LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(storage_err)?
            .map(|(id, created_at, app_version, data_dir, platform)| {
                Ok(Installation { id, created_at: parse_ts(&created_at)?, app_version, data_dir, platform })
            })
            .transpose()
    }

    /// Garante a existencia da instalacao. Nunca sobrescreve o id existente;
    /// apenas atualiza `app_version` e `data_dir` quando mudarem.
    pub fn ensure(&mut self, app_version: &str, data_dir: &str, platform: &str) -> DotsResult<Installation> {
        if let Some(existing) = self.get()? {
            if existing.app_version != app_version || existing.data_dir != data_dir {
                self.conn
                    .execute(
                        "UPDATE installations SET app_version = ?1, data_dir = ?2 WHERE id = ?3",
                        params![app_version, data_dir, existing.id],
                    )
                    .map_err(storage_err)?;
            }
            return Ok(Installation {
                app_version: app_version.to_string(),
                data_dir: data_dir.to_string(),
                ..existing
            });
        }
        let inst = Installation {
            id: new_id(),
            created_at: dots_core::now(),
            app_version: app_version.to_string(),
            data_dir: data_dir.to_string(),
            platform: platform.to_string(),
        };
        self.conn
            .execute(
                "INSERT INTO installations (id, created_at, app_version, data_dir, platform) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![inst.id, ts(&inst.created_at), inst.app_version, inst.data_dir, inst.platform],
            )
            .map_err(storage_err)?;
        Ok(inst)
    }
}
