use dots_core::{DotsError, DotsResult};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

/// Migracao embutida no binario. Arquivos vivem em `migrations/` na raiz do repo.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration { version: 1, name: "init", sql: include_str!("../../../migrations/0001_init.sql") },
    Migration { version: 2, name: "execution", sql: include_str!("../../../migrations/0002_execution.sql") },
    Migration { version: 3, name: "automation", sql: include_str!("../../../migrations/0003_automation.sql") },
    Migration { version: 4, name: "model", sql: include_str!("../../../migrations/0004_model.sql") },
    Migration { version: 5, name: "apps", sql: include_str!("../../../migrations/0005_apps.sql") },
    Migration { version: 6, name: "role_template", sql: include_str!("../../../migrations/0006_role_template.sql") },
    Migration { version: 7, name: "work_group", sql: include_str!("../../../migrations/0007_work_group.sql") },
    Migration { version: 8, name: "teams", sql: include_str!("../../../migrations/0008_teams.sql") },
];

pub fn latest_version() -> u32 {
    MIGRATIONS.iter().map(|m| m.version).max().unwrap_or(0)
}

fn checksum(sql: &str) -> String {
    hex::encode(Sha256::digest(sql.as_bytes()))
}

pub(crate) fn ensure_table(conn: &Connection) -> DotsResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            name       TEXT NOT NULL,
            checksum   TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )
    .map_err(crate::storage_err)
}

pub(crate) fn current_version(conn: &Connection) -> DotsResult<u32> {
    ensure_table(conn)?;
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get::<_, u32>(0),
    )
    .map_err(crate::storage_err)
}

/// Lista as migracoes ainda nao aplicadas, validando checksums das aplicadas.
pub(crate) fn pending(conn: &Connection) -> DotsResult<Vec<Migration>> {
    pending_from(conn, MIGRATIONS)
}

pub(crate) fn pending_from(conn: &Connection, list: &[Migration]) -> DotsResult<Vec<Migration>> {
    ensure_table(conn)?;
    let mut stmt = conn
        .prepare("SELECT version, checksum FROM schema_migrations ORDER BY version")
        .map_err(crate::storage_err)?;
    let applied: Vec<(u32, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(crate::storage_err)?
        .collect::<Result<_, _>>()
        .map_err(crate::storage_err)?;

    for (version, stored) in &applied {
        match list.iter().find(|m| m.version == *version) {
            Some(m) if checksum(m.sql) == *stored => {}
            Some(m) => {
                return Err(DotsError::Storage(format!(
                    "migracao {} ({}) ja aplicada foi alterada no binario; checksum divergente",
                    m.version, m.name
                )));
            }
            None => {
                return Err(DotsError::Storage(format!(
                    "banco contem migracao {version} desconhecida por esta versao do DOTS (downgrade nao suportado)"
                )));
            }
        }
    }
    let max_applied = applied.iter().map(|a| a.0).max().unwrap_or(0);
    Ok(list.iter().copied().filter(|m| m.version > max_applied).collect())
}

/// Aplica uma migracao dentro de transacao propria.
pub(crate) fn apply(conn: &mut Connection, m: &Migration) -> DotsResult<()> {
    let tx = conn.transaction().map_err(crate::storage_err)?;
    tx.execute_batch(m.sql).map_err(|e| {
        DotsError::Storage(format!("falha na migracao {} ({}): {e}", m.version, m.name))
    })?;
    tx.execute(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
        params![m.version, m.name, checksum(m.sql), dots_core::now().to_rfc3339()],
    )
    .map_err(crate::storage_err)?;
    tx.commit().map_err(crate::storage_err)?;
    tracing::info!(version = m.version, name = m.name, "migracao aplicada");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_sequential_and_unique() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(m.version as usize, i + 1, "migracoes devem ser 1..n sem lacunas");
        }
    }
}
