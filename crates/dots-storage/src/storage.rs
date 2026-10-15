use std::path::{Path, PathBuf};

use dots_core::{DotsError, DotsResult};
use rusqlite::{Connection, OpenFlags};

use crate::migrations;
use crate::repos::{AppsRepo, TeamsRepo, ApprovalsRepo, AuditRepo, ChangesRepo, CheckpointsRepo, ConversationsRepo, DelegationsRepo, DotsRepo, EnginesRepo, EventsRepo, ExecutionsRepo, InstallationRepo, MemoriesRepo, PoliciesRepo, SchedulesRepo, SettingsRepo, SubscriptionsRepo, TasksRepo};

#[derive(Debug, Clone)]
pub struct StorageOptions {
    pub db_path: PathBuf,
    /// Diretorio onde backups pre-migracao sao gravados. `None` desabilita.
    pub backup_dir: Option<PathBuf>,
    /// Quantidade de backups a manter (os mais antigos sao removidos).
    pub backup_keep: usize,
}

impl StorageOptions {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self { db_path: db_path.into(), backup_dir: None, backup_keep: 10 }
    }
    pub fn with_backup_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.backup_dir = Some(dir.into());
        self
    }
}

/// Conexao SQLite com migracoes aplicadas. Nao e `Sync`: use via [`crate::StorageHandle`].
pub struct Storage {
    conn: Connection,
    options: StorageOptions,
}

impl Storage {
    /// Abre (ou cria) o banco, configura pragmas e aplica migracoes pendentes.
    /// Antes de migrar um banco ja existente, grava backup em `backup_dir`.
    pub fn open(options: StorageOptions) -> DotsResult<Self> {
        Self::open_with(options, migrations::MIGRATIONS)
    }

    /// Variante com lista explicita de migracoes. Usada em testes de upgrade.
    #[doc(hidden)]
    pub fn open_with(options: StorageOptions, migrations_list: &[migrations::Migration]) -> DotsResult<Self> {
        if let Some(parent) = options.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let existed = options.db_path.exists();
        let conn = Connection::open_with_flags(
            &options.db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(crate::storage_err)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA temp_store = MEMORY;",
        )
        .map_err(crate::storage_err)?;

        let mut storage = Self { conn, options };
        let pending = migrations::pending_from(&storage.conn, migrations_list)?;
        if !pending.is_empty() {
            if existed {
                storage.backup_before_migration()?;
            }
            for m in &pending {
                migrations::apply(&mut storage.conn, m)?;
            }
        }
        Ok(storage)
    }

    /// Abre um banco em memoria com migracoes (testes).
    pub fn open_in_memory() -> DotsResult<Self> {
        let conn = Connection::open_in_memory().map_err(crate::storage_err)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;").map_err(crate::storage_err)?;
        let mut storage = Self { conn, options: StorageOptions::new(":memory:") };
        for m in migrations::pending(&storage.conn)? {
            migrations::apply(&mut storage.conn, &m)?;
        }
        Ok(storage)
    }

    pub fn schema_version(&self) -> DotsResult<u32> {
        migrations::current_version(&self.conn)
    }

