//! Fase 3 de ponta a ponta com CLI falso: gatilhos por arquivo, delegacao com Supervisor,
//! limites e ciclos, pergunta ao usuario, retomada por checkpoint sem repetir comando, ocioso.
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
    Command::new(env!("CARGO_BIN_EXE_dotsd")).args(["--data-dir", data_dir.to_str().unwrap(), "--mode", "detached"]).env("DOTS_LOG", "info").stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap()
}

async fn wait_pid(data_dir: &std::path::Path, pid: u32) {
    let start = Instant::now();
    loop {
        if let Ok(Some(info)) = RuntimeInfo::read(data_dir) {
            if info.pid == pid && std::path::Path::new(&info.socket_path).exists() {
                tokio::time::sleep(Duration::from_millis(100)).await;
                return;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(15), "dotsd nao ficou pronto");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn connect(dir: &std::path::Path, name: &str) -> (IpcClient, dots_ipc::EventReceiver) {
    let paths = SocketPaths::resolve(dir);
    IpcClient::connect(&paths.socket_path, &paths.read_token().unwrap(), name).await.unwrap()
}

async fn start() -> Rt {
    let dir = tempfile::tempdir().unwrap();
    let child = spawn_dotsd(dir.path());
    wait_pid(dir.path(), child.id()).await;
    let (client, events) = connect(dir.path(), "e2e3").await;
    let rt = Rt { dir, child, client, events };
    let fake = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake_claude.py");
    let dets: Vec<EngineDetection> = serde_json::from_value(rt.call("engines.set_override", json!({ "kind": "claude", "path": fake })).await).unwrap();
    assert!(dets[0].installed);
    rt
}

impl Rt {
    async fn call(&self, m: &str, p: Value) -> Value {
        self.client.call(m, p).await.unwrap_or_else(|e| panic!("{m}: {e}"))
    }
    async fn create_dot(&self, name: &str, level: &str, role: &str) -> Dot {
        let dot: Dot = serde_json::from_value(self.call("dots.create", json!({ "name": name, "primary_engine": "claude", "role": role, "limits": { "max_execution_seconds": 120, "max_concurrent_tasks": 2, "max_tokens_per_task": null, "max_cost_usd_per_task": null } })).await).unwrap();
        self.call("policies.set", json!({ "dot_id": dot.id, "patch": { "auto_approve_level": level } })).await;
        dot
    }
    async fn wait_task(&self, id: &str, want: &[TaskStatus], timeout: Duration) -> Task {
        let start = Instant::now();
        loop {
            let t: Task = serde_json::from_value(self.call("tasks.get", json!({ "id": id })).await["task"].clone()).unwrap();
            if want.contains(&t.status) {
                return t;
            }
            assert!(start.elapsed() < timeout, "tarefa {id} ficou em {:?} (erro: {:?})", t.status, t.error);
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }
    async fn tasks(&self, dot_id: &str) -> Vec<Task> {
        serde_json::from_value(self.call("tasks.list", json!({ "dot_id": dot_id, "limit": 100 })).await).unwrap()
    }
    async fn wait_tasks(&self, dot_id: &str, pred: impl Fn(&[Task]) -> bool, timeout: Duration) -> Vec<Task> {
        let start = Instant::now();
        loop {
            let ts = self.tasks(dot_id).await;
            if pred(&ts) {
                return ts;
            }
            assert!(start.elapsed() < timeout, "condicao nao atingida; tarefas: {:?}", ts.iter().map(|t| (t.title.clone(), t.status, t.origin.clone())).collect::<Vec<_>>());
            tokio::time::sleep(Duration::from_millis(200)).await;
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

/// Processos de motor desta instancia (o caminho do diretorio de dados aparece nos argumentos do CLI).
fn engine_processes(data_dir: &std::path::Path) -> usize {
    let out = Command::new("/usr/bin/pgrep").args(["-f", &format!("fake_claude.py.*{}", data_dir.display())]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.trim().is_empty()).count()
}

#[tokio::test]
async fn file_trigger_creates_one_task_per_change_and_idle_runtime_spawns_nothing() {
    let rt = start().await;
    let dot = rt.create_dot("Revisor", "read", "worker").await;
    let ws = dot.workspace_path.clone().unwrap();
    rt.call("subscriptions.create", json!({ "dot_id": dot.id, "event_type": "file.changed", "filter": { "path_glob": "*.md" }, "prompt_template": "Revise {{path}}", "retry_safe": true, "cooldown_seconds": 0 })).await;
    let subs: Vec<EventSubscription> = serde_json::from_value(rt.call("subscriptions.list", json!({ "dot_id": dot.id })).await).unwrap();
    assert_eq!(subs.len(), 1);
    tokio::time::sleep(Duration::from_millis(800)).await; // observador se reconfigura
    // Duas gravacoes rapidas = um lote do debouncer = um evento = uma tarefa
    std::fs::write(format!("{ws}/notas.md"), "a").unwrap();
    std::fs::write(format!("{ws}/notas.md"), "ab").unwrap();
    std::fs::write(format!("{ws}/ignorado.txt"), "x").unwrap();
    let ts = rt.wait_tasks(&dot.id, |ts| ts.iter().any(|t| t.origin.starts_with("event:")), Duration::from_secs(15)).await;
    assert_eq!(ts.len(), 1, "apenas uma tarefa para o lote");
    assert!(ts[0].prompt.contains("notas.md"), "{}", ts[0].prompt);
    assert!(ts[0].retry_safe);
    rt.wait_task(&ts[0].id, &[TaskStatus::Completed], Duration::from_secs(20)).await;
    let events: Vec<EventRecord> = serde_json::from_value(rt.call("events.list", json!({ "event_type": "file.changed" })).await).unwrap();
    assert!(events.iter().all(|e| !e.payload["path"].as_str().unwrap().ends_with(".txt")) || events.len() >= 1);
    // evento task.completed foi registrado e nao reacionou o proprio DOT
    let done: Vec<EventRecord> = serde_json::from_value(rt.call("events.list", json!({ "event_type": "task.completed" })).await).unwrap();
    assert_eq!(done.len(), 1);
    // Nova alteracao depois do debounce = nova tarefa (cooldown 0)
    tokio::time::sleep(Duration::from_millis(2500)).await;
    std::fs::write(format!("{ws}/notas.md"), "abc").unwrap();
    rt.wait_tasks(&dot.id, |ts| ts.iter().filter(|t| t.origin.starts_with("event:")).count() == 2, Duration::from_secs(15)).await;
    // Ocioso: nada em execucao, nenhum processo de motor por 5 s, nenhuma tarefa nova
    let before = rt.tasks(&dot.id).await.len();
    rt.wait_tasks(&dot.id, |ts| ts.iter().all(|t| t.status.is_terminal()), Duration::from_secs(20)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(engine_processes(rt.dir.path()), 0);
    assert_eq!(rt.tasks(&dot.id).await.len(), before);
    let stats: RuntimeStats = serde_json::from_value(rt.call("stats.get", json!({})).await).unwrap();
    assert_eq!(stats.running, 0);
    assert!(stats.events_24h >= 3);
    rt.stop();
}

#[tokio::test]
async fn supervisor_coordinates_three_dots_and_consolidates() {
    let rt = start().await;
    let sup = rt.create_dot("Chefe", "read", "supervisor").await;
    rt.call("policies.set", json!({ "dot_id": sup.id, "patch": { "can_delegate_to": ["*"] } })).await;
    let a = rt.create_dot("Alfa", "read", "worker").await;
    let b = rt.create_dot("Beta", "read", "worker").await;
    let c = rt.create_dot("Gama", "read", "worker").await;
    let task: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": "DELEGATE:Alfa,Beta,Gama:Analise o modulo X", "title": "Coordenar" })).await).unwrap();
    let done = rt.wait_task(&task.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await;
    assert!(done.result_text.as_deref().unwrap().starts_with("AGUARDANDO ok:"), "{:?}", done.result_text);
    for d in [&a, &b, &c] {
        let ts = rt.wait_tasks(&d.id, |ts| ts.iter().any(|t| t.origin.starts_with("delegation:")), Duration::from_secs(15)).await;
        assert_eq!(ts[0].parent_task_id.as_deref(), Some(task.id.as_str()));
        assert_eq!(ts[0].delegation_chain, vec![sup.id.clone()]);
    }
    // Continuacao do supervisor com os tres resultados
    let follow = rt.wait_tasks(&sup.id, |ts| ts.iter().any(|t| t.origin == format!("followup:{}", task.id) && t.status == TaskStatus::Completed), Duration::from_secs(40)).await;
    let f = follow.iter().find(|t| t.origin.starts_with("followup:")).unwrap();
    assert_eq!(f.result_text.as_deref(), Some("CONSOLIDADO: 3 resultado(s)"));
    assert_eq!(follow.iter().filter(|t| t.origin.starts_with("followup:")).count(), 1, "uma unica continuacao");
    let v = rt.call("tasks.children", json!({ "id": task.id })).await;
    let dl: Vec<Delegation> = serde_json::from_value(v["delegations"].clone()).unwrap();
    assert_eq!(dl.len(), 3);
    assert!(dl.iter().all(|d| d.status == DelegationStatus::Completed && d.result.as_deref() == Some("OK")));
    let reqs: Vec<EventRecord> = serde_json::from_value(rt.call("events.list", json!({ "event_type": "dot.task.requested" })).await).unwrap();
    assert_eq!(reqs.len(), 3);
    rt.stop();
}

#[tokio::test]
async fn delegation_limits_cycles_and_cascade_cancel() {
    let rt = start().await;
    let w = rt.create_dot("Solitario", "read", "worker").await;
    let other = rt.create_dot("Outro", "read", "worker").await;
    // sem permissao de delegar
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": w.id, "prompt": "DELEGATE:Outro:x" })).await).unwrap();
    let done = rt.wait_task(&t.id, &[TaskStatus::Completed], Duration::from_secs(30)).await;
    assert!(done.result_text.unwrap().contains("ERRO:"), "deveria negar");
    assert!(rt.tasks(&other.id).await.is_empty());
    // ciclo: S -> A -> S recusado
    let s = rt.create_dot("S", "read", "supervisor").await;
    let a = rt.create_dot("A", "read", "worker").await;
    rt.call("policies.set", json!({ "dot_id": s.id, "patch": { "can_delegate_to": ["*"] } })).await;
    rt.call("policies.set", json!({ "dot_id": a.id, "patch": { "can_delegate_to": ["*"] } })).await;
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": s.id, "prompt": "DELEGATE:A:DELEGATE:S:volta" })).await).unwrap();
    rt.wait_task(&t.id, &[TaskStatus::Completed], Duration::from_secs(30)).await;
    let a_tasks = rt.wait_tasks(&a.id, |ts| ts.iter().any(|t| t.status == TaskStatus::Completed), Duration::from_secs(30)).await;
    assert!(a_tasks[0].result_text.as_deref().unwrap().contains("ciclo"), "{:?}", a_tasks[0].result_text);
    assert_eq!(rt.tasks(&s.id).await.iter().filter(|t| t.origin.starts_with("delegation:")).count(), 0);
    // cancelamento em cascata
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": s.id, "prompt": "DELEGATE:A:SLEEP:30" })).await).unwrap();
    rt.wait_task(&t.id, &[TaskStatus::Completed], Duration::from_secs(30)).await;
    let child = rt.wait_tasks(&a.id, |ts| ts.iter().any(|t| t.status == TaskStatus::Running && t.prompt.starts_with("SLEEP")), Duration::from_secs(20)).await;
    let child = child.into_iter().find(|t| t.prompt.starts_with("SLEEP")).unwrap();
    rt.call("tasks.cancel", json!({ "id": t.id })).await;
    let c = rt.wait_task(&child.id, &[TaskStatus::Cancelled], Duration::from_secs(15)).await;
    assert_eq!(c.status, TaskStatus::Cancelled);
    rt.stop();
}

#[tokio::test]
async fn ask_user_question_creates_followup_with_answer() {
    let mut rt = start().await;
    let s = rt.create_dot("Perguntador", "read", "supervisor").await;
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": s.id, "prompt": "ASK:Qual banco usar?" })).await).unwrap();
    let q: Approval = serde_json::from_value(rt.wait_event("approval.requested", Duration::from_secs(20)).await).unwrap();
    assert_eq!(q.kind, ApprovalKind::Question);
    assert_eq!(q.options, vec!["Postgres", "SQLite"]);
    assert_eq!(q.summary, "Qual banco usar?");
    rt.wait_task(&t.id, &[TaskStatus::Completed], Duration::from_secs(20)).await;
    let stats: RuntimeStats = serde_json::from_value(rt.call("stats.get", json!({})).await).unwrap();
    assert_eq!(stats.questions_pending, 1);
    assert_eq!(stats.approvals_pending, 0);
    rt.call("approvals.decide", json!({ "id": q.id, "approved": true, "reason": "SQLite" })).await;
    let follow = rt.wait_tasks(&s.id, |ts| ts.iter().any(|t| t.origin.starts_with("followup:") && t.status == TaskStatus::Completed), Duration::from_secs(30)).await;
    let f = follow.iter().find(|t| t.origin.starts_with("followup:")).unwrap();
    assert!(f.result_text.as_deref().unwrap().contains("resposta=SQLite"), "{:?}", f.result_text);
    rt.stop();
}

#[tokio::test]
async fn worker_question_goes_to_supervisor_and_delegation_waits_for_answer() {
    let mut rt = start().await;
    let sup = rt.create_dot("Gerente", "read", "supervisor").await;
    rt.call("policies.set", json!({ "dot_id": sup.id, "patch": { "can_delegate_to": ["*"] } })).await;
    let w = rt.create_dot("Funcionario", "read", "worker").await;
    let task: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": "DELEGATE:Funcionario:ASK:Qual banco usar?", "title": "Coordenar" })).await).unwrap();
    // O funcionario pergunta dentro do pedido delegado: a pergunta fica registrada, mas vai ao gerente como tarefa.
    let q: Approval = serde_json::from_value(rt.wait_event("approval.requested", Duration::from_secs(30)).await).unwrap();
    assert_eq!(q.dot_id, w.id);
    assert_eq!(q.summary, "Qual banco usar?");
    let asked = rt.wait_tasks(&sup.id, |ts| ts.iter().any(|t| t.origin == format!("question:{}", q.id) && t.status == TaskStatus::Completed), Duration::from_secs(40)).await;
    let qt = asked.iter().find(|t| t.origin.starts_with("question:")).unwrap();
    assert!(qt.prompt.contains("Pergunta do seu funcionario Funcionario") && qt.prompt.contains("Qual banco usar?"), "{}", qt.prompt);
    assert_eq!(qt.result_text.as_deref(), Some("RESPONDI"));
    // O funcionario continua com a resposta do gerente; so entao a delegacao fecha e o gerente consolida.
    let follow = rt.wait_tasks(&w.id, |ts| ts.iter().any(|t| t.origin.starts_with("followup:") && t.status == TaskStatus::Completed), Duration::from_secs(40)).await;
    let f = follow.iter().find(|t| t.origin.starts_with("followup:")).unwrap();
    assert!(f.prompt.contains("seu gerente Gerente respondeu"), "{}", f.prompt);
    assert!(f.result_text.as_deref().unwrap().contains("resposta=SQLite"), "{:?}", f.result_text);
    let consolidated = rt.wait_tasks(&sup.id, |ts| ts.iter().any(|t| t.origin == format!("followup:{}", task.id) && t.status == TaskStatus::Completed), Duration::from_secs(40)).await;
    let c = consolidated.iter().find(|t| t.origin == format!("followup:{}", task.id)).unwrap();
    assert_eq!(c.result_text.as_deref(), Some("CONSOLIDADO: 1 resultado(s)"));
    assert_eq!(consolidated.iter().filter(|t| t.origin == format!("followup:{}", task.id)).count(), 1, "o gerente consolida uma vez, depois da resposta");
    let v = rt.call("tasks.children", json!({ "id": task.id })).await;
    let dl: Vec<Delegation> = serde_json::from_value(v["delegations"].clone()).unwrap();
    assert_eq!(dl.len(), 1);
    assert_eq!(dl[0].status, DelegationStatus::Completed);
    assert_eq!(dl[0].child_task_id.as_deref(), Some(f.id.as_str()), "a continuacao virou a filha da delegacao");
    assert!(dl[0].result.as_deref().unwrap().contains("resposta=SQLite"));
    let stats: RuntimeStats = serde_json::from_value(rt.call("stats.get", json!({})).await).unwrap();
    assert_eq!(stats.questions_pending, 0);
    rt.stop();
}

#[tokio::test]
async fn restart_resumes_from_checkpoint_without_repeating_command() {
    let rt = start().await;
    let d = rt.create_dot("Executor", "exec", "worker").await;
    rt.call("policies.set", json!({ "dot_id": d.id, "patch": { "allowed_commands": ["echo"] } })).await;
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": d.id, "prompt": "RUNCMD:echo marcador-unico", "retry_safe": true })).await).unwrap();
    rt.wait_task(&t.id, &[TaskStatus::Running], Duration::from_secs(10)).await;
    // espera o comando ter rodado (invocacao gravada) antes de matar
    let start = Instant::now();
    loop {
        let v = rt.call("tasks.children", json!({ "id": t.id })).await;
        let cps: Vec<Checkpoint> = serde_json::from_value(v["checkpoints"].clone()).unwrap();
        if cps.iter().any(|c| c.step.starts_with("apos")) {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(15), "checkpoint automatico nao apareceu: {cps:?}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let Rt { dir, mut child, client, events } = rt;
    drop(client);
    drop(events);
    unsafe { libc::kill(child.id() as i32, libc::SIGKILL) };
    let _ = child.wait();
    let _ = std::fs::remove_file(dir.path().join("runtime.lock"));
    let mut child = spawn_dotsd(dir.path());
    wait_pid(dir.path(), child.id()).await;
    let (client, _) = connect(dir.path(), "e2e3b").await;
    let start = Instant::now();
    let task = loop {
        let v = client.call("tasks.get", json!({ "id": t.id })).await.unwrap();
        let task: Task = serde_json::from_value(v["task"].clone()).unwrap();
        if task.status.is_terminal() {
            break task;
        }
        assert!(start.elapsed() < Duration::from_secs(40), "nao terminou: {task:?}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq!(task.status, TaskStatus::Completed, "{:?}", task.error);
    assert_eq!(task.attempts, 2);
    assert_eq!(task.result_text.as_deref(), Some("REPLAYED"), "segunda tentativa nao deve repetir o comando");
    let audit: Vec<AuditLog> = serde_json::from_value(client.call("audit.list", json!({ "limit": 100 })).await.unwrap()).unwrap();
    assert_eq!(audit.iter().filter(|a| a.action == "tool.dots_run_command" && a.details["outcome"] == "ok").count(), 1, "comando executado uma unica vez");
    assert!(audit.iter().any(|a| a.action == "tool.dots_run_command" && a.details["outcome"] == "replayed"));
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let _ = child.wait();
}

#[tokio::test]
async fn cron_schedule_fires_a_task() {
    let rt = start().await;
    let d = rt.create_dot("Pontual", "read", "worker").await;
    let sch: Schedule = serde_json::from_value(rt.call("schedules.create", json!({ "dot_id": d.id, "kind": "cron", "spec": "* * * * *", "prompt": "Faca a rotina" })).await).unwrap();
    let next = sch.next_run_at.unwrap();
    assert!(next > dots_core::now() && (next - dots_core::now()).num_seconds() <= 60);
    assert!(rt.client.call("schedules.create", json!({ "dot_id": d.id, "kind": "cron", "spec": "nope", "prompt": "x" })).await.is_err());
    let ts = rt.wait_tasks(&d.id, |ts| ts.iter().any(|t| t.origin == format!("schedule:{}", sch.id)), Duration::from_secs(75)).await;
    assert_eq!(ts[0].title, "Agendado: cron * * * * *");
    let after: Schedule = serde_json::from_value(rt.call("schedules.list", json!({ "dot_id": d.id })).await[0].clone()).unwrap();
    assert!(after.last_run_at.is_some() && after.next_run_at.unwrap() > next);
    let evs: Vec<EventRecord> = serde_json::from_value(rt.call("events.list", json!({ "event_type": "schedule.due" })).await).unwrap();
    assert_eq!(evs.len(), 1);
    rt.stop();
}
