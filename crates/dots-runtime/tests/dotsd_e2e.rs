//! Sobe o binario real `dotsd`, conversa por IPC e verifica encerramento limpo.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use dots_ipc::{IpcClient, RuntimeInfo, SocketPaths};

fn spawn_dotsd(data_dir: &std::path::Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_dotsd"))
        .args(["--data-dir", data_dir.to_str().unwrap(), "--mode", "detached"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

async fn wait_ready(data_dir: &std::path::Path) -> RuntimeInfo {
    let start = Instant::now();
    loop {
        if let Ok(Some(info)) = RuntimeInfo::read(data_dir) {
            if std::path::Path::new(&info.socket_path).exists() {
                return info;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(15), "dotsd nao ficou pronto");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn full_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = spawn_dotsd(dir.path());
    let info = wait_ready(dir.path()).await;
    assert_eq!(info.pid, child.id());
    assert_eq!(info.mode, "detached");

    let paths = SocketPaths::resolve(dir.path());
    let token = paths.read_token().unwrap();
    let (client, mut events) = IpcClient::connect(&paths.socket_path, &token, "e2e").await.unwrap();

    // status
    let status: dots_core::RuntimeStatus = client.call_typed("runtime.status", serde_json::json!({})).await.unwrap();
    assert_eq!(status.pid, child.id());
    assert_eq!(status.schema_version, dots_storage::latest_version());
    assert_eq!(status.connected_clients, 1);

    // instalacao
    let inst: Option<dots_core::Installation> = client.call_typed("installation.get", serde_json::json!({})).await.unwrap();
    let inst = inst.expect("instalacao criada na primeira execucao");
    assert_eq!(inst.app_version, env!("CARGO_PKG_VERSION"));

    // DOT com workspace padrao
    let dot: dots_core::Dot = client
        .call_typed("dots.create", serde_json::json!({ "name": "QA", "primary_engine": "claude" }))
        .await
        .unwrap();
    let ws = dot.workspace_path.clone().expect("workspace padrao");
    assert!(std::path::Path::new(&ws).is_dir());
    assert!(ws.starts_with(dir.path().join("workspaces").to_str().unwrap()));
    let ev = tokio::time::timeout(Duration::from_secs(2), events.recv()).await.unwrap().unwrap();
    assert_eq!(ev.event, "dots.changed");

    // erro de validacao chega com codigo
    let err = client.call("dots.create", serde_json::json!({ "name": "" })).await.unwrap_err();
    assert_eq!(err.code(), "VALIDATION");
    let err = client.call("dots.get", serde_json::json!({ "id": "nao-existe" })).await.unwrap_err();
    assert_eq!(err.code(), "NOT_FOUND");

    // motores: tres entradas, sem presumir estado da maquina
    let dets: Vec<dots_core::EngineDetection> = client.call_typed("engines.detect", serde_json::json!({})).await.unwrap();
    assert_eq!(dets.len(), 3);
    let cfgs: Vec<dots_core::EngineConfiguration> = client.call_typed("engines.list", serde_json::json!({})).await.unwrap();
    assert!(cfgs.iter().all(|c| c.last_detection.is_some()));

    // instancia duplicada e recusada com codigo 3
    let second = Command::new(env!("CARGO_BIN_EXE_dotsd"))
        .args(["--data-dir", dir.path().to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(second.code(), Some(3));

    // encerramento via IPC
    client.call("runtime.shutdown", serde_json::json!({})).await.unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "dotsd nao encerrou");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(status.code(), Some(0));
    assert!(!paths.socket_path.exists());
    assert!(!dir.path().join("runtime.json").exists());
    assert!(!dir.path().join("runtime.pid").exists());
    assert!(dir.path().join("dots.db").exists());

    // reinicio preserva dados e id da instalacao
    let mut child = spawn_dotsd(dir.path());
    wait_ready(dir.path()).await;
    let token = paths.read_token().unwrap();
    let (client, _) = IpcClient::connect(&paths.socket_path, &token, "e2e").await.unwrap();
    let dots: Vec<dots_core::Dot> = client.call_typed("dots.list", serde_json::json!({})).await.unwrap();
    assert_eq!(dots.len(), 1);
    assert_eq!(dots[0].name, "QA");
    let inst2: Option<dots_core::Installation> = client.call_typed("installation.get", serde_json::json!({})).await.unwrap();
    assert_eq!(inst2.unwrap().id, inst.id);

    // SIGTERM encerra limpo
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let start = Instant::now();
    loop {
        if let Some(s) = child.try_wait().unwrap() {
            assert_eq!(s.code(), Some(0));
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!paths.socket_path.exists());
}
