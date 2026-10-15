use dots_core::DotsError;
use dots_ipc::RuntimeInfo;
use tauri::{AppHandle, State};

use crate::connection::RuntimeConnection;
use crate::supervisor;

#[tauri::command]
pub async fn ipc_call(
    conn: State<'_, RuntimeConnection>,
    method: String,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, DotsError> {
    conn.call(&method, params.unwrap_or(serde_json::Value::Null)).await
}

#[tauri::command]
pub async fn runtime_info(conn: State<'_, RuntimeConnection>) -> Result<Option<RuntimeInfo>, DotsError> {
    Ok(conn.current_info().await)
}

#[tauri::command]
pub async fn runtime_ensure(app: AppHandle, conn: State<'_, RuntimeConnection>) -> Result<RuntimeInfo, DotsError> {
    conn.ensure(&app).await
}

/// Para o runtime. Em launchd faz bootout (senao o KeepAlive reiniciaria);
/// caso contrario pede encerramento via IPC.
#[tauri::command]
pub async fn runtime_stop(conn: State<'_, RuntimeConnection>) -> Result<(), DotsError> {
    let info = conn.current_info().await;
    let via_launchd = info.as_ref().map(|i| i.mode == "launchd").unwrap_or(false) || supervisor::launchd_status().loaded;
    if via_launchd {
        supervisor::launchd_stop()?;
    }
    // Pede encerramento ordenado (se ainda conectado); ignora erro se ja caiu.
    let _ = conn.call("runtime.shutdown", serde_json::json!({})).await;
    conn.disconnect().await;
    if let Some(i) = info {
        let start = std::time::Instant::now();
        while supervisor::process_alive(i.pid) && start.elapsed() < std::time::Duration::from_secs(10) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn runtime_restart(app: AppHandle, conn: State<'_, RuntimeConnection>) -> Result<RuntimeInfo, DotsError> {
    runtime_stop(conn.clone()).await?;
    conn.ensure(&app).await
}

#[tauri::command]
pub fn runtime_launchd_status() -> supervisor::LaunchdStatus {
    supervisor::launchd_status()
}

#[tauri::command]
pub fn app_paths() -> supervisor::AppPaths {
    supervisor::app_paths()
}

#[tauri::command]
pub fn runtime_set_start_at_login(enabled: bool) -> Result<supervisor::AppPaths, DotsError> {
    supervisor::set_start_at_login(enabled)?;
    Ok(supervisor::app_paths())
}
