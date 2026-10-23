//! `dotsd`: runtime local do DOTS. Processo independente da janela, unico dono
//! do SQLite, servidor IPC e supervisor de servicos.

mod agy_setup;
mod autonomy;
mod browser;
mod browser_ipc;
mod cli;
mod cli_updates;
mod delegation;
mod dismissal;
mod events;
mod executor;
mod flexi;
mod flexi_sync;
mod flexi_team;
mod group;
mod hiring;
mod staffing;
mod teams;
mod gateway;
mod goals;
mod lock;
mod mcp_gateway;
mod paths;
mod performance;
mod policy;
mod registry;
mod scheduler;
mod schedules;
mod service;
mod shutdown;
mod stats;
mod tools;
mod usage;
mod watcher;

use std::sync::Arc;

use dots_core::RuntimeMode;
use dots_ipc::{IpcServer, RuntimeInfo, ServerConfig, SocketPaths};
use dots_storage::{StorageHandle, StorageOptions};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Codigos de saida documentados (usados pelo desktop e pelo launchd).
pub mod exit_codes {
    pub const OK: i32 = 0;
    pub const CONFIG: i32 = 2;
    pub const ALREADY_RUNNING: i32 = 3;
    pub const STORAGE: i32 = 4;
    pub const IPC: i32 = 5;
}

fn main() {
    let args = match cli::parse(std::env::args().skip(1)) {
        Ok(cli::Command::Run(a)) => a,
        Ok(cli::Command::McpGateway { data_dir }) => {
            let data_dir = match data_dir {
                Some(d) => d,
                None => match paths::default_data_dir() {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("erro: {e}");
                        std::process::exit(exit_codes::CONFIG);
                    }
                },
            };
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio");
            let code = rt.block_on(mcp_gateway::run(data_dir));
            std::process::exit(code);
        }
        Ok(cli::Command::Call { data_dir, method, params }) => {
            let data_dir = match data_dir.map(Ok).unwrap_or_else(paths::default_data_dir) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("erro: {e}");
                    std::process::exit(exit_codes::CONFIG);
                }
            };
            let params: serde_json::Value = match serde_json::from_str(&params) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("erro: params nao e JSON valido: {e}");
                    std::process::exit(exit_codes::CONFIG);
                }
            };
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio");
            let code = rt.block_on(async {
                let sp = SocketPaths::resolve(&data_dir);
                let token = match sp.read_token() {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("erro: {e}");
                        return exit_codes::IPC;
                    }
                };
                let (client, _events) = match dots_ipc::IpcClient::connect(&sp.socket_path, &token, "dotsd-cli").await {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("erro: {e}");
                        return exit_codes::IPC;
                    }
                };
                match client.call(&method, params).await {
                    Ok(v) => {
                        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                        exit_codes::OK
                    }
                    Err(e) => {
                        eprintln!("erro: {e}");
                        exit_codes::IPC
                    }
                }
            });
            std::process::exit(code);
        }
        Ok(cli::Command::Version) => {
            println!("dotsd {VERSION}");
            return;
        }
        Ok(cli::Command::Help) => {
            println!("{}", cli::HELP);
            return;
        }
        Err(e) => {
            eprintln!("erro: {e}\n\n{}", cli::HELP);
            std::process::exit(exit_codes::CONFIG);
        }
    };
    let code = run(args);
    std::process::exit(code);
}

fn run(args: cli::RunArgs) -> i32 {
    let layout = match paths::DataLayout::prepare(args.data_dir) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("erro ao preparar diretorio de dados: {e}");
            return exit_codes::CONFIG;
        }
    };
    let _log_guard = init_logging(&layout, args.log_to_stderr);
    tracing::info!(version = VERSION, data_dir = %layout.root.display(), mode = ?args.mode, "dotsd iniciando");

    // Instancia unica por flock. Mantido vivo ate o fim do processo.
    let _lock = match lock::InstanceLock::acquire(&layout.lock_file) {
        Ok(l) => l,
        Err(lock::LockError::AlreadyRunning(pid)) => {
            tracing::error!(pid, "outra instancia do dotsd ja esta em execucao");
            eprintln!("dotsd ja em execucao (pid {pid})");
            return exit_codes::ALREADY_RUNNING;
        }
        Err(lock::LockError::Io(e)) => {
            tracing::error!(error = %e, "falha ao obter lock de instancia");
            return exit_codes::CONFIG;
        }
    };
    if let Err(e) = std::fs::write(&layout.pid_file, std::process::id().to_string()) {
        tracing::warn!(error = %e, "nao foi possivel gravar pid file");
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "falha ao criar runtime tokio");
            return exit_codes::CONFIG;
        }
    };
    let code = runtime.block_on(async_main(layout.clone(), args.mode));
    let _ = std::fs::remove_file(&layout.pid_file);
    RuntimeInfo::remove(&layout.root);
    tracing::info!(code, "dotsd encerrado");
    code
}

