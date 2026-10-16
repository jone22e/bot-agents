use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dots_core::{DotsError, DotsResult};
use dots_ipc::{IpcClient, RuntimeInfo};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::supervisor;

/// Conexao persistente com o runtime, compartilhada pelos comandos.
pub struct RuntimeConnection {
    inner: Arc<Mutex<Option<Connected>>>,
    starting: Arc<Mutex<()>>,
}

struct Connected {
    client: IpcClient,
    info: RuntimeInfo,
}

impl RuntimeConnection {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(None)), starting: Arc::new(Mutex::new(())) }
    }

    pub async fn current_info(&self) -> Option<RuntimeInfo> {
        let guard = self.inner.lock().await;
        guard.as_ref().filter(|c| c.client.is_connected()).map(|c| c.info.clone())
    }

    /// Tenta conectar ao runtime existente; se nao houver, inicia-o.
    pub async fn ensure(&self, app: &AppHandle) -> DotsResult<RuntimeInfo> {
        let _starting = self.starting.lock().await;
        if let Some(info) = self.current_info().await {
            return Ok(info);
        }
        let data = supervisor::data_dir();
        std::fs::create_dir_all(&data)?;

        if let Ok(info) = self.try_connect(app, &data).await {
            if !supervisor::runtime_is_stale(&info.version, &data) {
                return Ok(info);
            }
            // Runtime antigo ainda em execucao (ex.: launchd o reiniciou antes da atualizacao): troca pelo novo.
            tracing::info!(running = %info.version, "runtime desatualizado em execucao; atualizando");
            if info.mode == "launchd" || supervisor::launchd_status().loaded {
                let _ = supervisor::launchd_stop();
            }
            let _ = self.call("runtime.shutdown", serde_json::json!({})).await;
            self.disconnect().await;
            let start = std::time::Instant::now();
            while supervisor::process_alive(info.pid) && start.elapsed() < Duration::from_secs(10) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        let (binary, binary_changed) = supervisor::runtime_binary(&data)?;
        let mut used_launchd = false;
        if !supervisor::dev_mode() && cfg!(target_os = "macos") {
            match supervisor::launchd_install_and_start(&binary, &data, binary_changed) {
                Ok(()) => used_launchd = true,
                Err(e) => tracing::warn!(error = %e, "launchd indisponivel; usando spawn detached"),
            }
        }
        if !used_launchd {
            supervisor::spawn_detached(&binary, &data)?;
        }
        supervisor::wait_runtime_info(&data, Duration::from_secs(15)).await?;
        self.try_connect(app, &data).await
    }

    async fn try_connect(&self, app: &AppHandle, data: &Path) -> DotsResult<RuntimeInfo> {
        let info = RuntimeInfo::read(data)?.ok_or_else(|| DotsError::RuntimeUnavailable("runtime nao iniciado".into()))?;
        if !supervisor::process_alive(info.pid) {
            return Err(DotsError::RuntimeUnavailable("runtime.json obsoleto (processo morto)".into()));
        }
        let token = std::fs::read_to_string(&info.token_path)
            .map_err(|e| DotsError::RuntimeUnavailable(format!("token: {e}")))?
            .trim()
            .to_string();
        let socket = Path::new(&info.socket_path);
        let (client, mut events) = IpcClient::connect(socket, &token, "desktop").await?;
        let app2 = app.clone();
        let inner = self.inner.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(ev) = events.recv().await {
                let _ = app2.emit("runtime:event", &ev);
            }
            tracing::warn!("conexao com o runtime encerrada");
            let mut guard = inner.lock().await;
            *guard = None;
            let _ = app2.emit("runtime:disconnected", ());
        });
        let mut guard = self.inner.lock().await;
        *guard = Some(Connected { client, info: info.clone() });
        let _ = app.emit("runtime:connected", &info);
        Ok(info)
    }

    pub async fn call(&self, method: &str, params: serde_json::Value) -> DotsResult<serde_json::Value> {
        let client = {
            let guard = self.inner.lock().await;
            guard.as_ref().filter(|c| c.client.is_connected()).map(|c| c.client.clone())
        };
        match client {
            Some(c) => c.call(method, params).await,
            None => Err(DotsError::RuntimeUnavailable("sem conexao com o runtime".into())),
        }
    }

    pub async fn disconnect(&self) {
        let mut guard = self.inner.lock().await;
        *guard = None;
    }
}