    pub fn db_path(&self) -> &Path {
        &self.options.db_path
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn dots(&mut self) -> DotsRepo<'_> {
        DotsRepo::new(&mut self.conn)
    }
    pub fn engines(&mut self) -> EnginesRepo<'_> {
        EnginesRepo::new(&mut self.conn)
    }
    pub fn settings(&mut self) -> SettingsRepo<'_> {
        SettingsRepo::new(&mut self.conn)
    }
    pub fn audit(&mut self) -> AuditRepo<'_> {
        AuditRepo::new(&mut self.conn)
    }
    pub fn installation(&mut self) -> InstallationRepo<'_> {
        InstallationRepo::new(&mut self.conn)
    }
    pub fn tasks(&mut self) -> TasksRepo<'_> {
        TasksRepo::new(&mut self.conn)
    }
    pub fn executions(&mut self) -> ExecutionsRepo<'_> {
        ExecutionsRepo::new(&mut self.conn)
    }
    pub fn conversations(&mut self) -> ConversationsRepo<'_> {
        ConversationsRepo::new(&mut self.conn)
    }
    pub fn memories(&mut self) -> MemoriesRepo<'_> {
        MemoriesRepo::new(&mut self.conn)
    }
    pub fn approvals(&mut self) -> ApprovalsRepo<'_> {
        ApprovalsRepo::new(&mut self.conn)
    }
    pub fn policies(&mut self) -> PoliciesRepo<'_> {
        PoliciesRepo::new(&mut self.conn)
    }
    pub fn changes(&mut self) -> ChangesRepo<'_> {
        ChangesRepo::new(&mut self.conn)
    }
    pub fn events(&mut self) -> EventsRepo<'_> {
        EventsRepo::new(&mut self.conn)
    }
    pub fn subscriptions(&mut self) -> SubscriptionsRepo<'_> {
        SubscriptionsRepo::new(&mut self.conn)
    }
    pub fn schedules(&mut self) -> SchedulesRepo<'_> {
        SchedulesRepo::new(&mut self.conn)
    }
    pub fn checkpoints(&mut self) -> CheckpointsRepo<'_> {
        CheckpointsRepo::new(&mut self.conn)
    }
    pub fn delegations(&mut self) -> DelegationsRepo<'_> {
        DelegationsRepo::new(&mut self.conn)
    }
    pub fn apps(&mut self) -> AppsRepo<'_> {
        AppsRepo::new(&mut self.conn)
    }
    pub fn teams(&mut self) -> TeamsRepo<'_> {
        TeamsRepo::new(&mut self.conn)
    }

    /// Copia consistente do banco (API de backup online do SQLite) para `dest`.
    pub fn backup_to(&self, dest: &Path) -> DotsResult<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut dst = Connection::open(dest).map_err(crate::storage_err)?;
        let backup = rusqlite::backup::Backup::new(&self.conn, &mut dst).map_err(crate::storage_err)?;
        backup
            .run_to_completion(256, std::time::Duration::from_millis(5), None)
            .map_err(crate::storage_err)?;
        Ok(())
    }

    /// Backup manual nomeado com timestamp. Retorna o caminho gerado.
    pub fn backup_now(&self, label: &str) -> DotsResult<PathBuf> {
        let dir = self
            .options
            .backup_dir
            .clone()
            .ok_or_else(|| DotsError::Storage("diretorio de backup nao configurado".into()))?;
        let ts = dots_core::now().format("%Y%m%dT%H%M%SZ");
        let file = dir.join(format!("dots-{label}-{ts}.db"));
        self.backup_to(&file)?;
        self.prune_backups(&dir)?;
        Ok(file)
    }

    pub fn list_backups(&self) -> DotsResult<Vec<PathBuf>> {
        let Some(dir) = &self.options.backup_dir else { return Ok(vec![]) };
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "db").unwrap_or(false))
            .collect();
        files.sort();
        Ok(files)
    }

    fn backup_before_migration(&self) -> DotsResult<()> {
        if self.options.backup_dir.is_none() {
            return Ok(());
        }
        let from = migrations::current_version(&self.conn)?;
        let path = self.backup_now(&format!("pre-migrate-v{from}"))?;
        tracing::info!(path = %path.display(), "backup pre-migracao gravado");
        Ok(())
    }

    fn prune_backups(&self, dir: &Path) -> DotsResult<()> {
        let mut files = self.list_backups()?;
        let _ = dir;
        while files.len() > self.options.backup_keep {
            let oldest = files.remove(0);
            let _ = std::fs::remove_file(&oldest);
        }
        Ok(())
    }

    /// Checkpoint do WAL (chamado no encerramento ordenado).
    pub fn checkpoint(&self) -> DotsResult<()> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(crate::storage_err)
    }
}
