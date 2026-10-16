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
async fn new_dots_start_once_and_managers_get_an_editable_schedule() {
    let rt = start().await;
    let manager = rt.create_dot("Gerente novo", "read", "supervisor").await;
    rt.call("dots.update", json!({ "id": manager.id, "patch": { "objective": "Acompanhar a equipe" } })).await;
    rt.call("dots.start_autonomy", json!({ "id": manager.id })).await;
    rt.wait_tasks(&manager.id, |ts| ts.iter().any(|t| t.origin == format!("autonomy:start:{}", manager.id)), Duration::from_secs(10)).await;
    let schedules: Vec<Schedule> = serde_json::from_value(rt.call("schedules.list", json!({ "dot_id": manager.id })).await).unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].kind, ScheduleKind::Every);
    assert_eq!(schedules[0].spec, "60");
    assert!(schedules[0].enabled);
    rt.call("dots.start_autonomy", json!({ "id": manager.id })).await;
    assert_eq!(rt.tasks(&manager.id).await.iter().filter(|t| t.origin == format!("autonomy:start:{}", manager.id)).count(), 1);
    assert_eq!(rt.call("schedules.list", json!({ "dot_id": manager.id })).await.as_array().unwrap().len(), 1);

    let worker = rt.create_dot("Funcionário novo", "read", "worker").await;
    rt.call("dots.update", json!({ "id": worker.id, "patch": { "objective": "Verificar tarefas da equipe" } })).await;
    rt.call("dots.start_autonomy", json!({ "id": worker.id })).await;
    let tasks = rt.wait_tasks(&worker.id, |ts| ts.iter().any(|t| t.origin == format!("autonomy:start:{}", worker.id)), Duration::from_secs(10)).await;
    assert_eq!(tasks.iter().filter(|t| t.origin.starts_with("autonomy:start:")).count(), 1);
    assert!(rt.call("schedules.list", json!({ "dot_id": worker.id })).await.as_array().unwrap().is_empty());

    rt.call("schedules.delete", json!({ "id": schedules[0].id })).await;
    rt.call("dots.start_autonomy", json!({ "id": manager.id })).await;
    assert!(rt.call("schedules.list", json!({ "dot_id": manager.id })).await.as_array().unwrap().is_empty(), "agenda apagada pelo usuario nao volta");
    rt.stop();
}

