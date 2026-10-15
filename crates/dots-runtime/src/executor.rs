//! Executa uma tarefa: monta o contexto, inicia o motor, persiste eventos e finaliza.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dots_core::automation::event_types as et;
use dots_core::{DotRole, DotsError, DotsResult, EngineKind, ExecutionEvent, ExecutionStatus, FailureKind, MemoryCreate, MemoryKind, MessageRole, Task, TaskKind, TaskStatus};
use dots_engines::ExecutionRequest;
use serde_json::json;
use tokio::sync::mpsc;

use crate::gateway::gateway_config;
use crate::policy::{AuthorizedDir, ExecutionContext};
use crate::service::RuntimeService;

const MAX_MEMORY_CHARS: usize = 6000;
const MAX_ATTACHMENT_CHARS: usize = 40_000;

fn random_token() -> String {
    use rand::RngCore;
    let mut b = [0u8; 24];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

impl RuntimeService {
    /// Ponto de entrada do scheduler. Nunca retorna erro para o chamador: toda falha vira estado da tarefa.
    pub async fn run_task(self: Arc<Self>, task: Task) {
        let task_id = task.id.clone();
        if let Err(e) = self.clone().run_task_inner(task).await {
            tracing::error!(task = %task_id, error = %e, "falha ao executar tarefa");
            let (tid, msg) = (task_id.clone(), e.message().to_string());
            let target = match e {
                DotsError::Engine(_) | DotsError::Validation(_) | DotsError::NotFound(_) => TaskStatus::WaitingUser,
                _ => TaskStatus::Failed,
            };
            let _ = self.storage.call(move |s| s.tasks().set_status(&tid, target, Some(&msg))).await;
            self.events.emit("task.changed", json!({ "id": task_id, "status": target.as_str() }));
        }
        self.scheduler_notify.notify_one();
    }

    async fn run_task_inner(self: Arc<Self>, task: Task) -> DotsResult<()> {
        let dot_id = task.dot_id.clone();
        let dot = self.storage.call({ let d = dot_id.clone(); move |s| s.dots().get(&d) }).await?;
        let engine = task.engine.or(dot.primary_engine).ok_or_else(|| DotsError::Engine("o DOT nao tem motor principal definido".into()))?;
        let cfg = self.storage.call(move |s| s.engines().get(engine)).await?;
        let executable = cfg
            .as_ref()
            .and_then(|c| c.executable_override.clone().or_else(|| c.last_detection.as_ref().filter(|d| d.installed).and_then(|d| d.executable_path.clone())))
            .ok_or_else(|| DotsError::Engine(format!("motor {} nao detectado; verifique em Motores de IA", engine.display_name())))?;
        if let Some(c) = &cfg {
            if !c.enabled {
                return Err(DotsError::Engine(format!("motor {} esta desabilitado", engine.display_name())));
            }
        }
        // Modelo: o do DOT vale mais que o padrao do motor; sem nenhum, o CLI usa o proprio padrao.
        let model = dot.model.clone().or_else(|| cfg.as_ref().and_then(|c| c.model.clone()));
        let policy = self.storage.call({ let d = dot_id.clone(); move |s| s.policies().get(&d) }).await?;
        let workspaces = self.storage.call({ let d = dot_id.clone(); move |s| s.dots().workspaces(&d) }).await?;
        let workspace = PathBuf::from(dot.workspace_path.clone().ok_or_else(|| DotsError::Validation("o DOT nao tem workspace".into()))?);
        std::fs::create_dir_all(&workspace)?;
        let mut dirs: Vec<AuthorizedDir> = workspaces.iter().map(|w| AuthorizedDir { path: PathBuf::from(&w.path), can_read: w.can_read, can_write: w.can_write }).collect();
        if !dirs.iter().any(|d| d.path == workspace) {
            dirs.push(AuthorizedDir { path: workspace.clone(), can_read: true, can_write: true });
        }
        let extra_dirs: Vec<PathBuf> = dirs.iter().filter(|d| d.path != workspace && d.can_read).map(|d| d.path.clone()).collect();

        // Estado: RUNNING + execucao
        let t = self.storage.call({ let id = task.id.clone(); move |s| s.tasks().set_status(&id, TaskStatus::Running, None) }).await?;
        let execution = self
            .storage
            .call({ let (tid, did, ws) = (task.id.clone(), dot_id.clone(), workspace.to_string_lossy().to_string()); move |s| s.executions().create(&tid, &did, engine, t.attempts, Some(&ws)) })
            .await?;
        self.events.emit("task.changed", json!({ "id": task.id, "dot_id": dot_id, "status": "RUNNING", "execution_id": execution.id }));

        // Continuidade da conversa
        let resume_session = match &task.conversation_id {
            Some(cid) => self.storage.call({ let c = cid.clone(); move |s| s.conversations().engine_session(&c, engine) }).await.ok().flatten(),
            None => None,
        };
        let new_session_id = if engine == EngineKind::Claude && resume_session.is_none() { Some(uuid::Uuid::new_v4().to_string()) } else { None };

        // Prompt e instrucoes
        let attachments = self.storage.call({ let id = task.id.clone(); move |s| s.tasks().attachments(&id) }).await.unwrap_or_default();
        let memories = self.storage.call({ let (d, q) = (dot_id.clone(), task.prompt.clone()); move |s| s.memories().search(&d, &q, &[MemoryKind::LongTerm, MemoryKind::History], 12) }).await.unwrap_or_default();
        let working = self.storage.call({ let (d, src) = (dot_id.clone(), format!("task:{}", task.id)); move |s| Ok(s.memories().list(&d, Some(MemoryKind::Working), 50)?.into_iter().find(|m| m.source.as_deref() == Some(&src))) }).await.ok().flatten();
        let checkpoint = if t.attempts > 1 || task.origin.starts_with("followup:") {
            self.storage.call({ let id = if task.origin.starts_with("followup:") { task.origin.trim_start_matches("followup:").to_string() } else { task.id.clone() }; move |s| s.checkpoints().latest(&id) }).await.ok().flatten()
        } else {
            None
        };
        // DOT ligado ao Flexi recebe o mapa do Flexi (marketplaces, Google Shopping x Google Ads).
        let flexi_context = self.storage.call({ let d = dot_id.clone(); move |s| s.apps().integration(&d, crate::flexi::KIND) }).await.ok().flatten().map(|_| crate::flexi::FLEXI_CONTEXT);
        let system_instructions = build_instructions(&dot, &policy, &dirs, &memories, working.as_ref().map(|m| m.content.as_str()), resume_session.is_some(), checkpoint.as_ref(), t.attempts, flexi_context);
        // Checkpoint inicial da tentativa.
        let _ = self.storage.call({ let (tid, eid, att) = (task.id.clone(), execution.id.clone(), t.attempts); move |s| s.checkpoints().create(&tid, &eid, &format!("inicio da tentativa {att}"), &json!({}), "", "", &[], "", &[], "auto") }).await;
        let mut prompt = task.prompt.clone();
        let tmp_ctx = ExecutionContext {
            token: String::new(), execution_id: execution.id.clone(), task_id: task.id.clone(), dot_id: dot_id.clone(), dot_name: dot.name.clone(),
            workspace: workspace.clone(), dirs: dirs.clone(), policy: policy.clone(), events: mpsc::channel(1).0,
        };
        for a in &attachments {
            match tmp_ctx.authorize_path(a, false).and_then(|p| std::fs::read_to_string(&p).map_err(|e| DotsError::Io(e.to_string())).map(|c| (p, c))) {
                Ok((p, content)) => {
                    let c: String = content.chars().take(MAX_ATTACHMENT_CHARS).collect();
                    prompt.push_str(&format!("\n\n<attachment path=\"{}\">\n{}\n</attachment>", p.display(), c));
                }
                Err(e) => prompt.push_str(&format!("\n\n[anexo {a} indisponivel: {}]", e.message())),
            }
        }

        // Token de execucao (env + arquivo no workspace para CLIs que nao propagam ambiente)
        let token = random_token();
        let dots_dir = workspace.join(".dots");
        std::fs::create_dir_all(&dots_dir)?;
        let token_file = dots_dir.join("execution.token");
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let _ = std::fs::remove_file(&token_file);
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&token_file)?;
            f.write_all(token.as_bytes())?;
        }

        let (ev_tx, mut ev_rx) = mpsc::channel::<ExecutionEvent>(256);
        let ctx = Arc::new(ExecutionContext {
            token: token.clone(), execution_id: execution.id.clone(), task_id: task.id.clone(), dot_id: dot_id.clone(), dot_name: dot.name.clone(),
            workspace: workspace.clone(), dirs: dirs.clone(), policy: policy.clone(), events: ev_tx,
        });
        let req = ExecutionRequest {
            execution_id: execution.id.clone(),
            engine,
            executable: PathBuf::from(executable),
            model,
            workspace: workspace.clone(),
            extra_dirs,
            prompt,
            system_instructions,
            resume_session: resume_session.clone(),
            new_session_id,
            native_write: policy.engine_native_write,
            network_allowed: policy.allow_network,
            gateway: Some(gateway_config(&self.dotsd_path, &self.layout.root, &token)),
            timeout: Duration::from_secs(dot.limits.max_execution_seconds as u64),
            max_turns: Some(60),
            max_budget_usd: dot.limits.max_cost_usd_per_task,
            home: self.home.clone(),
            path_dirs: self.path_dirs.clone(),
            env: vec![("DOTS_EXECUTION_TOKEN".into(), token.clone()), ("DOTS_DATA_DIR".into(), self.layout.root.to_string_lossy().to_string())],
        };

        let (handle, mut stream) = match self.engines.execute(req).await {
            Ok(x) => x,
            Err(e) => {
                let _ = std::fs::remove_file(&token_file);
                let (eid, msg) = (execution.id.clone(), e.message().to_string());
                let _ = self.storage.call(move |s| s.executions().finish(&eid, ExecutionStatus::Failed, None, Some(&msg), None, &json!({}))).await;
                return Err(e);
            }
        };
        let pid = handle.pid;
        let _ = self.storage.call({ let eid = execution.id.clone(); move |s| s.executions().set_pid(&eid, pid) }).await;
        self.registry.register(ctx.clone(), handle);
        if task.cancel_requested {
            self.registry.cancel(&execution.id);
        }

        // Mensagem do DOT em streaming (conversas)
        let dot_message_id = match &task.conversation_id {
            Some(cid) => self
                .storage
                .call({ let (c, tid, eid) = (cid.clone(), task.id.clone(), execution.id.clone()); move |s| s.conversations().add_message(&c, MessageRole::Dot, "", Some(&tid), Some(&eid), json!({ "streaming": true })) })
                .await
                .ok()
                .map(|m| m.id),
            None => None,
        };

        // Consumo de eventos (motor + gateway)
        let mut text_acc = String::new();
        let mut final_text: Option<String> = None;
        let mut outcome: Option<(ExecutionStatus, Option<String>, FailureKind)> = None;
        let mut cost: Option<f64> = None;
        let mut usage = json!({});
        let mut tool_names: Vec<String> = vec![];
        let mut steps: Vec<String> = vec![];
        let mut done = false;
        while !done {
            let ev = tokio::select! {
                e = stream.recv() => match e { Some(e) => e, None => break },
                e = ev_rx.recv() => match e { Some(e) => e, None => continue },
            };
            let persist = !matches!(ev, ExecutionEvent::TextDelta { .. } | ExecutionEvent::Raw { .. });
            match &ev {
                ExecutionEvent::Started { engine_session_id: Some(sid) } => {
                    let (eid, s) = (execution.id.clone(), sid.clone());
                    let _ = self.storage.call(move |st| st.executions().set_session(&eid, &s)).await;
                    if let Some(cid) = &task.conversation_id {
                        let (c, s) = (cid.clone(), sid.clone());
                        let _ = self.storage.call(move |st| st.conversations().set_engine_session(&c, engine, Some(&s))).await;
                    }
                }
                ExecutionEvent::TextDelta { text } => text_acc.push_str(text),
                ExecutionEvent::Text { text } => {
                    text_acc = text.clone();
                    final_text = Some(text.clone());
                }
                ExecutionEvent::ToolCall { name, .. } => {
                    tool_names.push(name.clone());
                    steps.push(format!("ferramenta {name}"));
                }
                ExecutionEvent::Usage { cost_usd, .. } => {
                    cost = cost_usd.or(cost);
                }
                ExecutionEvent::Completed { result_text } => {
                    if !result_text.is_empty() {
                        final_text = Some(result_text.clone());
                    }
                    outcome = Some((ExecutionStatus::Completed, None, FailureKind::Engine));
                    done = true;
                }
                ExecutionEvent::Failed { kind, message } => {
                    let st = match kind {
                        FailureKind::Cancelled => ExecutionStatus::Cancelled,
                        FailureKind::Timeout => ExecutionStatus::Timeout,
                        _ => ExecutionStatus::Failed,
                    };
                    outcome = Some((st, Some(message.clone()), *kind));
                    done = true;
                }
                _ => {}
            }
            let seq = if persist {
                let (eid, e) = (execution.id.clone(), ev.clone());
                self.storage.call(move |st| st.executions().add_event(&eid, &e)).await.map(|r| r.seq).unwrap_or(0)
            } else {
                0
            };
            self.events.emit("execution.event", json!({ "execution_id": execution.id, "task_id": task.id, "dot_id": dot_id, "seq": seq, "event": ev }));
            if let (Some(mid), ExecutionEvent::TextDelta { .. } | ExecutionEvent::Text { .. }) = (&dot_message_id, &ev) {
                if text_acc.len() % 400 < 40 {
                    let (m, c) = (mid.clone(), text_acc.clone());
                    let _ = self.storage.call(move |st| st.conversations().update_message_content(&m, &c, None)).await;
                }
            }
            if matches!(ev, ExecutionEvent::ToolCall { .. } | ExecutionEvent::ToolResult { .. }) && steps.len() % 3 == 0 {
                let (d, src, content) = (dot_id.clone(), format!("task:{}", task.id), format!("Tarefa: {}\nEtapas ate agora: {}", task.title, steps.join("; ")));
                let _ = self.storage.call(move |st| st.memories().upsert_working(&d, &src, &content)).await;
            }
        }
        let (status, error, kind) = outcome.unwrap_or((ExecutionStatus::Failed, Some("motor encerrou sem resultado".into()), FailureKind::Engine));
        if let Some(sess) = resume_session.as_ref().filter(|_| matches!(kind, FailureKind::Engine) && status == ExecutionStatus::Failed) {
            // Retomada falhou: limpa a sessao para a proxima tentativa reconstruir o contexto.
            if let Some(cid) = &task.conversation_id {
                let c = cid.clone();
                let _ = self.storage.call(move |st| st.conversations().set_engine_session(&c, engine, None)).await;
                tracing::warn!(session = %sess, "sessao do motor descartada apos falha na retomada");
            }
        }

        // Finalizacao
        self.registry.unregister(&execution.id, &token);
        let _ = std::fs::remove_file(&token_file);
        let _ = self.storage.call({ let (eid, err, u) = (execution.id.clone(), error.clone(), std::mem::take(&mut usage)); move |st| st.executions().finish(&eid, status, None, err.as_deref(), cost, &u) }).await;
        let result_text = final_text.clone().unwrap_or_else(|| text_acc.clone());
        if let Some(mid) = &dot_message_id {
            let (m, c, meta) = (mid.clone(), if result_text.is_empty() { error.clone().unwrap_or_default() } else { result_text.clone() }, json!({ "streaming": false, "status": status.as_str(), "tools": tool_names, "error": error }));
            let _ = self.storage.call(move |st| st.conversations().update_message_content(&m, &c, Some(meta))).await;
            self.events.emit("message.changed", json!({ "conversation_id": task.conversation_id, "message_id": mid }));
        }
        let retryable = matches!(kind, FailureKind::Engine | FailureKind::Timeout) && status != ExecutionStatus::Completed && status != ExecutionStatus::Cancelled;
        let will_retry = retryable && task.retry_safe && t.attempts < task.max_attempts;
        let task_status = match status {
            ExecutionStatus::Completed => TaskStatus::Completed,
            ExecutionStatus::Cancelled => TaskStatus::Cancelled,
            _ if will_retry => TaskStatus::Queued,
            _ => TaskStatus::Failed,
        };
        if status == ExecutionStatus::Completed {
            let (tid, r) = (task.id.clone(), result_text.clone());
            let _ = self.storage.call(move |st| st.tasks().set_result(&tid, &r)).await;
        }
        let _ = self.storage.call({ let (tid, err) = (task.id.clone(), error.clone()); move |st| st.tasks().set_status(&tid, task_status, err.as_deref()) }).await;
        // Memoria: remove trabalho, grava historico
        let (d, src) = (dot_id.clone(), format!("task:{}", task.id));
        let _ = self.storage.call(move |st| {
            let working = st.memories().list(&d, Some(MemoryKind::Working), 100)?;
            for w in working.into_iter().filter(|w| w.source.as_deref() == Some(&src)) {
                let _ = st.memories().delete(&w.id);
            }
            Ok(())
        }).await;
        if task.kind == TaskKind::Task || status != ExecutionStatus::Completed {
            let summary: String = result_text.chars().take(600).collect();
            let content = format!("[{}] {} — {}{}", status.as_str(), task.title, if summary.is_empty() { error.clone().unwrap_or_default() } else { summary }, if tool_names.is_empty() { String::new() } else { format!(" (ferramentas: {})", tool_names.join(", ")) });
            let (d, src) = (dot_id.clone(), format!("task:{}", task.id));
            let _ = self.storage.call(move |st| {
                st.memories().create(MemoryCreate { dot_id: d.clone(), kind: MemoryKind::History, content, importance: Some(0), source: Some(src) })?;
                st.memories().prune(&d, MemoryKind::History, 200)
            }).await;
        }
        self.events.emit("task.changed", json!({ "id": task.id, "dot_id": dot_id, "status": task_status.as_str(), "execution_id": execution.id }));
        tracing::info!(task = %task.id, execution = %execution.id, status = status.as_str(), retry = will_retry, "tarefa finalizada");
        if will_retry {
            tracing::info!(task = %task.id, attempt = t.attempts, "nova tentativa a partir do ultimo checkpoint");
            self.scheduler_notify.notify_one();
            return Ok(());
        }
        // Eventos internos e fechamento de delegacoes.
        let depth = self.event_depths.lock().unwrap().remove(&task.id).unwrap_or(0);
        let (etype, key) = if task_status == TaskStatus::Completed { (et::TASK_COMPLETED, format!("task:{}:completed", task.id)) } else { (et::TASK_FAILED, format!("task:{}:{}", task.id, task_status.as_str())) };
        let _ = self.emit_event(etype, &format!("dot:{dot_id}"), json!({ "dot_id": dot_id, "dot_name": dot.name, "task_id": task.id, "title": task.title, "status": task_status.as_str(), "result": result_text.chars().take(2000).collect::<String>(), "error": error }), task.correlation_id.clone(), Some(key), depth).await;
        self.on_task_finished(&task, task_status, &result_text, error.as_deref()).await;
        Ok(())
    }
}