async fn async_main(layout: paths::DataLayout, mode: RuntimeMode) -> i32 {
    // Storage
    let storage_opts = StorageOptions::new(&layout.db_file).with_backup_dir(&layout.backups_dir);
    let actor = match StorageHandle::spawn(storage_opts) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(error = %e, "falha ao abrir o banco");
            return exit_codes::STORAGE;
        }
    };
    let storage = actor.handle.clone();
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let data_dir_str = layout.root.to_string_lossy().to_string();
    let installation = match storage
        .call(move |s| s.installation().ensure(VERSION, &data_dir_str, &platform))
        .await
    {
        Ok(i) => i,
        Err(e) => {
            tracing::error!(error = %e, "falha ao registrar instalacao");
            return exit_codes::STORAGE;
        }
    };
    tracing::info!(installation = %installation.id, "instalacao pronta");

    // IPC
    let socket_paths = SocketPaths::resolve(&layout.root);
    let server = match IpcServer::bind(socket_paths.clone(), ServerConfig { version: VERSION.into(), event_buffer: 256 }) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "falha ao abrir socket IPC");
            return exit_codes::IPC;
        }
    };
    let started_at = dots_core::now();
    let info = RuntimeInfo {
        pid: std::process::id(),
        version: VERSION.into(),
        proto: dots_core::PROTOCOL_VERSION,
        socket_path: socket_paths.socket_path.to_string_lossy().to_string(),
        token_path: socket_paths.token_path.to_string_lossy().to_string(),
        started_at: started_at.to_rfc3339(),
        mode: serde_json::to_value(mode).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
    };
    if let Err(e) = info.write(&layout.root) {
        tracing::error!(error = %e, "falha ao gravar runtime.json");
        return exit_codes::IPC;
    }

    let shutdown = shutdown::Shutdown::new();
    let home = paths::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
    let path_dirs = dots_engines::candidate_dirs(&home);
    let max_concurrent = storage
        .call(|s| s.settings().get("runtime.max_concurrent_executions"))
        .await
        .ok()
        .flatten()
        .and_then(|s| s.value.as_u64())
        .unwrap_or(4) as usize;
    let dotsd_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("dotsd"));
    let (watch_tx, watch_rx) = tokio::sync::mpsc::channel::<Vec<std::path::PathBuf>>(64);
    let watcher = Arc::new(std::sync::Mutex::new(watcher::Watcher::new(watch_tx)));
    let service = Arc::new(service::RuntimeService {
        storage: storage.clone(),
        events: server.event_sink(),
        clients: server.connected_clients(),
        layout: layout.clone(),
        socket_path: socket_paths.socket_path.clone(),
        started_at,
        mode,
        shutdown: shutdown.clone(),
        schema_version: storage.call(|s| s.schema_version()).await.unwrap_or(0),
        registry: registry::ExecutionRegistry::default(),
        engines: dots_engines::EngineManager::new(max_concurrent, max_concurrent),
        scheduler_notify: tokio::sync::Notify::new(),
        paused: std::sync::atomic::AtomicBool::new(false),
        home,
        path_dirs,
        dotsd_path,
        schedules_notify: tokio::sync::Notify::new(),
        autonomy_lock: tokio::sync::Mutex::new(()),
        delegation_finish_lock: tokio::sync::Mutex::new(()),
        watcher: watcher.clone(),
        event_depths: std::sync::Mutex::new(std::collections::HashMap::new()),
        usage: crate::usage::UsageCache::default(),
        cli_updates: crate::cli_updates::UpdatesCache::default(),
        flexi_link: crate::flexi_sync::LinkState::default(),
        browsers: crate::browser::BrowserPool::new(paths::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")), layout.browsers_dir.clone()),
        flexi: crate::flexi::FlexiRegistry::new(if std::env::var("DOTS_SECRETS").as_deref() == Ok("memory") {
            Arc::new(dots_flexi::MemoryStore::default())
        } else {
            // Chaveiro do sistema; onde ele nao funciona (ex.: Docker bloqueia keyctl), cofre em arquivo 0600 na pasta de dados.
            Arc::new(dots_flexi::FallbackStore::new(dots_flexi::KeychainStore::new("com.c2s.dots"), dots_flexi::FileStore::new(layout.root.join("secrets.json"))))
        }),
    });

    // Recuperacao: execucoes/tarefas interrompidas por um runtime anterior.
    match storage
        .call(|s| {
            let running = s.executions().running()?;
            let n = s.executions().mark_interrupted()?;
            let recovered = s.tasks().recover_interrupted()?;
            Ok((n, recovered, running))
        })
        .await
    {
        Ok((n, recovered, running)) => {
            // Processos de motor orfaos de um runtime que morreu sem encerramento ordenado.
            for e in running {
                if let Some(pid) = e.pid {
                    if unsafe { libc::kill(pid as i32, 0) } == 0 {
                        tracing::warn!(pid, execution = %e.id, "encerrando processo de motor orfao");
                        unsafe { libc::killpg(pid as i32, libc::SIGTERM) };
                    }
                }
            }
            if n > 0 || !recovered.is_empty() {
                tracing::warn!(executions = n, tasks = recovered.len(), "recuperacao apos reinicio");
                for (t, st) in recovered {
                    tracing::info!(task = %t.id, status = st.as_str(), retry_safe = t.retry_safe, "tarefa recuperada");
                }
            }
        }
        Err(e) => tracing::error!(error = %e, "falha na recuperacao de tarefas"),
    }
    tokio::spawn(scheduler::run(service.clone()));
    service.scheduler_notify.notify_one();
    tokio::spawn(schedules::run(service.clone()));
    tokio::spawn(autonomy::run(service.clone()));
    tokio::spawn(watcher::run(service.clone(), watch_rx, watcher.clone()));
    tokio::spawn(flexi_sync::run(service.clone()));
    service.refresh_watcher().await;

    // Deteccao inicial dos motores em segundo plano (nao bloqueia o IPC).
    {
        let svc = service.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.detect_engines(None).await {
                tracing::warn!(error = %e, "deteccao inicial de motores falhou");
            }
        });
    }

    let handler = {
        let svc = service.clone();
        move |method: String, params: serde_json::Value| -> dots_ipc::HandlerFuture {
            let svc = svc.clone();
            Box::pin(async move { svc.dispatch(&method, params).await })
        }
    };
    tracing::info!(socket = %socket_paths.socket_path.display(), "IPC pronto");

    let signal_shutdown = shutdown.clone();
    tokio::spawn(async move {
        shutdown::wait_for_signal().await;
        signal_shutdown.trigger("sinal do sistema");
    });

    server.serve(Arc::new(handler), shutdown.wait()).await;
    service.browsers.shutdown_all().await;
    tracing::info!("servidor IPC encerrado; finalizando storage");
    actor.shutdown();
    exit_codes::OK
}

fn init_logging(layout: &paths::DataLayout, stderr: bool) -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::prelude::*;
    let file = tracing_appender::rolling::daily(&layout.logs_dir, "dotsd.log");
    let (nb, guard) = tracing_appender::non_blocking(file);
    let filter = tracing_subscriber::EnvFilter::try_from_env("DOTS_LOG").unwrap_or_else(|_| "info".into());
    let file_layer = tracing_subscriber::fmt::layer().with_ansi(false).with_writer(nb);
    let registry = tracing_subscriber::registry().with(filter).with(file_layer);
    if stderr {
        registry.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr)).init();
    } else {
        registry.init();
    }
    guard
}
