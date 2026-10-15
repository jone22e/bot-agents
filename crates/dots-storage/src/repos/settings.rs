use dots_core::{DotsResult, Setting};
use rusqlite::{Connection, OptionalExtension, params};

use super::{parse_ts, ts};
use crate::storage_err;

pub struct SettingsRepo<'a> {
    conn: &'a mut Connection,
}

impl<'a> SettingsRepo<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, key: &str) -> DotsResult<Option<Setting>> {
        self.conn
            .query_row(
                "SELECT key, value_json, updated_at FROM application_settings WHERE key = ?1",
                params![key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
            )
            .optional()
            .map_err(storage_err)?
            .map(|(key, value, updated_at)| {
                Ok(Setting { key, value: serde_json::from_str(&value)?, updated_at: parse_ts(&updated_at)? })
            })
            .transpose()
    }

    pub fn list(&self) -> DotsResult<Vec<Setting>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value_json, updated_at FROM application_settings ORDER BY key")
            .map_err(storage_err)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
            .map_err(storage_err)?;
        let mut out = vec![];
        for row in rows {
            let (key, value, updated_at) = row.map_err(storage_err)?;
            out.push(Setting { key, value: serde_json::from_str(&value)?, updated_at: parse_ts(&updated_at)? });
        }
        Ok(out)
    }

    pub fn set(&mut self, key: &str, value: &serde_json::Value) -> DotsResult<Setting> {
        let now = dots_core::now();
        self.conn
            .execute(
                "INSERT INTO application_settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                params![key, serde_json::to_string(value)?, ts(&now)],
            )
            .map_err(storage_err)?;
        Ok(Setting { key: key.to_string(), value: value.clone(), updated_at: now })
    }

    pub fn delete(&mut self, key: &str) -> DotsResult<bool> {
        let n = self
            .conn
            .execute("DELETE FROM application_settings WHERE key = ?1", params![key])
            .map_err(storage_err)?;
        Ok(n > 0)
    }
}
