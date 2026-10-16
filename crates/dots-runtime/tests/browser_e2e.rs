//! Navegador por DOT de ponta a ponta: CLI falso -> gateway MCP real -> Chrome com perfil isolado.
//! Ignorado automaticamente onde nao ha Chrome/Chromium instalado.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use dots_core::*;
use dots_ipc::{IpcClient, RuntimeInfo, SocketPaths};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn has_browser() -> bool {
    let home = dirs::home_dir().unwrap();
    [
        std::path::PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        home.join("Library/Caches/ms-playwright"),
    ]
    .iter()
    .any(|p| p.exists())
}

/// Servidor HTTP minimo: responde a mesma pagina para qualquer caminho.
async fn serve_html(html: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", html.len(), html);
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    format!("http://{addr}/")
}

async fn call(client: &IpcClient, m: &str, p: Value) -> Value {
    client.call(m, p).await.unwrap_or_else(|e| panic!("{m}: {e}"))
}

async fn wait_task(client: &IpcClient, id: &str, timeout: Duration) -> Task {
    let start = Instant::now();
    loop {
        let v = call(client, "tasks.get", json!({ "id": id })).await;
        let t: Task = serde_json::from_value(v["task"].clone()).unwrap();
        if matches!(t.status, TaskStatus::Completed | TaskStatus::Failed) {
            return t;
        }
        assert!(start.elapsed() < timeout, "tarefa {id} ficou em {:?} (erro: {:?})", t.status, t.error);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn each_dot_gets_its_own_browser_profile() {
    if !has_browser() {
        eprintln!("sem navegador na maquina; teste ignorado");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_dotsd"))
        .args(["--data-dir", dir.path().to_str().unwrap(), "--mode", "detached"])
        .env("DOTS_LOG", "info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if let Ok(Some(info)) = RuntimeInfo::read(dir.path()) {
            if std::path::Path::new(&info.socket_path).exists() {
                break;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(15), "dotsd nao ficou pronto");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let paths = SocketPaths::resolve(dir.path());
    let (client, mut events) = IpcClient::connect(&paths.socket_path, &paths.read_token().unwrap(), "e2e").await.unwrap();

    let fake = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake_claude.py");
    let dets: Vec<EngineDetection> = serde_json::from_value(call(&client, "engines.set_override", json!({ "kind": "claude", "path": fake })).await).unwrap();
    assert!(dets[0].installed, "fake deve ser detectado: {:?}", dets[0].issues);

    let url = serve_html("<title>Loja Spike</title><h1>Ola DOT</h1><a href='/entrar'>Entrar</a><input aria-label='Busca'>").await;

    let mk = |name: &str| {
        let name = name.to_string();
        let client = &client;
        async move {
            let dot: Dot = serde_json::from_value(call(client, "dots.create", json!({ "name": name, "primary_engine": "claude", "limits": { "max_execution_seconds": 60, "max_concurrent_tasks": 1, "max_tokens_per_task": null, "max_cost_usd_per_task": null } })).await).unwrap();
            dot
        }
    };

    // DOT A: rede liberada, nivel exec -> abre a pagina sem aprovacao.
    let a = mk("Navegador A").await;
    call(&client, "policies.set", json!({ "dot_id": a.id, "patch": { "auto_approve_level": "exec", "allow_network": true } })).await;
    let task: Task = serde_json::from_value(call(&client, "tasks.create", json!({ "dot_id": a.id, "prompt": format!("BROWSE:{url}") })).await).unwrap();
    let done = wait_task(&client, &task.id, Duration::from_secs(60)).await;
    let text = done.result_text.clone().unwrap_or_default();
    assert_eq!(done.status, TaskStatus::Completed, "{text}");
    assert!(text.starts_with("PAGINA Pagina: Loja Spike"), "{text}");
    assert!(text.contains("heading \"Ola DOT\" [ref="), "{text}");
    assert!(text.contains("link \"Entrar\" [ref="), "{text}");
    assert!(text.contains("textbox \"Busca\" [ref="), "{text}");
    let profile_a = dir.path().join("browsers").join(&a.id);
    assert!(profile_a.is_dir() && profile_a.read_dir().unwrap().next().is_some(), "perfil do DOT A criado em {}", profile_a.display());

    // Assistir: browser.status ve o navegador vivo; browser.watch liga o screencast e os quadros chegam como evento.
    let st = call(&client, "browser.status", json!({ "dot_id": a.id })).await;
    assert_eq!(st["running"], true, "{st}");
    assert!(st["url"].as_str().unwrap().starts_with(&url), "{st}");
    assert_eq!(st["title"], "Loja Spike");
    let w = call(&client, "browser.watch", json!({ "dot_id": a.id })).await;
    assert_eq!(w["started"], true, "{w}");
    assert_eq!(w["watching"], true, "{w}");
    let start = Instant::now();
    let frame = loop {
        let remaining = Duration::from_secs(15).checked_sub(start.elapsed()).expect("quadro nao chegou");
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Some(ev)) if ev.event == "browser.frame" && ev.payload["dot_id"] == a.id => break ev.payload,
            Ok(Some(_)) => continue,
            _ => panic!("quadro nao chegou"),
        }
    };
    assert!(frame["data"].as_str().unwrap().starts_with("/9j/"), "JPEG base64");
    assert!(frame["width"].as_f64().unwrap() > 0.0);
    let w2 = call(&client, "browser.watch", json!({ "dot_id": a.id })).await;
    assert_eq!(w2["started"], false, "renovacao nao inicia outra transmissao");
    call(&client, "browser.unwatch", json!({ "dot_id": a.id })).await;
    let st = call(&client, "browser.status", json!({ "dot_id": a.id })).await;
    assert_eq!(st["watching"], false);

    // Clicar em algo irreversivel ("Excluir ...") aguarda aprovacao mesmo no nivel exec; aprovado, o clique acontece.
    let url2 = serve_html("<title>Acoes</title><button onclick=\"document.title='clicou'\">Excluir anuncio</button><a href='/x'>Mais</a>").await;
    let task: Task = serde_json::from_value(call(&client, "tasks.create", json!({ "dot_id": a.id, "prompt": format!("CLICK:{url2}|button \"Excluir anuncio\"") })).await).unwrap();
    let start = Instant::now();
    let pending = loop {
        let list: Vec<Approval> = serde_json::from_value(call(&client, "approvals.list", json!({ "pending_only": true })).await).unwrap();
        if let Some(ap) = list.into_iter().find(|x| x.task_id == task.id) {
            break ap;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "o clique deveria pedir aprovacao");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(pending.tool, "dots_browser_click");
    assert_eq!(pending.risk, RiskLevel::Critical);
    assert!(pending.summary.starts_with("navegador: clicar em button \"Excluir anuncio\""), "{}", pending.summary);
    call(&client, "approvals.decide", json!({ "id": pending.id, "approved": true })).await;
    let done = wait_task(&client, &task.id, Duration::from_secs(60)).await;
    let text = done.result_text.clone().unwrap_or_default();
    assert!(text.starts_with("CLICOU Pagina: clicou"), "{text}");

    // DOT B: rede geral desabilitada -> pede permissao para o dominio antes de abrir.
    let b = mk("Navegador B").await;
    call(&client, "policies.set", json!({ "dot_id": b.id, "patch": { "auto_approve_level": "exec" } })).await;
    let task: Task = serde_json::from_value(call(&client, "tasks.create", json!({ "dot_id": b.id, "prompt": format!("BROWSE:{url}") })).await).unwrap();
    let start = Instant::now();
    let pending = loop {
        let list: Vec<Approval> = serde_json::from_value(call(&client, "approvals.list", json!({ "pending_only": true })).await).unwrap();
        if let Some(ap) = list.into_iter().find(|x| x.task_id == task.id) { break ap; }
        assert!(start.elapsed() < Duration::from_secs(60), "site externo deveria pedir permissao");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(pending.tool, "dots_browser_open");
    assert_eq!(pending.risk, RiskLevel::Exec);
    assert_eq!(pending.resources, vec!["127.0.0.1"]);
    call(&client, "approvals.decide", json!({ "id": pending.id, "approved": false })).await;
    let done = wait_task(&client, &task.id, Duration::from_secs(60)).await;
    let text = done.result_text.clone().unwrap_or_default();
    assert!(text.starts_with("FALHOU NEGADO: o usuario nao aprovou acesso"), "{text}");
    assert!(!dir.path().join("browsers").join(&b.id).exists(), "DOT B nao deve ter perfil");

    let task: Task = serde_json::from_value(call(&client, "tasks.create", json!({ "dot_id": b.id, "prompt": format!("BROWSE:{url}") })).await).unwrap();
    let start = Instant::now();
    let pending = loop {
        let list: Vec<Approval> = serde_json::from_value(call(&client, "approvals.list", json!({ "pending_only": true })).await).unwrap();
        if let Some(ap) = list.into_iter().find(|x| x.task_id == task.id) { break ap; }
        assert!(start.elapsed() < Duration::from_secs(60), "segunda tentativa deveria pedir permissao");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    call(&client, "approvals.decide", json!({ "id": pending.id, "approved": true })).await;
    let done = wait_task(&client, &task.id, Duration::from_secs(60)).await;
    assert!(done.result_text.unwrap_or_default().starts_with("PAGINA Pagina: Loja Spike"));
    let policy: ToolPolicy = serde_json::from_value(call(&client, "policies.get", json!({ "dot_id": b.id })).await).unwrap();
    assert!(!policy.allow_network, "aprovar dominio nao libera a rede geral");

    let task: Task = serde_json::from_value(call(&client, "tasks.create", json!({ "dot_id": b.id, "prompt": format!("BROWSE:{url}") })).await).unwrap();
    let done = wait_task(&client, &task.id, Duration::from_secs(60)).await;
    assert!(done.result_text.unwrap_or_default().starts_with("PAGINA Pagina: Loja Spike"), "dominio aprendido deve abrir sem novo pedido");
    let approvals: Vec<Approval> = serde_json::from_value(call(&client, "approvals.list", json!({ "pending_only": false })).await).unwrap();
    assert_eq!(approvals.iter().filter(|x| x.dot_id == b.id).count(), 2);

    // Auditoria registrou a chamada do DOT A.
    let audit: Vec<AuditLog> = serde_json::from_value(call(&client, "audit.list", json!({ "limit": 100 })).await).unwrap();
    assert!(audit.iter().any(|x| x.action == "tool.dots_browser_open" && x.details["outcome"] == "ok" && x.actor == format!("dot:{}", a.id)));
    assert!(audit.iter().any(|x| x.action == "tool.dots_browser_open" && x.details["outcome"] == "rejected" && x.actor == format!("dot:{}", b.id)));

    // Excluir os DOTs fecha os navegadores e remove os perfis.
    call(&client, "dots.delete", json!({ "id": a.id })).await;
    let profile_b = dir.path().join("browsers").join(&b.id);
    call(&client, "dots.delete", json!({ "id": b.id })).await;
    assert!(!profile_a.exists(), "perfil removido ao excluir o DOT");
    assert!(!profile_b.exists(), "perfil B removido ao excluir o DOT");

    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let _ = child.wait();
    // Nenhum Chrome orfao com o perfil deste teste (os auxiliares do Chrome saem logo apos o principal).
    let start = Instant::now();
    loop {
        let ps = Command::new("pgrep").args(["-f", dir.path().to_str().unwrap()]).output().unwrap();
        if ps.stdout.is_empty() {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "processos restantes:\n{}", String::from_utf8_lossy(&ps.stdout));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
