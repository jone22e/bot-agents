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
/// Limite das licoes de demissao nas instrucoes (as mais recentes primeiro).
const MAX_LESSON_CHARS: usize = 2500;
const MAX_ATTACHMENT_CHARS: usize = 40_000;
/// Tamanho de contexto (tokens na ultima chamada ao modelo) a partir do qual a sessao do motor e
/// encerrada ao fim da tarefa: a proxima tarefa da conversa comeca com sessao nova, so com memorias.
/// Evita pagar um historico gigante a cada turno; a continuidade vem de memorias, historico e checkpoints.
const SESSION_ROTATE_CONTEXT_TOKENS: u64 = 100_000;
/// Aviso gravado quando o motor conclui o turno sem o gateway MCP `dots` ter sido iniciado.
const GATEWAY_ABSENT_NOTICE: &str = "O motor concluiu o turno sem o servidor MCP dots: nenhuma ferramenta dots_*/flexi_* esteve disponivel. Verifique a carga da maquina e o log do runtime.";

/// Decide se a sessao do motor deve ser encerrada apos a tarefa: so quando concluiu e o contexto medido passou do limite.
fn should_rotate_session(status: ExecutionStatus, context_tokens: Option<u64>) -> bool {
    status == ExecutionStatus::Completed && context_tokens.is_some_and(|n| n >= SESSION_ROTATE_CONTEXT_TOKENS)
}

fn is_thread_store_conflict(error: &str) -> bool {
    error.contains("thread-store conflict") || error.contains("already has an active writer")
}

