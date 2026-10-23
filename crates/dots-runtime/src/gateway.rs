//! Servico do gateway de ferramentas: autoriza, aprova, executa e audita.

use std::time::Duration;

use dots_core::automation::event_types as et;
use dots_core::{ApprovalKind, DotRole, DotStatus, DotsError, DotsResult, ExecutionEvent, MemoryCreate, MemoryKind, RiskLevel, TaskStatus};
use serde_json::{Value, json};

use crate::policy::{Decision, SharedContext};
use crate::service::RuntimeService;
use crate::tools::{self, ToolOutput};

pub const APPROVAL_TTL_SECONDS: i64 = 600;

/// Acoes na pagina ja aberta do navegador do DOT (a politica precisa da pagina e do elemento).
const BROWSER_ACTIONS: &[&str] = &["dots_browser_click", "dots_browser_type", "dots_browser_press", "dots_browser_select", "dots_browser_read"];

/// Ferramentas de gestao que o gerente usa mesmo acima de leitura (o resto acima de leitura e trabalho da equipe).
const MANAGER_TOOLS: &[&str] = &["dots_remember", "dots_hire_employee", "dots_dismiss_employee"];

/// `ref` de um elemento (numero, ou texto com o numero: alguns modelos mandam "12" ou "ref=12").
fn browser_ref(args: &Value) -> Option<u64> {
    let v = args.get("ref")?;
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().trim_start_matches("ref=").trim_start_matches('[').trim_end_matches(']').parse().ok()))
}

/// Metadados declarados pelo agente nao fazem parte dos parametros da ferramenta executada.
fn take_approval_assessment(args: &mut Value) -> DotsResult<Option<(u8, bool)>> {
    let Some(raw) = args.as_object_mut().and_then(|obj| obj.remove("_dots_approval_assessment")) else {
        return Ok(None);
    };
    let confidence = raw.get("confidence_percent").and_then(Value::as_u64)
        .filter(|value| *value <= 100)
        .ok_or_else(|| DotsError::Validation("confidence_percent deve ser um inteiro de 0 a 100".into()))?;
    let reversible = raw.get("reversible").and_then(Value::as_bool)
        .ok_or_else(|| DotsError::Validation("reversible deve ser booleano".into()))?;
    Ok(Some((confidence as u8, reversible)))
}