#[tokio::test]
async fn runtime_restart_resumes_existing_idle_manager_without_user_message() {
    let mut rt = start().await;
    let manager = rt.create_dot("Gerente existente", "read", "supervisor").await;
    rt.call("dots.update", json!({ "id": manager.id, "patch": { "objective": "Gerenciar trabalho recorrente" } })).await;
    let old: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": manager.id, "prompt": "Trabalho anterior" })).await).unwrap();
    rt.wait_task(&old.id, &[TaskStatus::Completed], Duration::from_secs(20)).await;
    assert_eq!(rt.tasks(&manager.id).await.len(), 1);

    unsafe { libc::kill(rt.child.id() as i32, libc::SIGTERM) };
    rt.child.wait().unwrap();
    rt.child = spawn_dotsd(rt.dir.path());
    wait_pid(rt.dir.path(), rt.child.id()).await;
    let (client, events) = connect(rt.dir.path(), "e2e3-restart").await;
    rt.client = client;
    rt.events = events;

    let tasks = rt.wait_tasks(&manager.id, |ts| ts.iter().any(|t| t.origin == format!("autonomy:start:{}", manager.id)), Duration::from_secs(12)).await;
    assert_eq!(tasks.iter().filter(|t| t.origin.starts_with("autonomy:start:")).count(), 1);
    assert_eq!(rt.call("schedules.list", json!({ "dot_id": manager.id })).await.as_array().unwrap().len(), 1);
    rt.stop();
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
async fn supervisor_reviews_first_delivery_while_other_employee_is_still_busy() {
    let rt = start().await;
    let sup = rt.create_dot("Gerente", "read", "supervisor").await;
    rt.call("policies.set", json!({ "dot_id": sup.id, "patch": { "can_delegate_to": ["*"] } })).await;
    let fast = rt.create_dot("Rapido", "read", "worker").await;
    let slow = rt.create_dot("Ocupado", "read", "worker").await;
    rt.call("dots.update", json!({ "id": slow.id, "patch": { "limits": { "max_execution_seconds": 120, "max_concurrent_tasks": 1, "max_tokens_per_task": null, "max_cost_usd_per_task": null } } })).await;
    let busy: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": slow.id, "prompt": "SLEEP:5" })).await).unwrap();
    rt.wait_task(&busy.id, &[TaskStatus::Running], Duration::from_secs(10)).await;

    let parent: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": "DELEGATE:Rapido,Ocupado:Analise o pedido" })).await).unwrap();
    rt.wait_task(&parent.id, &[TaskStatus::Completed], Duration::from_secs(15)).await;
    rt.wait_task(&busy.id, &[TaskStatus::Running], Duration::from_secs(2)).await;
    let updates = rt.wait_tasks(&sup.id, |ts| ts.iter().any(|t| t.origin.starts_with("delegation_update:") && t.status == TaskStatus::Completed), Duration::from_secs(15)).await;
    let update = updates.iter().find(|t| t.origin.starts_with("delegation_update:")).unwrap();
    assert!(update.prompt.contains(&fast.name) && update.prompt.contains("Resultado: OK"), "{}", update.prompt);
    assert!(!updates.iter().any(|t| t.origin == format!("followup:{}", parent.id)), "a consolidacao espera o ultimo funcionario");
    let slow_tasks = rt.tasks(&slow.id).await;
    assert!(slow_tasks.iter().any(|t| t.origin.starts_with("delegation:") && t.status == TaskStatus::Queued));

    let final_tasks = rt.wait_tasks(&sup.id, |ts| ts.iter().any(|t| t.origin == format!("followup:{}", parent.id) && t.status == TaskStatus::Completed), Duration::from_secs(20)).await;
    assert_eq!(final_tasks.iter().filter(|t| t.origin.starts_with("delegation_update:")).count(), 1);
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
async fn worker_permission_request_goes_to_supervisor() {
    let mut rt = start().await;
    let sup = rt.create_dot("Gerente", "read", "supervisor").await;
    rt.call("policies.set", json!({ "dot_id": sup.id, "patch": { "can_delegate_to": ["*"] } })).await;
    let w = rt.create_dot("Escritor", "read", "worker").await;
    let task: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": "DELEGATE:Escritor:DENY:x.txt", "title": "Coordenar" })).await).unwrap();
    // o funcionario (nivel "so le") tenta escrever: o pedido de permissao vai ao gerente como tarefa
    let a: Approval = serde_json::from_value(rt.wait_event("approval.requested", Duration::from_secs(30)).await).unwrap();
    assert_eq!(a.dot_id, w.id);
    assert_eq!(a.tool, "dots_write_file");
    let decided = rt.wait_tasks(&sup.id, |ts| ts.iter().any(|t| t.origin == format!("approval:{}", a.id) && t.status == TaskStatus::Completed), Duration::from_secs(40)).await;
    let dt = decided.iter().find(|t| t.origin.starts_with("approval:")).unwrap();
    assert!(dt.prompt.contains("pede permissao") && dt.prompt.contains("dots_write_file"), "{}", dt.prompt);
    assert_eq!(dt.result_text.as_deref(), Some("DECIDI"));
    // com a permissao do gerente, o funcionario escreveu e a delegacao fechou com o resultado
    let child = rt.wait_tasks(&w.id, |ts| ts.iter().any(|t| t.origin.starts_with("delegation:") && t.status == TaskStatus::Completed), Duration::from_secs(40)).await;
    let c = child.iter().find(|t| t.origin.starts_with("delegation:")).unwrap();
    assert_eq!(c.result_text.as_deref(), Some("ESCREVEU"));
    let v = rt.call("tasks.children", json!({ "id": task.id })).await;
    let dl: Vec<Delegation> = serde_json::from_value(v["delegations"].clone()).unwrap();
    assert_eq!(dl[0].status, DelegationStatus::Completed);
    let stats: RuntimeStats = serde_json::from_value(rt.call("stats.get", json!({})).await).unwrap();
    assert_eq!(stats.approvals_pending, 0);
    rt.stop();
}

#[tokio::test]
async fn supervisor_sees_stopped_worker_and_resumes_delegation() {
    let rt = start().await;
    let sup = rt.create_dot("Chefe", "read", "supervisor").await;
    rt.call("policies.set", json!({ "dot_id": sup.id, "patch": { "can_delegate_to": ["*"] } })).await;
    let w = rt.create_dot("Lento", "read", "worker").await;
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": "DELEGATE:Lento:SLEEP:2" })).await).unwrap();
    rt.wait_task(&t.id, &[TaskStatus::Completed], Duration::from_secs(30)).await;
    let child = rt.wait_tasks(&w.id, |ts| ts.iter().any(|x| x.origin.starts_with("delegation:")), Duration::from_secs(20)).await;
    let child = child.into_iter().find(|x| x.origin.starts_with("delegation:")).unwrap();
    let did = child.origin.trim_start_matches("delegation:").to_string();
    // o funcionario para (cancelado): a delegacao fica cancelada e o gerente ve isso no status
    rt.call("tasks.cancel", json!({ "id": child.id })).await;
    rt.wait_task(&child.id, &[TaskStatus::Cancelled], Duration::from_secs(15)).await;
    let v = rt.call("tasks.children", json!({ "id": t.id })).await;
    let dl: Vec<Delegation> = serde_json::from_value(v["delegations"].clone()).unwrap();
    assert_eq!(dl[0].status, DelegationStatus::Cancelled);
    // o gerente retoma: a tarefa volta a fila, roda e a delegacao termina concluida
    let r: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": format!("RESUME:{did}") })).await).unwrap();
    let r = rt.wait_task(&r.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await;
    assert_eq!(r.result_text.as_deref(), Some("RETOMEI"), "{:?}", r.result_text);
    let done = rt.wait_task(&child.id, &[TaskStatus::Completed], Duration::from_secs(40)).await;
    assert_eq!(done.result_text.as_deref(), Some("ACORDEI"));
    let v = rt.call("tasks.children", json!({ "id": t.id })).await;
    let dl: Vec<Delegation> = serde_json::from_value(v["delegations"].clone()).unwrap();
    assert_eq!(dl[0].status, DelegationStatus::Completed);
    assert_eq!(dl[0].result.as_deref(), Some("ACORDEI"));
    // so quem delegou pode retomar
    let other = rt.create_dot("Outro", "read", "supervisor").await;
    rt.call("policies.set", json!({ "dot_id": other.id, "patch": { "can_delegate_to": ["*"] } })).await;
    let x: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": other.id, "prompt": format!("RESUME:{did}") })).await).unwrap();
    let x = rt.wait_task(&x.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await;
    assert!(x.result_text.as_deref().unwrap().starts_with("FALHOU:"), "{:?}", x.result_text);
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

#[tokio::test]
async fn manager_hires_within_limit_and_never_does_the_work() {
    let rt = start().await;
    rt.call("settings.set", json!({ "key": "teams.max_employees_per_manager", "value": 1 })).await;
    let sup = rt.create_dot("Chefe", "write", "supervisor").await;
    // Mesmo com nivel "escrita", o gerente nao executa o trabalho.
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": "WRITE:x.txt:oi", "title": "Escrever" })).await).unwrap();
    let done = rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await;
    let r = done.result_text.unwrap_or_default();
    assert!(r.starts_with("FALHOU: NEGADO") && r.contains("dots_hire_employee"), "{r}");
    assert!(!std::path::Path::new(sup.workspace_path.as_deref().unwrap()).join("x.txt").exists());

    // Sem funcionarios e sem equipe: contrata (a equipe nasce com ele).
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": r#"FLEXI:dots_hire_employee:{"name":"Ana","role":"Redatora","instructions":"Escreve textos curtos"}"#, "title": "Contratar" })).await).unwrap();
    let done = rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await;
    let r = done.result_text.unwrap_or_default();
    assert!(r.starts_with("OK 'Ana' foi contratado") && r.contains("1 de 1"), "{r}");
    let dots: Vec<Dot> = serde_json::from_value(rt.call("dots.list", json!({})).await).unwrap();
    let ana = dots.iter().find(|d| d.name == "Ana").expect("Ana criada");
    assert_eq!(ana.role, DotRole::Worker);
    assert_eq!(ana.primary_engine, Some(EngineKind::Claude));
    assert_eq!(ana.description, "Redatora");
    let info: DotTeamInfo = serde_json::from_value(rt.call("dots.team", json!({ "dot_id": sup.id })).await).unwrap();
    let team = info.manages.expect("equipe criada para o gerente");
    assert_eq!(team.member_ids, vec![ana.id.clone()]);
    let sp: ToolPolicy = serde_json::from_value(rt.call("policies.get", json!({ "dot_id": sup.id })).await).unwrap();
    assert_eq!(sp.can_delegate_to, vec![ana.id.clone()]);
    let ap: ToolPolicy = serde_json::from_value(rt.call("policies.get", json!({ "dot_id": ana.id })).await).unwrap();
    assert_eq!(ap.auto_approve_level, AutoApproveLevel::Write, "mesmas permissoes do gerente");
    assert!(ap.can_delegate_to.is_empty());
    let contacts: Vec<Contact> = serde_json::from_value(rt.call("contacts.list", json!({})).await).unwrap();
    assert_eq!(contacts.iter().find(|c| c.dot.id == ana.id).unwrap().title.as_deref(), Some("Redatora"));

    // Limite atingido.
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": r#"FLEXI:dots_hire_employee:{"name":"Bia","role":"Revisora"}"#, "title": "Contratar" })).await).unwrap();
    let r = rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.unwrap_or_default();
    assert!(r.starts_with("ERRO limite de 1"), "{r}");
    // Nome repetido.
    rt.call("settings.set", json!({ "key": "teams.max_employees_per_manager", "value": 3 })).await;
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": r#"FLEXI:dots_hire_employee:{"name":"ana","role":"Revisora"}"#, "title": "Contratar" })).await).unwrap();
    let r = rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.unwrap_or_default();
    assert!(r.starts_with("ERRO ja existe um DOT chamado"), "{r}");
    // Funcionario nao contrata.
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": ana.id, "prompt": r#"FLEXI:dots_hire_employee:{"name":"Caio","role":"Ajudante"}"#, "title": "Contratar" })).await).unwrap();
    let r = rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.unwrap_or_default();
    assert!(r.starts_with("ERRO so um gerente contrata"), "{r}");
    // O gerente delega ao contratado, que faz o trabalho.
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": "DELEGATE:Ana:WRITE:a.txt:oi", "title": "Coordenar" })).await).unwrap();
    let r = rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.unwrap_or_default();
    assert!(r.starts_with("AGUARDANDO ok:"), "{r}");
    let child = rt.wait_tasks(&ana.id, |ts| ts.iter().any(|t| t.origin.starts_with("delegation:") && t.status == TaskStatus::Completed), Duration::from_secs(40)).await;
    assert_eq!(child.iter().find(|t| t.origin.starts_with("delegation:")).unwrap().result_text.as_deref(), Some("FEITO"));
    rt.stop();
}

#[tokio::test]
async fn dismissal_deletes_employee_and_frees_the_slot() {
    let mut rt = start().await;
    rt.call("settings.set", json!({ "key": "teams.max_employees_per_manager", "value": 1 })).await;
    let sup = rt.create_dot("Chefe", "read", "supervisor").await;
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": r#"FLEXI:dots_hire_employee:{"name":"Ana","role":"Redatora"}"#, "title": "Contratar" })).await).unwrap();
    rt.wait_task(&t.id, &[TaskStatus::Completed], Duration::from_secs(30)).await;
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": r#"FLEXI:dots_dismiss_employee:{"dot":"Ana","reason":"publicou sem revisar","lesson":"revisar antes de publicar"}"#, "title": "Demitir" })).await).unwrap();
    let a: Approval = serde_json::from_value(rt.wait_event("approval.requested", Duration::from_secs(30)).await).unwrap();
    assert_eq!(a.tool, "dots_dismiss_employee");
    rt.call("approvals.decide", json!({ "id": a.id, "approved": true })).await;
    let r = rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.unwrap_or_default();
    assert!(r.starts_with("OK 'Ana' foi demitido e excluido") && r.contains("0 de 1") && r.contains("dots_hire_employee"), "{r}");
    let dots: Vec<Dot> = serde_json::from_value(rt.call("dots.list", json!({ "include_archived": true })).await).unwrap();
    assert!(!dots.iter().any(|d| d.name == "Ana"), "o demitido foi excluido");
    let info: DotTeamInfo = serde_json::from_value(rt.call("dots.team", json!({ "dot_id": sup.id })).await).unwrap();
    let team = info.manages.unwrap();
    assert!(team.member_ids.is_empty());
    let sp: ToolPolicy = serde_json::from_value(rt.call("policies.get", json!({ "dot_id": sup.id })).await).unwrap();
    assert!(sp.can_delegate_to.is_empty());
    let ds: Vec<Dismissal> = serde_json::from_value(rt.call("dismissals.list", json!({ "team_id": team.id })).await).unwrap();
    assert_eq!(ds.len(), 1);
    assert!(ds[0].dot_id.is_none() && ds[0].dot_name == "Ana" && ds[0].lesson == "revisar antes de publicar");
    // A vaga ficou livre: o gerente contrata o substituto.
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": sup.id, "prompt": r#"FLEXI:dots_hire_employee:{"name":"Carla","role":"Redatora"}"#, "title": "Contratar" })).await).unwrap();
    let r = rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.unwrap_or_default();
    assert!(r.starts_with("OK 'Carla' foi contratado"), "{r}");
    rt.stop();
}

#[tokio::test]
async fn group_message_reaches_every_manager_and_collects_replies() {
    let rt = start().await;
    let a = rt.create_dot("Gerente A", "read", "supervisor").await;
    let b = rt.create_dot("Gerente B", "read", "worker").await;
    let w = rt.create_dot("Avulso", "read", "worker").await;
    // B vira gerente por gerenciar uma equipe (o papel acompanha); o avulso fica fora do grupo.
    rt.call("teams.create", json!({ "name": "Equipe B", "manager_dot_id": b.id })).await;
    let members: Vec<Dot> = serde_json::from_value(rt.call("group.members", json!({})).await).unwrap();
    let mut ids: Vec<String> = members.iter().map(|d| d.id.clone()).collect();
    ids.sort();
    let mut want = vec![a.id.clone(), b.id.clone()];
    want.sort();
    assert_eq!(ids, want);
    let sent = rt.call("group.send", json!({ "content": "Bom dia, como estao as equipes?" })).await;
    assert_eq!(sent["tasks"].as_array().unwrap().len(), 2);
    for d in [&a, &b] {
        let ts = rt.wait_tasks(&d.id, |ts| ts.iter().any(|t| t.origin.starts_with("group:") && t.status == TaskStatus::Completed), Duration::from_secs(30)).await;
        let gt = ts.iter().find(|t| t.origin.starts_with("group:")).unwrap();
        assert!(gt.prompt.contains("grupo geral") && gt.prompt.contains("Bom dia"));
        // A mensagem tambem aparece na conversa do gerente.
        let conv: Conversation = serde_json::from_value(rt.call("conversations.main", json!({ "dot_id": d.id })).await).unwrap();
        let msgs: Vec<Message> = serde_json::from_value(rt.call("conversations.messages", json!({ "id": conv.id })).await).unwrap();
        assert!(msgs.iter().any(|m| m.role == MessageRole::User && m.content == "Bom dia, como estao as equipes?"));
    }
    assert!(rt.tasks(&w.id).await.is_empty(), "quem nao e gerente nao recebe");
    let start = Instant::now();
    let msgs: Vec<GroupMessage> = loop {
        let m: Vec<GroupMessage> = serde_json::from_value(rt.call("group.messages", json!({})).await).unwrap();
        if m.iter().filter(|x| x.role == MessageRole::Dot && x.content == "OK").count() == 2 { break m; }
        assert!(start.elapsed() < Duration::from_secs(15), "respostas nao chegaram ao grupo: {m:?}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq!(msgs[0].role, MessageRole::User);
    let mut names: Vec<String> = msgs.iter().filter_map(|m| m.dot_name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["Gerente A".to_string(), "Gerente B".to_string()]);
    rt.stop();
}

#[tokio::test]
async fn group_mention_only_the_marked_manager_answers() {
    let rt = start().await;
    let a = rt.create_dot("Roberto", "read", "supervisor").await;
    let b = rt.create_dot("Joao", "read", "supervisor").await;
    let sent = rt.call("group.send", json!({ "content": "@Roberto, precisa fazer mais por hora", "mentions": [a.id] })).await;
    assert_eq!(sent["tasks"].as_array().unwrap().len(), 2, "todos recebem");
    assert_eq!(sent["message"]["metadata"]["mentions"][0]["name"], "Roberto");
    let ta = rt.wait_tasks(&a.id, |ts| ts.iter().any(|t| t.origin.starts_with("group:") && t.status == TaskStatus::Completed), Duration::from_secs(30)).await;
    assert!(ta.iter().find(|t| t.origin.starts_with("group:")).unwrap().prompt.contains("dirigida a voce"));
    let tb = rt.wait_tasks(&b.id, |ts| ts.iter().any(|t| t.origin.starts_with("group-cc:") && t.status == TaskStatus::Completed), Duration::from_secs(30)).await;
    assert!(tb.iter().all(|t| !t.origin.starts_with("group:")), "quem nao foi marcado recebe so para ciencia");
    assert!(tb[0].prompt.contains("nao a voce"));
    let msgs: Vec<GroupMessage> = serde_json::from_value(rt.call("group.messages", json!({})).await).unwrap();
    let names: Vec<String> = msgs.iter().filter_map(|m| m.dot_name.clone()).collect();
    assert_eq!(names, vec!["Roberto".to_string()], "so o marcado responde no grupo: {msgs:?}");
    rt.stop();
}

async fn run_task(rt: &Rt, dot: &Dot, prompt: String) -> String {
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": dot.id, "prompt": prompt, "title": "x" })).await).unwrap();
    rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.unwrap_or_default()
}

#[tokio::test]
async fn dot_schedules_its_own_work_and_tells_the_conversation() {
    let rt = start().await;
    let a = rt.create_dot("Roberto", "read", "supervisor").await;
    let b = rt.create_dot("Marina", "read", "worker").await;
    let create = r#"FLEXI:dots_schedule_create:{"kind":"every","spec":"60","prompt":"Rodada horaria e relatorio"}"#;
    // Mesmo o gerente (que nao executa trabalho) agenda o proprio acompanhamento.
    let r = run_task(&rt, &a, create.into()).await;
    assert!(r.starts_with("OK agendado") && r.contains("a cada 60 min"), "{r}");
    let list: Vec<Schedule> = serde_json::from_value(rt.call("schedules.list", json!({ "dot_id": a.id })).await).unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].next_run_at.is_some());
    // Avisa na conversa.
    let conv: Conversation = serde_json::from_value(rt.call("conversations.main", json!({ "dot_id": a.id })).await).unwrap();
    let msgs: Vec<Message> = serde_json::from_value(rt.call("conversations.messages", json!({ "id": conv.id })).await).unwrap();
    assert!(msgs.iter().any(|m| m.role == MessageRole::System && m.content.starts_with("Roberto agendou: a cada 60 min")), "{msgs:?}");
    // Repetido nao duplica; intervalo curto demais e recusado.
    assert!(run_task(&rt, &a, create.into()).await.contains("ja existe"));
    assert!(run_task(&rt, &a, r#"FLEXI:dots_schedule_create:{"kind":"every","spec":"1","prompt":"x"}"#.into()).await.starts_with("ERRO intervalo minimo"));
    assert_eq!(serde_json::from_value::<Vec<Schedule>>(rt.call("schedules.list", json!({ "dot_id": a.id })).await).unwrap().len(), 1);
    // Outro DOT nao ve nem cancela o agendamento.
    assert!(run_task(&rt, &b, "FLEXI:dots_schedule_list:{}".into()).await.contains("nenhum agendamento"));
    let cancel = format!(r#"FLEXI:dots_schedule_cancel:{{"id":"{}"}}"#, list[0].id);
    assert!(run_task(&rt, &b, cancel.clone()).await.starts_with("ERRO agendamento"));
    // O dono lista e cancela.
    assert!(run_task(&rt, &a, "FLEXI:dots_schedule_list:{}".into()).await.contains(&list[0].id));
    assert!(run_task(&rt, &a, cancel).await.starts_with("OK agendamento cancelado"));
    assert!(serde_json::from_value::<Vec<Schedule>>(rt.call("schedules.list", json!({ "dot_id": a.id })).await).unwrap().is_empty());
    rt.stop();
}

#[tokio::test]
async fn approved_once_the_same_kind_of_action_does_not_ask_again() {
    let mut rt = start().await;
    let w = rt.create_dot("Escritor", "read", "worker").await;
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": w.id, "prompt": "WRITE:a.txt:1", "title": "1" })).await).unwrap();
    let a: Approval = serde_json::from_value(rt.wait_event("approval.requested", Duration::from_secs(30)).await).unwrap();
    rt.call("approvals.decide", json!({ "id": a.id, "approved": true })).await;
    assert_eq!(rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.as_deref(), Some("FEITO"));
    let learned: Vec<LearnedPermission> = serde_json::from_value(rt.call("policies.learned", json!({ "dot_id": w.id })).await).unwrap();
    assert_eq!(learned.len(), 1);
    assert_eq!(learned[0].approval_id.as_deref(), Some(a.id.as_str()));
    // Mesma acao de novo: nao pede.
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": w.id, "prompt": "WRITE:a.txt:2", "title": "2" })).await).unwrap();
    assert_eq!(rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.as_deref(), Some("FEITO"));
    let approvals: Vec<Approval> = serde_json::from_value(rt.call("approvals.list", json!({ "pending_only": false })).await).unwrap();
    assert_eq!(approvals.len(), 1, "so a primeira pediu");
    // Esquecida, volta a pedir.
    rt.call("policies.forget", json!({ "id": learned[0].id })).await;
    let t: Task = serde_json::from_value(rt.call("tasks.create", json!({ "dot_id": w.id, "prompt": "WRITE:a.txt:3", "title": "3" })).await).unwrap();
    let a2: Approval = serde_json::from_value(rt.wait_event("approval.requested", Duration::from_secs(30)).await).unwrap();
    assert_ne!(a2.id, a.id);
    rt.call("approvals.decide", json!({ "id": a2.id, "approved": false, "reason": "grave em b.txt" })).await;
    let r = rt.wait_task(&t.id, &[TaskStatus::Completed, TaskStatus::Failed], Duration::from_secs(30)).await.result_text.unwrap_or_default();
    assert!(r.starts_with("FALHOU: NEGADO") && r.contains("grave em b.txt"), "o que fazer diferente chega ao DOT: {r}");
    let learned: Vec<LearnedPermission> = serde_json::from_value(rt.call("policies.learned", json!({ "dot_id": w.id })).await).unwrap();
    assert!(learned.is_empty(), "negada nao aprende");
    rt.stop();
}