fn build_instructions(dot: &dots_core::Dot, policy: &dots_core::ToolPolicy, dirs: &[AuthorizedDir], memories: &[dots_core::Memory], working: Option<&str>, resumed: bool, checkpoint: Option<&dots_core::Checkpoint>, attempt: u32, app_context: Option<&str>) -> String {
    let mut s = String::new();
    s.push_str(&format!("Voce e \"{}\", um DOT (trabalhador digital persistente) na plataforma DOTS.\n", dot.name));
    if !dot.description.trim().is_empty() {
        s.push_str(&format!("Descricao: {}\n", dot.description.trim()));
    }
    if !dot.objective.trim().is_empty() {
        s.push_str(&format!("Objetivo: {}\n", dot.objective.trim()));
    }
    if !dot.instructions.trim().is_empty() {
        s.push_str(&format!("\nInstrucoes permanentes:\n{}\n", dot.instructions.trim()));
    }
    if let Some(c) = app_context {
        s.push_str(&format!("\n{}\n", c.trim()));
    }
    s.push_str("\nAmbiente e regras:\n");
    s.push_str(&format!("- Workspace principal: {} (caminhos relativos apontam para ca; use caminho absoluto para outras pastas autorizadas)\n", dot.workspace_path.as_deref().unwrap_or("?")));
    for d in dirs {
        s.push_str(&format!("- Diretorio autorizado: {} ({}{})\n", d.path.display(), if d.can_read { "leitura" } else { "" }, if d.can_write { "+escrita" } else { "" }));
    }
    s.push_str("- Use as ferramentas do servidor MCP `dots` (dots_read_file, dots_write_file, dots_list_dir, dots_search, dots_run_command, dots_git, dots_remember, dots_recall) para acessar arquivos, comandos e memoria. Elas sao autorizadas pelo runtime; uma negacao e definitiva: nao tente contornar.\n");
    if policy.allow_network {
        s.push_str("- Voce tem um navegador proprio (perfil isolado deste DOT, logins persistem): dots_browser_open abre uma URL e devolve a pagina em texto com [ref=N] por elemento.\n");
    }
    s.push_str(&format!("- Nivel de aprovacao automatica: {}. Acoes acima disso aguardam aprovacao do usuario (pode demorar).\n", policy.auto_approve_level.as_str()));
    if !policy.allowed_commands.is_empty() {
        s.push_str(&format!("- Comandos permitidos: {}\n", policy.allowed_commands.join(", ")));
    }
    s.push_str("- Ao concluir, responda com um resumo objetivo do que foi feito e do que ficou pendente.\n");
    s.push_str("- Em trabalhos longos, grave pontos de controle com dots_checkpoint apos cada etapa concluida.\n");
    if resumed {
        s.push_str("- Esta e uma continuacao da conversa anterior.\n");
    }
    if dot.role == DotRole::Supervisor {
        s.push_str("\nVoce e um SUPERVISOR: divide objetivos em pedidos claros, delega a DOTs da equipe (dots_list_team, dots_delegate), acompanha (dots_delegation_status), identifica bloqueios e consolida as entregas. Delegacoes sao assincronas: apos delegar, termine o turno dizendo o que aguarda; os resultados chegam em uma tarefa de continuacao. Perguntas dos seus funcionarios sobre pedidos delegados chegam a voce como tarefas: responda com dots_answer_question. Use dots_ask_user so quando uma decisao for do usuario. Voce nao pode alterar permissoes de ninguem.\n");
    } else if !policy.can_delegate_to.is_empty() {
        s.push_str("- Voce pode delegar pedidos a outros DOTs (dots_list_team, dots_delegate). Delegacoes sao assincronas.\n");
    }
    if let Some(cp) = checkpoint {
        s.push_str(&format!("\nRetomada (tentativa {attempt}). Ultimo ponto de controle:\n- Etapa: {}\n- Resultados: {}\n- Proxima acao: {}\n- Arquivos produzidos: {}\nNao repita etapas ja concluidas; o runtime devolve o resultado gravado se uma acao identica for repetida.\n", cp.step, if cp.results.is_empty() { "(nenhum)".into() } else { cp.results.chars().take(1500).collect::<String>() }, if cp.next_action.is_empty() { "(nao informada)".into() } else { cp.next_action.clone() }, if cp.artifacts.is_empty() { "(nenhum)".into() } else { cp.artifacts.join(", ") }));
    }
    if let Some(w) = working {
        s.push_str(&format!("\nEstado de trabalho desta tarefa:\n{w}\n"));
    }
    if !memories.is_empty() {
        s.push_str("\nMemorias relevantes:\n");
        let mut used = 0usize;
        for m in memories {
            let line = format!("- [{}] {}\n", m.kind.as_str(), m.content.trim());
            if used + line.len() > MAX_MEMORY_CHARS {
                break;
            }
            used += line.len();
            s.push_str(&line);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_are_bounded_and_include_identity() {
        let dot = dots_core::Dot { id: "d".into(), name: "QA".into(), description: "testa".into(), objective: "qualidade".into(), instructions: "seja rigoroso".into(), primary_engine: None, model: None, workspace_path: Some("/ws".into()), limits: Default::default(), status: dots_core::DotStatus::Active, role: DotRole::Worker, created_at: dots_core::now(), updated_at: dots_core::now() };
        let policy = dots_core::ToolPolicy::default_for("d");
        let mems: Vec<dots_core::Memory> = (0..100).map(|i| dots_core::Memory { id: i.to_string(), dot_id: "d".into(), kind: MemoryKind::LongTerm, content: "x".repeat(200), importance: 0, source: None, created_at: dots_core::now(), updated_at: dots_core::now() }).collect();
        let s = build_instructions(&dot, &policy, &[], &mems, Some("etapa 2"), true, None, 1, Some("Mapa do app: teste"));
        assert!(s.contains("Mapa do app: teste"));
        assert!(s.contains("\"QA\"") && s.contains("seja rigoroso") && s.contains("etapa 2") && s.contains("continuacao"));
        assert!(s.len() < MAX_MEMORY_CHARS + 2000);
    }
}
