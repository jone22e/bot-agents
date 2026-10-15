//! Fase 2 de ponta a ponta com um CLI falso que fala MCP com o gateway real do dotsd.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use dots_core::*;
use dots_ipc::{IpcClient, RuntimeInfo, SocketPaths};
use serde_json::{Value, json};

struct Rt {
    dir: tempfile::TempDir,
    child: std::process::Child,
    client: IpcClient,
    events: dots_ipc::EventReceiver,
}

fn spawn_dotsd(data_dir: &std::path::Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_dotsd"))
        .args(["--data-dir", data_dir.to_str().unwrap(), "--mode", "detached"])
        .env("DOTS_LOG", "info")
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

async fn start() -> Rt {
    let dir = tempfile::tempdir().unwrap();
    let child = spawn_dotsd(dir.path());
    wait_ready(dir.path()).await;
    let paths = SocketPaths::resolve(dir.path());
    let token = paths.read_token().unwrap();
    let (client, events) = IpcClient::connect(&paths.socket_path, &token, "e2e").await.unwrap();
    Rt { dir, child, client, events }
}

impl Rt {
    async fn call(&self, m: &str, p: Value) -> Value {
        self.client.call(m, p).await.unwrap_or_else(|e| panic!("{m}: {e}"))
    }
    async fn create_dot(&self, name: &str, level: &str) -> Dot {
        let dot: Dot = serde_json::from_value(self.call("dots.create", json!({ "name": name, "primary_engine": "claude", "limits": { "max_execution_seconds": 60, "max_concurrent_tasks": 1, "max_tokens_per_task": null, "max_cost_usd_per_task": null } })).await).unwrap();
        self.call("policies.set", json!({ "dot_id": dot.id, "patch": { "auto_approve_level": level } })).await;
        dot
    }
    async fn use_fake_engine(&self) {
        let fake = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake_claude.py");
        let dets: Vec<EngineDetection> = serde_json::from_value(self.call("engines.set_override", json!({ "kind": "claude", "path": fake })).await).unwrap();
        assert!(dets[0].installed, "fake deve ser detectado: {:?}", dets[0].issues);
        assert_eq!(dets[0].auth_status, AuthStatus::Authenticated);
    }
    async fn wait_task(&mut self, id: &str, want: &[TaskStatus], timeout: Duration) -> Task {
        let start = Instant::now();
        loop {
            let v = self.call("tasks.get", json!({ "id": id })).await;
            let t: Task = serde_json::from_value(v["task"].clone()).unwrap();
            if want.contains(&t.status) {
                return t;
            }
            assert!(start.elapsed() < timeout, "tarefa {id} ficou em {:?} (erro: {:?})", t.status, t.error);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    async fn wait_event(&mut self, name: &str, timeout: Duration) -> Value {
        let start = Instant::now();
        loop {
            let remaining = timeout.checked_sub(start.elapsed()).expect("evento nao chegou");
            match tokio::time::timeout(remaining, self.events.recv()).await {
                Ok(Some(ev)) if ev.event == name => return ev.payload,
                Ok(Some(_)) => continue,
                _ => panic!("evento {name} nao chegou"),
            }
        }
    }
    fn stop(mut self) {
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn write_file_through_gateway_and_block_outside_workspace() {
    let mut rt = start().await;
    rt.use_fake_engine().await;
    let dot = rt.create_dot("Escritor", "write").await;
    let ws = dot.workspace_path.clone().unwrap();

    // Escrita autorizada (auto-aprovada em nivel write)
    let task: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": dot.id, "prompt": "WRITE:hello.txt:ola mundo", "title": "Criar hello" })).await).unwrap();
    assert_eq!(task.status, TaskStatus::Queued);
    let done = rt.wait_task(&task.id, &[TaskStatus::Completed, TaskStatus::Failed, TaskStatus::WaitingUser], Duration::from_secs(30)).await;
    assert_eq!(done.status, TaskStatus::Completed, "erro: {:?}", done.error);
    assert_eq!(done.result_text.as_deref(), Some("FEITO"));
    assert_eq!(std::fs::read_to_string(format!("{ws}/hello.txt")).unwrap(), "ola mundo");
    assert!(!std::path::Path::new(&format!("{ws}/.dots/execution.token")).exists(), "token removido ao fim");

    let v = rt.call("tasks.get", json!({ "id": task.id })).await;
    let execs: Vec<Execution> = serde_json::from_value(v["executions"].clone()).unwrap();
    assert_eq!(execs.len(), 1);
    assert_eq!(execs[0].status, ExecutionStatus::Completed);
    assert!(execs[0].events_count >= 4);
    assert_eq!(execs[0].engine_session_id.as_deref().map(|s| s.len()), Some(36), "session-id uuid fixado pelo runtime");
    let evs: Vec<ExecutionEventRecord> = serde_json::from_value(rt.call("executions.events", json!({ "execution_id": execs[0].id })).await).unwrap();
    assert!(evs.iter().any(|e| matches!(e.event, ExecutionEvent::ToolCall { ref name, .. } if name == "mcp__dots__dots_write_file")));
    assert!(matches!(evs.last().unwrap().event, ExecutionEvent::Completed { .. }));
    let changes: Vec<WorkspaceChange> = serde_json::from_value(rt.call("executions.changes", json!({ "id": execs[0].id })).await).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, "create");
    let audit: Vec<AuditLog> = serde_json::from_value(rt.call("audit.list", json!({ "limit": 50 })).await).unwrap();
    assert!(audit.iter().any(|a| a.action == "tool.dots_write_file" && a.actor == format!("dot:{}", dot.id)));
    let mems: Vec<Memory> = serde_json::from_value(rt.call("memories.list", json!({ "dot_id": dot.id, "kind": "history" })).await).unwrap();
    assert_eq!(mems.len(), 1, "historico gravado");
    assert!(mems[0].content.contains("Criar hello"));

    // Fora do workspace: negado pelo runtime, auditado, tarefa conclui com NEGADO
    let task: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": dot.id, "prompt": "DENY:/etc/dots-should-not-exist" })).await).unwrap();
    let done = rt.wait_task(&task.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await;
    assert_eq!(done.result_text.as_deref(), Some("NEGADO"));
    assert!(!std::path::Path::new("/etc/dots-should-not-exist").exists());
    let audit: Vec<AuditLog> = serde_json::from_value(rt.call("audit.list", json!({ "limit": 50 })).await).unwrap();
    assert!(audit.iter().any(|a| a.action == "tool.dots_write_file" && a.details["outcome"] == "denied"));
    rt.stop();
}

#[tokio::test]
async fn approval_flow_approve_and_reject() {
    let mut rt = start().await;
    rt.use_fake_engine().await;
    let dot = rt.create_dot("Cauteloso", "read").await;
    let ws = dot.workspace_path.clone().unwrap();

    let task: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": dot.id, "prompt": "WRITE:a.txt:1" })).await).unwrap();
    let approval = rt.wait_event("approval.requested", Duration::from_secs(20)).await;
    let approval: Approval = serde_json::from_value(approval).unwrap();
    assert_eq!(approval.tool, "dots_write_file");
    assert_eq!(approval.risk, RiskLevel::Write);
    assert!(approval.resources[0].ends_with("a.txt"));
    let t = rt.wait_task(&task.id, &[TaskStatus::WaitingApproval], Duration::from_secs(5)).await;
    assert_eq!(t.status, TaskStatus::WaitingApproval);
    let pend: Vec<Approval> = serde_json::from_value(rt.call("approvals.list", json!({ "pending_only": true })).await).unwrap();
    assert_eq!(pend.len(), 1);
    rt.call("approvals.decide", json!({ "id": approval.id, "approved": true })).await;
    let done = rt.wait_task(&task.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(20)).await;
    assert_eq!(done.result_text.as_deref(), Some("FEITO"));
    assert!(std::path::Path::new(&format!("{ws}/a.txt")).exists());

    let task: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": dot.id, "prompt": "WRITE:b.txt:2" })).await).unwrap();
    let approval: Approval = serde_json::from_value(rt.wait_event("approval.requested", Duration::from_secs(20)).await).unwrap();
    rt.call("approvals.decide", json!({ "id": approval.id, "approved": false, "reason": "nao" })).await;
    let done = rt.wait_task(&task.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(20)).await;
    assert!(done.result_text.as_deref().unwrap().starts_with("FALHOU"));
    assert!(!std::path::Path::new(&format!("{ws}/b.txt")).exists());
    let a: Approval = serde_json::from_value(rt.call("approvals.list", json!({})).await[0].clone()).unwrap();
    assert_eq!(a.status, ApprovalStatus::Rejected);
    rt.stop();
}

#[tokio::test]
async fn cancel_and_two_dots_in_parallel_and_conversation() {
    let mut rt = start().await;
    rt.use_fake_engine().await;
    let a = rt.create_dot("A", "read").await;
    let b = rt.create_dot("B", "read").await;

    // Cancelamento de execucao em andamento
    let task: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": a.id, "prompt": "SLEEP:30" })).await).unwrap();
    rt.wait_task(&task.id, &[TaskStatus::Running], Duration::from_secs(10)).await;
    let started = Instant::now();
    rt.call("tasks.cancel", json!({ "id": task.id })).await;
    let done = rt.wait_task(&task.id, &[TaskStatus::Cancelled, TaskStatus::Failed, TaskStatus::Completed], Duration::from_secs(15)).await;
    assert_eq!(done.status, TaskStatus::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(8));

    // Dois DOTs ao mesmo tempo
    let ta: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": a.id, "prompt": "SLEEP:2" })).await).unwrap();
    let tb: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": b.id, "prompt": "SLEEP:2" })).await).unwrap();
    rt.wait_task(&ta.id, &[TaskStatus::Running], Duration::from_secs(10)).await;
    rt.wait_task(&tb.id, &[TaskStatus::Running], Duration::from_secs(10)).await;
    let ov = rt.call("overview.get", json!({})).await;
    assert_eq!(ov["dots_running"], 2);
    assert_eq!(ov["tasks_running"], 2);
    rt.wait_task(&ta.id, &[TaskStatus::Completed], Duration::from_secs(15)).await;
    rt.wait_task(&tb.id, &[TaskStatus::Completed], Duration::from_secs(15)).await;

    // Conversa: mensagem vira tarefa chat; resposta do DOT fica na conversa; sessao do motor guardada
    let conv: Conversation = serde_json::from_value(rt.call("conversations.create", json!({ "dot_id": a.id })).await).unwrap();
    let sent = rt.call("conversations.send", json!({ "conversation_id": conv.id, "content": "ola" })).await;
    let chat_task: Task = serde_json::from_value(sent["task"].clone()).unwrap();
    assert_eq!(chat_task.kind, TaskKind::Chat);
    rt.wait_task(&chat_task.id, &[TaskStatus::Completed], Duration::from_secs(20)).await;
    let msgs: Vec<Message> = serde_json::from_value(rt.call("conversations.messages", json!({ "id": conv.id })).await).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, MessageRole::User);
    assert_eq!(msgs[1].role, MessageRole::Dot);
    assert_eq!(msgs[1].content, "OK");
    assert_eq!(msgs[1].metadata["streaming"], false);
    let conv: Conversation = serde_json::from_value(rt.call("conversations.get", json!({ "id": conv.id })).await).unwrap();
    assert!(conv.engine_sessions["claude"].as_str().unwrap().len() == 36);
    assert_eq!(conv.title, "ola");
    // Segunda mensagem retoma a sessao (--resume) e a memoria/instrucoes chegam ao motor
    rt.call("memories.create", json!({ "dot_id": a.id, "kind": "long_term", "content": "A cor preferida do usuario e azul", "importance": 5 })).await;
    let sent = rt.call("conversations.send", json!({ "conversation_id": conv.id, "content": "ECHO_SYSTEM" })).await;
    let t2: Task = serde_json::from_value(sent["task"].clone()).unwrap();
    let t2 = rt.wait_task(&t2.id, &[TaskStatus::Completed], Duration::from_secs(20)).await;
    let sys = t2.result_text.unwrap();
    assert!(sys.contains("\"A\""), "identidade no system prompt: {sys}");
    let exec: Execution = serde_json::from_value(rt.call("tasks.get", json!({ "id": t2.id })).await["executions"][0].clone()).unwrap();
    assert_eq!(exec.engine_session_id, conv.engine_sessions["claude"].as_str().map(String::from), "retomou a mesma sessao");
    // dots_recall via gateway encontra a memoria
    let sent = rt.call("conversations.send", json!({ "conversation_id": conv.id, "content": "RECALL:cor preferida" })).await;
    let t3: Task = serde_json::from_value(sent["task"].clone()).unwrap();
    let t3 = rt.wait_task(&t3.id, &[TaskStatus::Completed], Duration::from_secs(20)).await;
    assert!(t3.result_text.unwrap().contains("azul"));
    rt.stop();
}

#[tokio::test]
async fn restart_recovers_interrupted_tasks() {
    let mut rt = start().await;
    rt.use_fake_engine().await;
    let dot = rt.create_dot("R", "read").await;
    let safe: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": dot.id, "prompt": "SLEEP:30", "retry_safe": true })).await).unwrap();
    rt.wait_task(&safe.id, &[TaskStatus::Running], Duration::from_secs(10)).await;
    // Mata o runtime sem encerramento ordenado
    unsafe { libc::kill(rt.child.id() as i32, libc::SIGKILL) };
    let _ = rt.child.wait();
    let _ = std::fs::remove_file(rt.dir.path().join("runtime.lock"));
    let mut child = spawn_dotsd(rt.dir.path());
    // runtime.json/socket antigos ficam no disco apos SIGKILL: espera o novo pid publicar.
    let start = Instant::now();
    loop {
        if let Ok(Some(info)) = RuntimeInfo::read(rt.dir.path()) {
            if info.pid == child.id() {
                break;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(15), "novo dotsd nao publicou runtime.json");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let paths = SocketPaths::resolve(rt.dir.path());
    let (client, _) = IpcClient::connect(&paths.socket_path, &paths.read_token().unwrap(), "e2e2").await.unwrap();
    // retry_safe volta para a fila e e reexecutada; execucao antiga marcada como FAILED
    let start = Instant::now();
    loop {
        let v = client.call("tasks.get", json!({ "id": safe.id })).await.unwrap();
        let t: Task = serde_json::from_value(v["task"].clone()).unwrap();
        let execs: Vec<Execution> = serde_json::from_value(v["executions"].clone()).unwrap();
        if execs.len() == 2 && execs[0].status == ExecutionStatus::Failed && execs[0].error.as_deref().map(|e| e.contains("interrompida")).unwrap_or(false) {
            assert!(matches!(t.status, TaskStatus::Running | TaskStatus::Queued | TaskStatus::Completed));
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(15), "recuperacao nao ocorreu: {t:?} {execs:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let _ = child.wait();
}