/// Uma recusa do escritor antes de qualquer atividade pode ser refeita em sessao nova
/// sem repetir acoes do DOT. Limita a recuperacao a uma tentativa extra.
fn retry_preflight_thread_conflict(engine: EngineKind, resumed: bool, started: bool, acted: bool, error: Option<&str>, attempt: u32, max_attempts: u32) -> bool {
    engine == EngineKind::Codex && resumed && !started && !acted && attempt <= max_attempts
        && error.is_some_and(is_thread_store_conflict)
}

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

        // O scheduler reservou a tarefa antes de despacha-la.
        let t = task.clone();
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
        // Licoes de demissoes na equipe (como funcionario ou como gerente): entram nas instrucoes para ninguem repetir o erro.
        let lessons = self.storage.call({ let d = dot_id.clone(); move |s| {
            let mut out: Vec<dots_core::Dismissal> = vec![];
            if let Some(team) = s.teams().member_of(&d)? { out.extend(s.dismissals().for_team(&team.id, 20)?); }
            if let Some(team) = s.teams().managed_by(&d)? { for x in s.dismissals().for_team(&team.id, 20)? { if !out.iter().any(|o| o.id == x.id) { out.push(x); } } }
            Ok(out)
        } }).await.unwrap_or_default();
        // Gerente: quantos funcionarios tem e o limite do usuario (para contratar quando faltar quem faca).
        let hiring = if dot.role == DotRole::Supervisor {
            let members = self.storage.call({ let d = dot_id.clone(); move |s| Ok(s.teams().managed_by(&d)?.map(|t| t.member_ids.len()).unwrap_or(0)) }).await.unwrap_or(0);
            Some((members, self.max_employees_per_manager().await))
        } else {
            None
        };
        let checkpoint = if t.attempts > 1 || task.origin.starts_with("followup:") {
            self.storage.call({ let id = if task.origin.starts_with("followup:") { task.origin.trim_start_matches("followup:").to_string() } else { task.id.clone() }; move |s| s.checkpoints().latest(&id) }).await.ok().flatten()
        } else {
            None
        };
        // DOT ligado ao Flexi recebe o mapa do Flexi (marketplaces, Google Shopping x Google Ads) e o navegador abre no Flexi logado.
        let flexi_conn = self.flexi_connection_for_dot(&dot_id).await.ok().flatten();
        let flexi_context = flexi_conn.as_ref().map(|_| crate::flexi::FLEXI_CONTEXT);
        let app_hosts: Vec<String> = flexi_conn.iter().map(|c| crate::policy::url_host(&c.base_url)).filter(|h| !h.is_empty()).collect();
        let mut effective_dot = dot.clone();
        let integration = self.storage.call({ let d = dot_id.clone(); move |s| s.apps().integration(&d, crate::flexi::KIND) }).await?;
        if let Some(integration) = integration.filter(|value| value.role_template.is_some()) {
            let role_template = integration.role_template.as_deref().unwrap_or_default();
            match self.flexi_fetch_templates(Some(&integration.connection_id)).await {
                Ok(templates) => {
                    effective_dot.instructions = crate::flexi_team::instructions_for_template(&templates, role_template)?;
                }
                Err(error) if crate::flexi_team::builtin_templates().iter().any(|template|
                    template.id == role_template && template.purpose == dot.instructions
                ) => {
                    tracing::warn!(role_template, error = %error, "Flexi antigo: usando instrucoes embutidas originais");
                }
                Err(error) => return Err(error),
            }
        }
        // Instrucoes estaveis por DOT vao no system prompt (iguais entre turnos: preservam o cache do CLI);
        // o que muda a cada tarefa (memorias, checkpoint, estado de trabalho) vai no prompt da tarefa.
        // Funcionario de equipe nao avisa o usuario (dots_notify_user): relata ao gerente.
        let manager_name = self.storage.call({ let d = dot_id.clone(); move |s| {
            let Some(team) = s.teams().member_of(&d)? else { return Ok(None) };
            let Some(mid) = team.manager_dot_id else { return Ok(None) };
            Ok(Some(s.dots().get(&mid)?.name))
        } }).await.ok().flatten();
        let system_instructions = build_instructions(&effective_dot, &policy, &dirs, flexi_context, &app_hosts, &lessons, hiring, manager_name.as_deref());
        // Conversa com turnos anteriores do DOT mas sem sessao do motor: a sessao foi encerrada (por tamanho ou falha).
        let session_reset = resume_session.is_none()
            && match &task.conversation_id {
                Some(cid) => self.storage.call({ let c = cid.clone(); move |s| s.conversations().messages(&c, 0) }).await.map(|ms| ms.iter().any(|m| m.role == MessageRole::Dot)).unwrap_or(false),
                None => false,
            };
        let task_context = build_task_context(&memories, working.as_ref().map(|m| m.content.as_str()), resume_session.is_some(), session_reset, checkpoint.as_ref(), t.attempts);
        // Checkpoint inicial da tentativa.
        let _ = self.storage.call({ let (tid, eid, att) = (task.id.clone(), execution.id.clone(), t.attempts); move |s| s.checkpoints().create(&tid, &eid, &format!("inicio da tentativa {att}"), &json!({}), "", "", &[], "", &[], "auto") }).await;
        // O pedido do usuario vem primeiro; o contexto entra num bloco logo depois (antes dos anexos).
        let mut prompt = if task_context.is_empty() { task.prompt.clone() } else { format!("{}\n\n<contexto_da_tarefa>\n{task_context}</contexto_da_tarefa>", task.prompt) };
        let tmp_ctx = ExecutionContext {
            token: String::new(), execution_id: execution.id.clone(), task_id: task.id.clone(), dot_id: dot_id.clone(), dot_name: dot.name.clone(),
            workspace: workspace.clone(), dirs: dirs.clone(), policy: policy.clone(), events: mpsc::channel(1).0, app_hosts: app_hosts.clone(),
            gateway_seen: Default::default(),
        };
        for a in &attachments {
            match tmp_ctx.authorize_path(a, false).and_then(|p| std::fs::read(&p).map_err(|e| DotsError::Io(e.to_string())).map(|c| (p, c))) {
                Ok((p, bytes)) => match String::from_utf8(bytes) {
                    Ok(content) => {
                        let c: String = content.chars().take(MAX_ATTACHMENT_CHARS).collect();
                        prompt.push_str(&format!("\n\n<attachment path=\"{}\">\n{}\n</attachment>", p.display(), c));
                    }
                    // Imagem, PDF, planilha...: o motor abre pelo caminho com as proprias ferramentas de leitura.
                    Err(_) => prompt.push_str(&format!("\n\n<attachment path=\"{}\" binary=\"true\">Arquivo binario anexado pelo usuario (imagem, PDF ou similar). Abra pelo caminho para ver o conteudo.</attachment>", p.display())),
                },
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
            workspace: workspace.clone(), dirs: dirs.clone(), policy: policy.clone(), events: ev_tx, app_hosts: app_hosts.clone(),
            gateway_seen: Default::default(),
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

        // O CLI pode chamar o gateway logo ao iniciar; o token precisa existir antes do spawn.
        self.registry.register_context(ctx.clone());
        let (handle, mut stream) = match self.engines.execute(req).await {
            Ok(x) => x,
            Err(e) => {
                self.registry.unregister(&execution.id, &token);
                let _ = std::fs::remove_file(&token_file);
                let (eid, msg) = (execution.id.clone(), e.message().to_string());
                let _ = self.storage.call(move |s| s.executions().finish(&eid, ExecutionStatus::Failed, None, Some(&msg), None, &json!({}))).await;
                return Err(e);
            }
        };
        let pid = handle.pid;
        self.registry.register_handle(&execution.id, handle);
        let _ = self.storage.call({ let eid = execution.id.clone(); move |s| s.executions().set_pid(&eid, pid) }).await;
        let cancel_requested = self.storage.call({ let id = task.id.clone(); move |s| Ok(s.tasks().get(&id)?.cancel_requested) }).await.unwrap_or(false);
        if cancel_requested {
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
        let mut last_context_tokens: Option<u64> = None;
        let mut tool_names: Vec<String> = vec![];
        let mut steps: Vec<String> = vec![];
        let mut engine_started = false;
        let mut done = false;
        while !done {
            let ev = tokio::select! {
                e = stream.recv() => match e { Some(e) => e, None => break },
                e = ev_rx.recv() => match e { Some(e) => e, None => continue },
            };
            let persist = !matches!(ev, ExecutionEvent::TextDelta { .. } | ExecutionEvent::Raw { .. });
            match &ev {
                ExecutionEvent::Started { engine_session_id: Some(sid) } => {
                    engine_started = true;
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
                ExecutionEvent::Usage { input_tokens, output_tokens, cost_usd, context_tokens } => {
                    cost = cost_usd.or(cost);
                    last_context_tokens = context_tokens.or(last_context_tokens);
                    usage = json!({ "input_tokens": input_tokens, "output_tokens": output_tokens, "context_tokens": last_context_tokens });
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
        // Eventos terminais podem chegar antes de o CLI sair. Mantenha a conversa ocupada
        // ate o processo liberar o escritor da thread persistida.
        while stream.recv().await.is_some() {}
        let (status, error, kind) = outcome.unwrap_or((ExecutionStatus::Failed, Some("motor encerrou sem resultado".into()), FailureKind::Engine));
        // Turno concluido sem o gateway ter falado com o runtime: o CLI descartou o servidor MCP `dots` (ex.: prazo de
        // inicializacao) e o modelo respondeu sem nenhuma ferramenta. Fica registrado no evento e na mensagem.
        let gateway_absent = status == ExecutionStatus::Completed && !ctx.gateway_seen.load(std::sync::atomic::Ordering::SeqCst);
        if gateway_absent {
            tracing::warn!(execution = %execution.id, dot = %dot.name, ?engine, "turno concluido sem o gateway MCP dots: o motor rodou sem as ferramentas do DOTS");
            let notice = ExecutionEvent::Notice { level: "warn".into(), text: GATEWAY_ABSENT_NOTICE.into() };
            let (eid, e) = (execution.id.clone(), notice.clone());
            let seq = self.storage.call(move |st| st.executions().add_event(&eid, &e)).await.map(|r| r.seq).unwrap_or(0);
            self.events.emit("execution.event", json!({ "execution_id": execution.id, "task_id": task.id, "dot_id": dot_id, "seq": seq, "event": notice }));
        }
        if let Some(sess) = resume_session.as_ref().filter(|_| status == ExecutionStatus::Failed && (matches!(kind, FailureKind::Engine) || error.as_deref().is_some_and(is_thread_store_conflict))) {
            // Retomada falhou: limpa a sessao para a proxima tentativa reconstruir o contexto.
            if let Some(cid) = &task.conversation_id {
                let c = cid.clone();
                let _ = self.storage.call(move |st| st.conversations().set_engine_session(&c, engine, None)).await;
                tracing::warn!(session = %sess, "sessao do motor descartada apos falha na retomada");
            }
        } else if should_rotate_session(status, last_context_tokens) {
            // Contexto grande demais: encerra a sessao; a proxima tarefa da conversa recomeca so com memorias.
            if let Some(cid) = &task.conversation_id {
                let c = cid.clone();
                let _ = self.storage.call(move |st| st.conversations().set_engine_session(&c, engine, None)).await;
                tracing::info!(conversation = %cid, context_tokens = last_context_tokens.unwrap_or(0), "sessao do motor encerrada por tamanho do contexto");
            }
        }

        // Finalizacao
        self.registry.unregister(&execution.id, &token);
        let _ = std::fs::remove_file(&token_file);
        let _ = self.storage.call({ let (eid, err, u) = (execution.id.clone(), error.clone(), std::mem::take(&mut usage)); move |st| st.executions().finish(&eid, status, None, err.as_deref(), cost, &u) }).await;
        let result_text = final_text.clone().unwrap_or_else(|| text_acc.clone());
        let safe_conflict_retry = status == ExecutionStatus::Failed && retry_preflight_thread_conflict(
            engine, resume_session.is_some(), engine_started,
            !tool_names.is_empty() || !result_text.is_empty(), error.as_deref(), t.attempts, task.max_attempts,
        );
        let retryable = matches!(kind, FailureKind::Engine | FailureKind::Timeout) && status != ExecutionStatus::Completed && status != ExecutionStatus::Cancelled;
        let will_retry = (retryable && task.retry_safe && t.attempts < task.max_attempts) || safe_conflict_retry;
        if let Some(mid) = &dot_message_id {
            let content = if safe_conflict_retry { "Sessao do Codex ocupada; retomando em uma sessao nova.".to_string() } else if result_text.is_empty() { error.clone().unwrap_or_default() } else { result_text.clone() };
            let (m, c, meta) = (mid.clone(), content, json!({ "streaming": false, "status": status.as_str(), "tools": tool_names, "error": if safe_conflict_retry { None } else { error.as_deref() }, "gateway_absent": gateway_absent }));
            let _ = self.storage.call(move |st| st.conversations().update_message_content(&m, &c, Some(meta))).await;
            self.events.emit("message.changed", json!({ "conversation_id": task.conversation_id, "message_id": mid }));
        }
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

/// Instrucoes estaveis do DOT (identidade, regras, papel, licoes). Nao dependem da tarefa: ficam iguais entre
/// turnos da mesma conversa, o que preserva o cache de prompt do CLI em sessoes retomadas.
fn build_instructions(dot: &dots_core::Dot, policy: &dots_core::ToolPolicy, dirs: &[AuthorizedDir], app_context: Option<&str>, browser_hosts: &[String], lessons: &[dots_core::Dismissal], hiring: Option<(usize, u32)>, manager: Option<&str>) -> String {
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
    s.push_str("- Ao chamar uma ferramenta que pode pedir aprovacao, inclua _dots_approval_assessment com confidence_percent (inteiro de 0 a 100: sua estimativa de a acao atingir o resultado esperado, nao chance de aprovacao) e reversible (true somente se souber como desfazer os efeitos). Se nao puder avaliar honestamente os dois, omita o campo. Essa avaliacao e informativa e nao muda a politica de permissao.\n");
    s.push_str("- Voce so trabalha quando chega uma tarefa: passado um horario, nada te acorda sozinho. Para fazer algo depois ou de tempos em tempos (rodadas, relatorios por hora, \"proxima rodada as HH:MM\"), crie um agendamento com dots_schedule_create; no horario o runtime cria a tarefa com o pedido. NUNCA prometa um horario futuro sem ter agendado. Antes de agendar, confira dots_schedule_list para nao duplicar; cancele com dots_schedule_cancel o que nao for mais preciso. Diga ao usuario o que agendou e quando.\n");
    if policy.allow_network || !browser_hosts.is_empty() {
        s.push_str("- Voce tem um navegador proprio (perfil isolado deste DOT, logins persistem): dots_browser_open abre uma URL e devolve a pagina em texto com [ref=N] por elemento; para agir nela use dots_browser_click (clicar), dots_browser_type (digitar), dots_browser_select (escolher opcao), dots_browser_press (teclas, e Enter/Escape em dialogos do navegador) e dots_browser_read (ler de novo). Cada acao devolve a pagina atualizada com refs novos. E o UNICO navegador autorizado: nao use ferramentas de computador ou navegador do proprio assistente (cua, computer use, browser use); elas sao negadas e o usuario nao ve nada. O que voce abre com dots_browser_open o usuario acompanha ao vivo no painel Computador. Use as ferramentas diretamente, sem perguntar antes com dots_ask_user: o runtime pede a aprovacao necessaria. Cliques que gravam ou sao irreversiveis (Salvar, Excluir, Publicar...) passam pela aprovacao do proprio runtime.");
        if !browser_hosts.is_empty() {
            s.push_str(&format!(" Apps conectados que ja abrem logados: {}.", browser_hosts.join(", ")));
        }
        if !policy.allow_network {
            s.push_str(" Para sites externos, use dots_browser_open: o DOTS pedira permissao ao usuario para o dominio antes de abrir. Se o usuario negar, nao tente contornar.");
        }
        s.push('\n');
    }
    s.push_str(&format!("- Nivel de aprovacao automatica: {}. Acoes acima disso aguardam aprovacao do usuario (pode demorar).\n", policy.auto_approve_level.as_str()));
    if !policy.allowed_commands.is_empty() {
        s.push_str(&format!("- Comandos permitidos: {}\n", policy.allowed_commands.join(", ")));
    }
    s.push_str("- Ao concluir, responda com um resumo objetivo do que foi feito e do que ficou pendente.\n");
    match (dot.role == DotRole::Supervisor, manager) {
        (true, _) | (false, None) => s.push_str("- Quando o usuario declarar uma META (um numero a atingir, com ou sem prazo), registre na hora com dots_goal_create e confirme o que registrou. Se a meta exigir acompanhamento futuro, confira dots_schedule_list e crie com dots_schedule_create uma rodada de verificacao na frequencia que o usuario definiu; inclua o id da meta no pedido. Se nao houver frequencia definida, pergunte antes de agendar. Ao fim de cada rodada ou entrega, lance o progresso com dots_goal_progress (valor acumulado e nota curta). Veja dots_goal_list antes de criar para nao duplicar; se o usuario mudar o alvo, o prazo ou o nome de uma meta, altere a existente com dots_goal_update (nunca crie outra nem cancele a atual para isso); encerre com dots_goal_close. O usuario acompanha a meta pelo Flexi: numeros nao lancados nao existem para ele.\n"),
        (false, Some(m)) => s.push_str(&format!("- Metas sao do gerente: NAO use dots_goal_*. Relate os numeros da sua entrega no resultado da tarefa para {m} lancar o progresso.\n")),
    }
    match manager {
        Some(m) => s.push_str(&format!("- So o gerente avisa o usuario: NAO use dots_notify_user. Se algo precisar de atencao (erro que te impede de continuar, pendencia, resultado importante), relate no resultado da tarefa: ele chega ao seu gerente {m}, que decide se avisa o usuario. Para decisoes, pergunte com dots_ask_user.\n")),
        None => s.push_str("- Se algo precisar da atencao do usuario (erro que te impede de continuar, pendencia que so ele resolve, resultado importante), avise com dots_notify_user: o aviso fica no topo da tela ate ele clicar. Uma vez por assunto; nao repita. Para decisoes dele, pergunte com dots_ask_user.\n"),
    }
    s.push_str("- Em trabalhos longos, grave pontos de controle com dots_checkpoint apos cada etapa concluida.\n");
    if dot.role == DotRole::Supervisor {
        s.push_str("\nVoce e um SUPERVISOR (gerente): divide objetivos em pedidos claros, delega a DOTs da equipe (dots_list_team, dots_delegate), acompanha (dots_delegation_status), identifica bloqueios e consolida as entregas. Voce NUNCA executa o trabalho da equipe, mesmo sem nenhum funcionario: so consulta, delega, acompanha e responde; acoes que alteram algo sao negadas para voce. Se nao houver funcionario para a tarefa (ou todos estiverem ocupados com outra coisa), contrate um com dots_hire_employee: voce cria o nome (unico, de pessoa) e define a funcao e as instrucoes; depois delegue a ele. Delegacoes sao assincronas: apos delegar, termine o turno dizendo o que aguarda; cada entrega gera uma tarefa de acompanhamento para voce e, ao fim, outra de consolidacao. Ao receber cada acompanhamento, avalie o resultado e o estado da equipe por iniciativa propria, corrija bloqueios e oriente quem ainda trabalha, sem esperar o usuario cobrar. Perguntas e pedidos de permissao dos seus funcionarios sobre pedidos delegados chegam a voce como tarefas: responda com dots_answer_question e decida com dots_decide_approval (aprove o que esta dentro do pedido e do que o usuario autorizou; acoes irreversiveis que o usuario nao autorizou, confirme antes com ele). Use dots_ask_user so quando uma decisao for do usuario. Se um funcionario esta PARADO (interrompido ou com falha), retome com dots_resume_delegation ou delegue de novo com instrucoes melhores, em vez de so relatar que ele nao esta trabalhando. Voce nao pode alterar permissoes de ninguem. Voce pode pausar um funcionario da equipe (dots_pause_employee: ele para de receber trabalho ate ser retomado) e retoma-lo (dots_resume_employee), sem aprovacao do usuario. Se um funcionario errar de forma grave ou repetir o mesmo erro depois de instrucoes melhores, voce pode demiti-lo com dots_dismiss_employee (informe o motivo e a licao); isso passa pela aprovacao do usuario, EXCLUI o funcionario e registra a licao para toda a equipe; em seguida contrate um substituto com dots_hire_employee e delegue a ele de novo.\n");
        if let Some((members, limit)) = hiring {
            s.push_str(&format!("- Sua equipe tem {members} funcionario(s); o limite definido pelo usuario e {limit} por gerente.\n"));
        }
    } else if !policy.can_delegate_to.is_empty() {
        s.push_str("- Voce pode delegar pedidos a outros DOTs (dots_list_team, dots_delegate). Delegacoes sao assincronas.\n");
    }
    if !lessons.is_empty() {
        s.push_str(if dot.role == DotRole::Supervisor {
            "\nDemissoes na sua equipe (funcionarios que voce demitiu; cobre que os erros nao se repitam):\n"
        } else {
            "\nLicoes de demissoes na sua equipe (funcionarios anteriores demitidos pelo gerente; NAO repita estes erros):\n"
        });
        let mut used = 0usize;
        for l in lessons {
            let line = format!("- {} ({}): demitido por {} em {}. Motivo: {}. Licao: {}\n", l.dot_name, l.team_name, l.manager_name, l.created_at.format("%d/%m/%Y"), l.reason.trim(), l.lesson.trim());
            if used + line.len() > MAX_LESSON_CHARS {
                break;
            }
            used += line.len();
            s.push_str(&line);
        }
    }
    s
}

/// Contexto que muda a cada tarefa: retomada, checkpoint, estado de trabalho e memorias relevantes.
/// Vai no prompt da tarefa (nao no system prompt) para nao invalidar o cache da conversa.
fn build_task_context(memories: &[dots_core::Memory], working: Option<&str>, resumed: bool, session_reset: bool, checkpoint: Option<&dots_core::Checkpoint>, attempt: u32) -> String {
    let mut s = String::new();
    if resumed {
        s.push_str("Esta e uma continuacao da conversa anterior.\n");
    } else if session_reset {
        s.push_str("Esta conversa continua de antes, mas a sessao do motor foi reiniciada: o historico detalhado nao esta neste contexto. O que foi feito esta nas memorias abaixo e em dots_recall; se faltar algo, pergunte ao usuario em vez de supor.\n");
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
    fn retries_only_preflight_codex_writer_conflict() {
        let error = "failed to initialize thread persistence: thread-store conflict: thread 01a178a3-88fd-70e3-8cbe-5b04015c3e already has an active writer";
        assert!(retry_preflight_thread_conflict(EngineKind::Codex, true, false, false, Some(error), 1, 1));
        assert!(!retry_preflight_thread_conflict(EngineKind::Codex, true, true, false, Some(error), 1, 1));
        assert!(!retry_preflight_thread_conflict(EngineKind::Codex, true, false, true, Some(error), 1, 1));
        assert!(!retry_preflight_thread_conflict(EngineKind::Codex, false, false, false, Some(error), 1, 1));
        assert!(!retry_preflight_thread_conflict(EngineKind::Codex, true, false, false, Some(error), 2, 1));
        assert!(!retry_preflight_thread_conflict(EngineKind::Codex, true, false, false, Some("authentication failed"), 1, 1));
    }

    #[test]
    fn instructions_are_bounded_and_include_identity() {
        let dot = dots_core::Dot { id: "d".into(), name: "QA".into(), description: "testa".into(), objective: "qualidade".into(), instructions: "seja rigoroso".into(), primary_engine: None, model: None, workspace_path: Some("/ws".into()), limits: Default::default(), status: dots_core::DotStatus::Active, role: DotRole::Worker, flexi_share: Default::default(), created_at: dots_core::now(), updated_at: dots_core::now() };
        let policy = dots_core::ToolPolicy::default_for("d");
        let mems: Vec<dots_core::Memory> = (0..100).map(|i| dots_core::Memory { id: i.to_string(), dot_id: "d".into(), kind: MemoryKind::LongTerm, content: "x".repeat(200), importance: 0, source: None, created_at: dots_core::now(), updated_at: dots_core::now() }).collect();
        let lesson = dots_core::Dismissal { id: "x".into(), team_id: Some("t".into()), team_name: "Mercado Livre".into(), manager_dot_id: Some("m".into()), manager_name: "Milei".into(), dot_id: Some("d".into()), dot_name: "Lula".into(), reason: "publicou anuncio sem revisar o preco".into(), lesson: "revisar o preco antes de publicar".into(), memories_wiped: 3, created_at: dots_core::now() };
        let s = build_instructions(&dot, &policy, &[], Some("Mapa do app: teste"), &["flexi.exemplo.com".to_string()], &[lesson], None, None);
        assert!(s.contains("avise com dots_notify_user"), "DOT sem gerente avisa o usuario");
        let e = build_instructions(&dot, &policy, &[], None, &[], &[], None, Some("Milei"));
        assert!(e.contains("NAO use dots_notify_user") && e.contains("seu gerente Milei") && !e.contains("avise com dots_notify_user"), "funcionario relata ao gerente em vez de avisar o usuario");
        assert!(s.contains("registre na hora com dots_goal_create"), "DOT sem gerente registra metas");
        assert!(e.contains("NAO use dots_goal_*") && !e.contains("dots_goal_create"), "funcionario nao registra metas");
        let gerente = dots_core::Dot { role: DotRole::Supervisor, ..dot.clone() };
        let nested = build_instructions(&gerente, &policy, &[], None, &[], &[], Some((0, 3)), Some("Milei"));
        assert!(nested.contains("registre na hora com dots_goal_create"), "gerente de subequipe registra as metas da propria equipe");
        assert!(s.contains("NAO repita estes erros") && s.contains("revisar o preco antes de publicar") && s.contains("Milei"), "licoes de demissao entram nas instrucoes");
        assert!(s.contains("Mapa do app: teste"));
        assert!(s.contains("dots_browser_open") && s.contains("flexi.exemplo.com"), "navegador aparece por causa do app conectado");
        assert!(s.contains("Use as ferramentas diretamente, sem perguntar antes com dots_ask_user: o runtime pede a aprovacao necessaria") && s.contains("dots_browser_click"), "navegador usa a aprovacao do runtime, com as acoes");
        assert!(s.contains("o DOTS pedira permissao ao usuario para o dominio antes de abrir"), "site externo exige permissao por dominio");
        assert!(s.contains("\"QA\"") && s.contains("seja rigoroso"));
        assert!(!s.contains("Memorias relevantes") && !s.contains("continuacao"), "system prompt nao carrega nada que mude entre turnos");
        assert!(s.len() < 6000);

        let ctx = build_task_context(&mems, Some("etapa 2"), true, false, None, 1);
        assert!(ctx.contains("etapa 2") && ctx.contains("continuacao") && ctx.contains("Memorias relevantes"));
        assert!(ctx.len() < MAX_MEMORY_CHARS + 1000, "memorias continuam limitadas");
        let reset = build_task_context(&[], None, false, true, None, 1);
        assert!(reset.contains("sessao do motor foi reiniciada") && !reset.contains("continuacao da conversa"));
        assert!(build_task_context(&[], None, false, false, None, 1).is_empty(), "primeira tarefa sem contexto extra nao ganha bloco vazio");
    }

    #[test]
    fn session_rotates_only_when_completed_and_context_is_large() {
        assert!(should_rotate_session(ExecutionStatus::Completed, Some(SESSION_ROTATE_CONTEXT_TOKENS)));
        assert!(!should_rotate_session(ExecutionStatus::Completed, Some(SESSION_ROTATE_CONTEXT_TOKENS - 1)));
        assert!(!should_rotate_session(ExecutionStatus::Completed, None), "motor sem medida de contexto nunca rotaciona");
        assert!(!should_rotate_session(ExecutionStatus::Failed, Some(u64::MAX)), "falha segue o fluxo de retomada/limpeza ja existente");
    }
}
