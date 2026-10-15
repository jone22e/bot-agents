//! Lado Rust do app desktop: cliente IPC do runtime `dotsd`, supervisao do
//! processo (launchd ou detached) e comandos finos para a webview.
//! O app nunca abre o SQLite; toda operacao passa pelo runtime.

mod commands;
mod connection;
mod supervisor;

use tauri::Manager;

pub use connection::RuntimeConnection;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("DOTS_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        // Deve ser o primeiro plugin registrado.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(RuntimeConnection::new())
        .invoke_handler(tauri::generate_handler![
            commands::ipc_call,
            commands::runtime_info,
            commands::runtime_ensure,
            commands::runtime_stop,
            commands::runtime_restart,
            commands::runtime_launchd_status,
            commands::app_paths,
            commands::runtime_set_start_at_login,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let conn = handle.state::<RuntimeConnection>();
                match conn.ensure(&handle).await {
                    Ok(info) => tracing::info!(pid = info.pid, mode = %info.mode, "runtime conectado"),
                    Err(e) => tracing::error!(error = %e, "falha ao iniciar/conectar runtime"),
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
