//! Lado Rust do app desktop: cliente IPC do runtime `dotsd`, supervisao do
//! processo (launchd ou detached) e comandos finos para a webview.
//! O app nunca abre o SQLite; toda operacao passa pelo runtime.

mod commands;
mod connection;
mod media;
mod supervisor;
mod updater;

use tauri::{Manager, RunEvent, WindowEvent};

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
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            media::transcribe_audio,
            media::image_preview,
            updater::update_state,
            updater::update_check,
            updater::update_download,
            updater::update_install,
            updater::update_set_auto,
        ])
        // Janela de volta ao primeiro plano: procura versao nova (se faz tempo desde a ultima tentativa).
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(true) = event {
                updater::on_focus(window.app_handle());
            }
        })
        .setup(|app| {
            app.manage(updater::Updater::new(app.handle()));
            updater::start(app.handle());
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
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // Versao nova ja baixada e o usuario nao reiniciou: instala ao fechar.
            if let RunEvent::Exit = event {
                updater::install_on_exit(app);
            }
        });
}