impl RuntimeService {
    /// Entrada unica das chamadas vindas do `dotsd mcp-gateway`.
    pub async fn gateway_call(&self, token: &str, tool: &str, mut args: Value) -> DotsResult<ToolOutput> {
        let ctx = self
            .registry
            .context_by_token(token)
            .ok_or_else(|| DotsError::Unauthorized("token de execucao invalido ou execucao encerrada".into()))?;
        if tool == "permission_prompt" {
            return self.permission_prompt(ctx, &args).await;
        }
        let assessment = take_approval_assessment(&mut args)?;
        let args = if BROWSER_ACTIONS.contains(&tool) { self.browser_context(&ctx.dot_id, args).await } else { args };
        // Demissao: quem nao gerencia a equipe do alvo nem chega a pedir aprovacao ao usuario.
        if tool == "dots_dismiss_employee" {
            let (who, reason) = (args.get("dot").and_then(|v| v.as_str()).unwrap_or(""), args.get("reason").and_then(|v| v.as_str()).unwrap_or(""));
            if let Err(msg) = self.dismissal_precheck(&ctx, who, reason).await? {
                self.audit_tool(&ctx, tool, &args, "denied", &msg).await;
                return Ok(ToolOutput::err(format!("NEGADO: {msg}")));
            }
        }
        // Sem rede geral, um site externo no navegador pode ser liberado pelo usuario para este dominio.
        // A permissao aprendida nao se aplica a comandos ou ferramentas nativas de rede.
        let browser_host = if !ctx.policy.allow_network && (tool == "dots_browser_open" || BROWSER_ACTIONS.contains(&tool)) {
            let url = args.get(if tool == "dots_browser_open" { "url" } else { "_page_url" }).and_then(Value::as_str).unwrap_or("");
            let host = crate::policy::url_host(url);
            if (url.starts_with("http://") || url.starts_with("https://"))
                && !host.is_empty()
                && !ctx.app_hosts.iter().any(|h| host == *h || host.ends_with(&format!(".{h}"))) {
                Some(host)
            } else {
                None
            }
        } else {
            None
        };
        let browser_grant = if let Some(host) = &browser_host {
            let (dot_id, key) = (ctx.dot_id.clone(), format!("dots_browser_open|{host}"));
            let learned = self.storage.call(move |st| st.learned().has(&dot_id, &key)).await?;
            if !learned && tool == "dots_browser_open" {
                if let Some(msg) = self.manager_does_work(&ctx, tool, RiskLevel::Exec).await {
                    self.audit_tool(&ctx, tool, &args, "denied", &msg).await;
                    return Ok(ToolOutput::err(format!("NEGADO: {msg}")));
                }
                let summary = format!("acessar {host} no navegador deste DOT");
                let (approved, reason) = self.request_approval(&ctx, tool, &args, RiskLevel::Exec, &[host.clone()], &summary, assessment).await?;
                if !approved {
                    self.audit_tool(&ctx, tool, &args, "rejected", &summary).await;
                    return Ok(ToolOutput::err(format!("NEGADO: o usuario nao aprovou acesso a {host}.{} Nao tente contornar; prossiga sem ele ou explique o bloqueio.", said(&reason))));
                }
            }
            learned || tool == "dots_browser_open"
        } else {
            false
        };
        let decision = if browser_grant { ctx.evaluate_with_network(tool, &args, true) } else { ctx.evaluate(tool, &args) };
        if let Decision::Allow { risk, .. } | Decision::NeedsApproval { risk, .. } = &decision {
            if let Some(msg) = self.manager_does_work(&ctx, tool, *risk).await {
                self.audit_tool(&ctx, tool, &args, "denied", &msg).await;
                return Ok(ToolOutput::err(format!("NEGADO: {msg}")));
            }
        }
        let (risk, resources, summary) = match decision {
            Decision::Deny(reason) => {
                self.audit_tool(&ctx, tool, &args, "denied", &reason).await;
                return Ok(ToolOutput::err(format!("NEGADO pela politica do DOTS: {reason}")));
            }
            Decision::Allow { risk, resources, summary } => (risk, resources, summary),
            Decision::NeedsApproval { risk, resources, summary } if (browser_grant && tool == "dots_browser_open") || self.already_learned(&ctx, tool, &args).await => {
                self.audit_tool(&ctx, tool, &args, "learned", &summary).await;
                (risk, resources, summary)
            }
            Decision::NeedsApproval { risk, resources, summary } => {
                let (approved, reason) = self.request_approval(&ctx, tool, &args, risk, &resources, &summary, assessment).await?;
                if !approved {
                    self.audit_tool(&ctx, tool, &args, "rejected", &summary).await;
                    return Ok(ToolOutput::err(format!("NEGADO: o usuario nao aprovou esta acao.{} Nao tente contornar; prossiga sem ela ou explique o bloqueio.", said(&reason))));
                }
                (risk, resources, summary)
            }
        };
        // Guarda de nao idempotencia: chamada identica ja concluida nesta tarefa (tentativa anterior).
        let guarded = matches!(tool, "dots_write_file" | "dots_delete_file" | "dots_run_command" | "dots_git");
        let input_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(args.to_string().as_bytes()));
        if guarded {
            let (tid, t, h) = (ctx.task_id.clone(), tool.to_string(), input_hash.clone());
            if let Ok(Some(prev)) = self.storage.call(move |st| st.checkpoints().find_invocation(&tid, &t, &h)).await {
                let (tid2, eid, t2, h2) = (ctx.task_id.clone(), ctx.execution_id.clone(), tool.to_string(), input_hash.clone());
                let _ = self.storage.call(move |st| st.checkpoints().record_invocation(&tid2, &eid, &t2, &h2, "replayed", "")).await;
                self.audit_tool(&ctx, tool, &args, "replayed", &summary).await;
                return Ok(ToolOutput::ok(format!("[ja executado em tentativa anterior desta tarefa; resultado gravado, nao repetido]
{prev}")));
            }
        }
        let out = self.execute_tool(&ctx, tool, &args).await;
        if guarded {
            if let Ok(o) = &out {
                let (tid, eid, t, h, outcome, text) = (ctx.task_id.clone(), ctx.execution_id.clone(), tool.to_string(), input_hash.clone(), if o.is_error { "error" } else { "ok" }, o.text.clone());
                let _ = self.storage.call(move |st| st.checkpoints().record_invocation(&tid, &eid, &t, &h, outcome, &text)).await;
                if !o.is_error {
                    let (tid, eid, step, res) = (ctx.task_id.clone(), ctx.execution_id.clone(), format!("apos {}", summary.chars().take(80).collect::<String>()), o.text.chars().take(500).collect::<String>());
                    let _ = self.storage.call(move |st| st.checkpoints().create(&tid, &eid, &step, &json!({}), "", &res, &[], "", &[], "auto")).await;
                }
            }
        }
        match &out {
            Ok(o) => {
                self.audit_tool(&ctx, tool, &args, if o.is_error { "error" } else { "ok" }, &summary).await;
                let _ = self.emit_event(et::TOOL_FINISHED, &format!("dot:{}", ctx.dot_id), json!({ "dot_id": ctx.dot_id, "task_id": ctx.task_id, "tool": tool, "ok": !o.is_error }), None, None, 0).await;
                for (path, kind, change_summary) in &o.changes {
                    let (eid, did, p, k, s) = (ctx.execution_id.clone(), ctx.dot_id.clone(), path.clone(), kind.clone(), change_summary.clone());
                    let _ = self.storage.call(move |st| st.changes().record(&eid, &did, &p, &k, &s)).await;
                }
                let _ = risk;
                let _ = resources;
            }
            Err(e) => self.audit_tool(&ctx, tool, &args, "error", e.message()).await,
        }
        out
    }

    async fn permission_prompt(&self, ctx: SharedContext, args: &Value) -> DotsResult<ToolOutput> {
        let tool_name = args.get("tool_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let input = args.get("input").cloned().unwrap_or(json!({}));
        // Ferramentas do proprio gateway sao autorizadas na chamada real.
        let decision = if tool_name.starts_with("mcp__dots__") || tool_name.starts_with("mcp__dots-") {
            Decision::Allow { risk: RiskLevel::Read, resources: vec![], summary: tool_name.clone() }
        } else {
            ctx.evaluate_native(&tool_name, &input)
        };
        if let Decision::Allow { risk, .. } | Decision::NeedsApproval { risk, .. } = &decision {
            if let Some(msg) = self.manager_does_work(&ctx, &tool_name, *risk).await {
                self.audit_tool(&ctx, &tool_name, &input, "denied", &msg).await;
                return Ok(ToolOutput::ok(json!({ "behavior": "deny", "message": format!("Negado pelo DOTS: {msg}") }).to_string()));
            }
        }
        let allow = match decision {
            Decision::Allow { .. } => (true, None),
            Decision::Deny(reason) => {
                self.audit_tool(&ctx, &tool_name, &input, "denied", &reason).await;
                return Ok(ToolOutput::ok(json!({ "behavior": "deny", "message": format!("Negado pela politica do DOTS: {reason}") }).to_string()));
            }
            Decision::NeedsApproval { .. } if self.already_learned(&ctx, &tool_name, &input).await => (true, None),
            Decision::NeedsApproval { risk, resources, summary } => self.request_approval(&ctx, &tool_name, &input, risk, &resources, &summary, None).await?,
        };
        if allow.0 {
            Ok(ToolOutput::ok(json!({ "behavior": "allow", "updatedInput": input }).to_string()))
        } else {
            Ok(ToolOutput::ok(json!({ "behavior": "deny", "message": format!("O usuario nao aprovou esta acao.{} Nao tente contornar; prossiga sem ela ou explique o bloqueio.", said(&allow.1)) }).to_string()))
        }
    }

    /// Gerente (supervisor ou quem gerencia uma equipe) nao executa o trabalho: acoes acima de leitura, fora das de
    /// gestao, sao negadas com a orientacao de delegar ou contratar. Devolve a mensagem quando nega.
    async fn manager_does_work(&self, ctx: &SharedContext, tool: &str, risk: RiskLevel) -> Option<String> {
        if risk == RiskLevel::Read || MANAGER_TOOLS.contains(&tool) {
            return None;
        }
        let d = ctx.dot_id.clone();
        let manager = self.storage.call(move |s| Ok(s.dots().get(&d)?.role == DotRole::Supervisor || s.teams().managed_by(&d)?.is_some())).await.unwrap_or(false);
        manager.then(|| "voce e gerente e gerente nao executa o trabalho, mesmo sem funcionarios. Delegue a um funcionario da equipe (dots_delegate); se nao houver quem faca, contrate um (dots_hire_employee) e delegue a ele.".to_string())
    }

    /// O DOT ja teve aprovada uma acao deste tipo (permissao aprendida)?
    async fn already_learned(&self, ctx: &SharedContext, tool: &str, input: &Value) -> bool {
        let Some((key, _)) = crate::policy::learn_key(tool, input) else { return false };
        let d = ctx.dot_id.clone();
        self.storage.call(move |st| st.learned().has(&d, &key)).await.unwrap_or(false)
    }

    /// Aprovada uma vez, o tipo de acao fica liberado para este DOT (pelo usuario ou pelo gerente).
    async fn learn(&self, ctx: &SharedContext, tool: &str, input: &Value, approval_id: &str) {
        let Some((key, summary)) = crate::policy::learn_key(tool, input) else { return };
        let (d, a) = (ctx.dot_id.clone(), approval_id.to_string());
        match self.storage.call(move |st| st.learned().grant(&d, &key, &summary, Some(&a))).await {
            Ok(()) => self.events.emit("learned.changed", json!({ "dot_id": ctx.dot_id })),
            Err(e) => tracing::warn!(error = %e, "nao foi possivel gravar a permissao aprendida"),
        }
    }

    /// Cria a aprovacao, coloca a tarefa em WAITING_APPROVAL e aguarda a decisao (ou expira). Devolve se foi aprovada
    /// e, se negada, o que quem decidiu disse para fazer diferente.
    async fn request_approval(&self, ctx: &SharedContext, tool: &str, input: &Value, risk: RiskLevel, resources: &[String], summary: &str, assessment: Option<(u8, bool)>) -> DotsResult<(bool, Option<String>)> {
        let ttl = self.approval_ttl().await;
        let (eid, tid, did, t, i, r, res, s) = (ctx.execution_id.clone(), ctx.task_id.clone(), ctx.dot_id.clone(), tool.to_string(), input.clone(), risk, resources.to_vec(), summary.to_string());
        let approval = self
            .storage
            .call(move |st| st.approvals().create_with_assessment(&eid, &tid, &did, &t, &i, r, &res, &s, ttl, assessment))
            .await?;
        let rx = self.registry.wait_approval(&approval.id);
        let tid = ctx.task_id.clone();
        let _ = self.storage.call(move |st| st.tasks().set_status(&tid, TaskStatus::WaitingApproval, None)).await;
        self.events.emit("task.changed", json!({ "id": ctx.task_id, "dot_id": ctx.dot_id, "status": "WAITING_APPROVAL" }));
        self.events.emit("approval.requested", json!(approval));
        let _ = ctx.events.send(ExecutionEvent::PermissionRequested { approval_id: approval.id.clone(), tool: tool.to_string(), risk }).await;
        // Em pedido delegado, quem decide e o gerente (o usuario continua podendo decidir pela interface).
        let routed = if tool == "dots_browser_open" && !ctx.policy.allow_network {
            false
        } else {
            self.route_approval_to_manager(ctx, &approval).await
        };
        tracing::info!(approval = %approval.id, tool, risk = risk.as_str(), routed_to_manager = routed, "aguardando aprovacao");

        let approved = match tokio::time::timeout(Duration::from_secs(ttl.max(1) as u64), rx).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => false,
            Err(_) => {
                self.registry.drop_approval(&approval.id);
                let _ = self.storage.call(|st| st.approvals().expire_due()).await;
                self.events.emit("approval.resolved", json!({ "id": approval.id, "status": "expired" }));
                false
            }
        };
        let reason = if approved {
            self.learn(ctx, tool, input, &approval.id).await;
            None
        } else {
            let id = approval.id.clone();
            self.storage.call(move |st| st.approvals().get(&id)).await.ok().and_then(|a| a.reason).filter(|r| !r.trim().is_empty())
        };
        let tid = ctx.task_id.clone();
        let _ = self.storage.call(move |st| st.tasks().set_status(&tid, TaskStatus::Running, None)).await;
        self.events.emit("task.changed", json!({ "id": ctx.task_id, "dot_id": ctx.dot_id, "status": "RUNNING" }));
        let _ = ctx.events.send(ExecutionEvent::PermissionResolved { approval_id: approval.id.clone(), approved }).await;
        Ok((approved, reason))
    }

    /// Decisao do usuario (via IPC). Para perguntas, `reason` e a resposta.
    pub async fn decide_approval(&self, id: &str, approved: bool, reason: Option<String>) -> DotsResult<dots_core::Approval> {
        self.decide_approval_by(id, approved, reason, "user", "o usuario").await
    }

    /// Decisao de uma aprovacao por `actor` ("user" ou "dot:<id>"); `who` e como o decisor aparece na continuacao.
    pub async fn decide_approval_by(&self, id: &str, approved: bool, reason: Option<String>, actor: &str, who: &str) -> DotsResult<dots_core::Approval> {
        let (i, r) = (id.to_string(), reason.clone());
        let approval = self.storage.call(move |st| st.approvals().resolve(&i, approved, r.as_deref())).await?;
        self.registry.resolve_approval(id, approved);
        self.events.emit("approval.resolved", json!(approval));
        let (etype, key) = if approved { (et::APPROVAL_GRANTED, format!("approval:{id}:granted")) } else { (et::APPROVAL_REJECTED, format!("approval:{id}:rejected")) };
        let _ = self.emit_event(etype, actor, json!({ "approval_id": id, "dot_id": approval.dot_id, "dot_name": approval.dot_name, "task_id": approval.task_id, "tool": approval.tool, "kind": approval.kind.as_str() }), None, Some(key), 0).await;
        if approval.kind == ApprovalKind::Question {
            self.continue_after_question(&approval, who).await;
        }
        let (a, i2, actor) = (if approved { "approval.approve" } else { "approval.reject" }, id.to_string(), actor.to_string());
        let _ = self.storage.call(move |st| st.audit().record(&actor, a, "approval", Some(&i2), json!({ "reason": reason })).map(|_| ())).await;
        Ok(approval)
    }

    async fn approval_ttl(&self) -> i64 {
        self.storage
            .call(|st| st.settings().get("approvals.ttl_seconds"))
            .await
            .ok()
            .flatten()
            .and_then(|s| s.value.as_i64())
            .unwrap_or(APPROVAL_TTL_SECONDS)
    }

    async fn execute_tool(&self, ctx: &SharedContext, tool: &str, args: &Value) -> DotsResult<ToolOutput> {
        if tool.starts_with("flexi_") {
            return self.flexi_execute(ctx, tool, args).await;
        }
        let s = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let n = |k: &str, d: u64| args.get(k).and_then(|v| v.as_u64()).unwrap_or(d);
        let home = self.home.clone();
        let path_dirs = self.path_dirs.clone();
        match tool {
            "dots_list_dir" => tools::list_dir(&ctx.authorize_path(&s("path"), false)?, n("depth", 1) as usize),
            "dots_read_file" => tools::read_file(&ctx.authorize_path(&s("path"), false)?, n("offset", 0) as usize, n("limit", 400) as usize, ctx.policy.max_file_bytes),
            "dots_search" => tools::search(&ctx.authorize_path(&s("path"), false)?, &s("pattern"), args.get("glob").and_then(|g| g.as_str()), &path_dirs, &home).await,
            "dots_write_file" => tools::write_file(&ctx.authorize_path(&s("path"), true)?, &s("content")),
            "dots_delete_file" => tools::delete_file(&ctx.authorize_path(&s("path"), true)?),
            "dots_run_command" => {
                let cwd = if s("cwd").is_empty() { ctx.workspace.clone() } else { ctx.authorize_path(&s("cwd"), false)? };
                tools::run_command(&s("command"), &cwd, Duration::from_secs(n("timeout_seconds", 120).clamp(1, 600)), &home, &path_dirs).await
            }
            "dots_git" => {
                let cwd = if s("cwd").is_empty() { ctx.workspace.clone() } else { ctx.authorize_path(&s("cwd"), false)? };
                let argv: Vec<String> = args.get("args").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                tools::git(&s("op"), &argv, &cwd, &home, &path_dirs).await
            }
            "dots_checkpoint" => {
                let (tid, eid, step, state, next, arts) = (ctx.task_id.clone(), ctx.execution_id.clone(), s("step"), args.get("state").cloned().unwrap_or(json!({})), s("next_action"), args.get("artifacts").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default());
                let results = s("results");
                let cp = self.storage.call(move |st| st.checkpoints().create(&tid, &eid, &step, &state, "", &results, &[], &next, &arts, "dot")).await?;
                Ok(ToolOutput::ok(format!("checkpoint gravado ({}): {}", cp.id, cp.step)))
            }
            "dots_delegate" => self.delegate(ctx, &s("to"), &s("request"), args.get("context").and_then(|c| c.as_str())).await,
            "dots_delegation_status" => self.delegation_status(ctx, args.get("id").and_then(|c| c.as_str())).await,
            "dots_cancel_delegation" => self.cancel_delegation(ctx, &s("id")).await,
            "dots_resume_delegation" => self.resume_delegation(ctx, &s("id")).await,
            "dots_decide_approval" => self.decide_approval_as_manager(ctx, &s("approval_id"), args.get("approved").and_then(|v| v.as_bool()).unwrap_or(false), args.get("reason").and_then(|v| v.as_str())).await,
            "dots_list_team" => self.list_team(ctx).await,
            "dots_schedule_create" => self.schedule_create_for_dot(ctx, &s("kind"), &s("spec"), &s("prompt")).await,
            "dots_schedule_list" => self.schedule_list_for_dot(ctx).await,
            "dots_schedule_cancel" => self.schedule_cancel_for_dot(ctx, &s("id")).await,
            "dots_goal_create" => self.goal_create_for_dot(ctx, &s("title"), args.get("target").and_then(|v| v.as_f64()).unwrap_or(0.0), &s("unit"), args.get("current").and_then(|v| v.as_f64()).unwrap_or(0.0), &s("deadline")).await,
            "dots_goal_progress" => self.goal_progress_for_dot(ctx, &s("id"), args.get("value").and_then(|v| v.as_f64()), args.get("delta").and_then(|v| v.as_f64()), &s("note")).await,
            "dots_goal_list" => self.goal_list_for_dot(ctx).await,
            "dots_goal_close" => self.goal_close_for_dot(ctx, &s("id"), &s("status")).await,
            "dots_dismiss_employee" => self.dismiss_employee(ctx, &s("dot"), &s("reason"), &s("lesson")).await,
            "dots_hire_employee" => self.hire_employee(ctx, &s("name"), &s("role"), &s("instructions")).await,
            "dots_pause_employee" => self.set_employee_status(ctx, &s("dot"), DotStatus::Paused, &s("reason")).await,
            "dots_resume_employee" => self.set_employee_status(ctx, &s("dot"), DotStatus::Active, "").await,
            "dots_ask_user" => self.ask_user(ctx, &s("question"), args.get("options").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()).await,
            "dots_answer_question" => self.answer_question(ctx, &s("question_id"), &s("answer")).await,
            "dots_notify_user" => self.notify_user(ctx, &s("title"), &s("detail"), &s("action")).await,
            "dots_browser_open" => {
                let url = s("url");
                let browser = self.browsers.get(&ctx.dot_id, &self.browser_options().await).await?;
                match self.browser_login_for_app(&ctx.dot_id, &browser, &url).await {
                    Ok(true) => tracing::info!(dot = %ctx.dot_id, "navegador: sessao do app conectado aplicada"),
                    Ok(false) => {}
                    Err(DotsError::Unauthorized(m)) => return Ok(ToolOutput::err(format!("navegador: {m}"))),
                    Err(e) => tracing::warn!(dot = %ctx.dot_id, error = %e, "navegador: nao foi possivel aplicar a sessao do app"),
                }
                match browser.open(&url).await {
                    Ok(page) => Ok(ToolOutput::ok(tools::cap(page.render()))),
                    Err(e) => Ok(ToolOutput::err(format!("navegador: {}", e.message()))),
                }
            }
            "dots_browser_click" | "dots_browser_type" | "dots_browser_press" | "dots_browser_select" | "dots_browser_read" => {
                let Some(browser) = self.browsers.running(&ctx.dot_id).await else {
                    return Ok(ToolOutput::err("navegador: nenhuma pagina aberta; use dots_browser_open antes".into()));
                };
                let r = browser_ref(args);
                let need_ref = || r.ok_or_else(|| DotsError::Validation("informe o ref (numero do [ref=N] na leitura da pagina)".into()));
                let flag = |k: &str, d: bool| args.get(k).and_then(|v| v.as_bool()).unwrap_or(d);
                let res = match tool {
                    "dots_browser_click" => match need_ref() { Ok(r) => browser.click(r).await, Err(e) => Err(e) },
                    "dots_browser_type" => match need_ref() { Ok(r) => browser.type_text(r, &s("text"), flag("clear", true), flag("submit", false)).await, Err(e) => Err(e) },
                    "dots_browser_select" => match need_ref() { Ok(r) => browser.select(r, &s("option")).await, Err(e) => Err(e) },
                    "dots_browser_press" => browser.press(&s("key")).await,
                    _ => browser.read().await,
                };
                match res {
                    Ok(page) => Ok(ToolOutput::ok(tools::cap(page.render()))),
                    Err(e) => Ok(ToolOutput::err(format!("navegador: {}", e.message()))),
                }
            }
            "dots_system_info" => {
                let engines = self.storage.call(|st| st.engines().list()).await?;
                let list: Vec<(String, Option<String>)> = engines.iter().map(|e| (e.kind.as_str().to_string(), e.last_detection.as_ref().and_then(|d| d.version.clone()))).collect();
                Ok(tools::system_info(crate::VERSION, &list))
            }
            "dots_remember" => {
                let (did, content, imp) = (ctx.dot_id.clone(), s("content"), args.get("importance").and_then(|v| v.as_i64()).map(|v| v as i32));
                let src = Some(format!("task:{}", ctx.task_id));
                let m = self.storage.call(move |st| st.memories().create(MemoryCreate { dot_id: did, kind: MemoryKind::LongTerm, content, importance: imp, source: src })).await?;
                self.events.emit("memory.changed", json!({ "dot_id": ctx.dot_id }));
                Ok(ToolOutput::ok(format!("memoria gravada ({})", m.id)))
            }
            "dots_recall" => {
                let (did, q, lim) = (ctx.dot_id.clone(), s("query"), n("limit", 10) as u32);
                let found = self.storage.call(move |st| st.memories().search(&did, &q, &[MemoryKind::LongTerm, MemoryKind::History], lim)).await?;
                if found.is_empty() {
                    return Ok(ToolOutput::ok("nenhuma memoria relevante".into()));
                }
                Ok(ToolOutput::ok(found.iter().map(|m| format!("- [{}|imp {}] {}", m.kind.as_str(), m.importance, m.content)).collect::<Vec<_>>().join("\n")))
            }
            other => Ok(ToolOutput::err(format!("ferramenta desconhecida: {other}"))),
        }
    }

    /// Acrescenta aos argumentos de uma acao no navegador o que a politica precisa saber, lido do proprio navegador:
    /// `_page_url` (pagina atual), `_element` (papel e nome do `ref`) e `_dialog` (dialogo nativo aberto).
    /// O que o DOT mandar com esses nomes e descartado.
    async fn browser_context(&self, dot_id: &str, mut args: Value) -> Value {
        if let Some(o) = args.as_object_mut() {
            o.retain(|k, _| !k.starts_with('_'));
        } else {
            args = json!({});
        }
        let Some(browser) = self.browsers.running(dot_id).await else { return args };
        args["_page_url"] = json!(browser.current_url().await.unwrap_or_default());
        if let Some(d) = browser.dialog() {
            args["_dialog"] = json!(d);
        } else if let Some(r) = browser_ref(&args) {
            if let Ok(desc) = browser.describe(r).await {
                args["_element"] = json!(desc);
            }
        }
        args
    }

    async fn audit_tool(&self, ctx: &SharedContext, tool: &str, input: &Value, outcome: &str, summary: &str) {
        let actor = format!("dot:{}", ctx.dot_id);
        let action = format!("tool.{}", tool.trim_start_matches("mcp__dots__"));
        let eid = ctx.execution_id.clone();
        let details = json!({ "outcome": outcome, "summary": summary, "input": truncate_json(input), "task_id": ctx.task_id, "dot_name": ctx.dot_name });
        let _ = self.storage.call(move |st| st.audit().record(&actor, &action, "execution", Some(&eid), details).map(|_| ())).await;
    }
}

/// Trecho da negacao com o que quem decidiu disse para fazer diferente (vazio sem motivo).
fn said(reason: &Option<String>) -> String {
    reason.as_deref().map(|r| format!(" Disse para fazer diferente: \"{}\".", r.trim())).unwrap_or_default()
}

fn truncate_json(v: &Value) -> Value {
    let s = v.to_string();
    if s.len() <= 2000 {
        return v.clone();
    }
    // Recua ate o inicio de um caractere: cortar no meio de um acento (UTF-8 multibyte) causa panico.
    let mut end = 2000;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    Value::String(format!("{}…", &s[..end]))
}

#[cfg(test)]
mod truncate_json_tests {
    use super::{take_approval_assessment, truncate_json};
    use serde_json::{Value, json};

    #[test]
    fn corta_sem_partir_caractere_acentuado() {
        // Na string JSON (com as aspas) o "ç" ocupa os bytes 1999..2001, entao o byte 2000 cai no meio dele.
        let texto = format!("{}ãção", "a".repeat(1996));
        let v = json!(texto);
        assert!(v.to_string().len() > 2000);
        let out = truncate_json(&v);
        let Value::String(s) = out else { panic!("esperava string") };
        assert!(s.ends_with('…'));
        assert!(s.trim_end_matches('…').len() <= 2000);
    }

    #[test]
    fn mantem_valor_curto_intacto() {
        let v = json!({ "op": "restore", "msg": "ação" });
        assert_eq!(truncate_json(&v), v);
    }

    #[test]
    fn avaliacao_do_agente_e_validada_e_removida_dos_parametros() {
        let mut args = json!({ "id": 42, "_dots_approval_assessment": { "confidence_percent": 91, "reversible": false } });
        assert_eq!(take_approval_assessment(&mut args).unwrap(), Some((91, false)));
        assert_eq!(args, json!({ "id": 42 }));
        assert_eq!(take_approval_assessment(&mut args).unwrap(), None);

        let mut invalid = json!({ "_dots_approval_assessment": { "confidence_percent": 101, "reversible": true } });
        assert!(take_approval_assessment(&mut invalid).is_err());
        let mut invalid = json!({ "_dots_approval_assessment": { "confidence_percent": 80, "reversible": "sim" } });
        assert!(take_approval_assessment(&mut invalid).is_err());
    }
}

impl ToolOutput {
    pub fn to_mcp_result(&self) -> Value {
        json!({ "content": [{ "type": "text", "text": self.text }], "isError": self.is_error })
    }
}

pub fn gateway_config(dotsd_path: &std::path::Path, data_dir: &std::path::Path, token: &str) -> dots_engines::GatewayConfig {
    dots_engines::GatewayConfig {
        server_name: "dots".into(),
        command: dotsd_path.to_path_buf(),
        args: vec!["mcp-gateway".into()],
        env: vec![
            ("DOTS_DATA_DIR".into(), data_dir.to_string_lossy().to_string()),
            ("DOTS_EXECUTION_TOKEN".into(), token.to_string()),
            ("HOME".into(), std::env::var("HOME").unwrap_or_default()),
        ],
    }
}

