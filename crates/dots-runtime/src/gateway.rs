//! Servico do gateway de ferramentas: autoriza, aprova, executa e audita.

use std::time::Duration;

use dots_core::automation::event_types as et;
use dots_core::{ApprovalKind, DotsError, DotsResult, ExecutionEvent, MemoryCreate, MemoryKind, RiskLevel, TaskStatus};
use serde_json::{Value, json};

use crate::policy::{Decision, SharedContext};
use crate::service::RuntimeService;
use crate::tools::{self, ToolOutput};

pub const APPROVAL_TTL_SECONDS: i64 = 600;

impl RuntimeService {
    /// Entrada unica das chamadas vindas do `dotsd mcp-gateway`.
    pub async fn gateway_call(&self, token: &str, tool: &str, args: Value) -> DotsResult<ToolOutput> {
        let ctx = self
            .registry
            .context_by_token(token)
            .ok_or_else(|| DotsError::Unauthorized("token de execucao invalido ou execucao encerrada".into()))?;
        if tool == "permission_prompt" {
            return self.permission_prompt(ctx, &args).await;
        }
        let decision = ctx.evaluate(tool, &args);
        let (risk, resources, summary) = match decision {
            Decision::Deny(reason) => {
                self.audit_tool(&ctx, tool, &args, "denied", &reason).await;
                return Ok(ToolOutput::err(format!("NEGADO pela politica do DOTS: {reason}")));
            }
            Decision::Allow { risk, resources, summary } => (risk, resources, summary),
            Decision::NeedsApproval { risk, resources, summary } => {
                let approved = self.request_approval(&ctx, tool, &args, risk, &resources, &summary).await?;
                if !approved {
                    self.audit_tool(&ctx, tool, &args, "rejected", &summary).await;
                    return Ok(ToolOutput::err("NEGADO: o usuario nao aprovou esta acao. Nao tente contornar; prossiga sem ela ou explique o bloqueio.".into()));
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
        let allow = match decision {
            Decision::Allow { .. } => true,
            Decision::Deny(reason) => {
                self.audit_tool(&ctx, &tool_name, &input, "denied", &reason).await;
                return Ok(ToolOutput::ok(json!({ "behavior": "deny", "message": format!("Negado pela politica do DOTS: {reason}") }).to_string()));
            }
            Decision::NeedsApproval { risk, resources, summary } => self.request_approval(&ctx, &tool_name, &input, risk, &resources, &summary).await?,
        };
        if allow {
            Ok(ToolOutput::ok(json!({ "behavior": "allow", "updatedInput": input }).to_string()))
        } else {
            Ok(ToolOutput::ok(json!({ "behavior": "deny", "message": "O usuario nao aprovou esta acao. Nao tente contornar; prossiga sem ela ou explique o bloqueio." }).to_string()))
        }
    }

    /// Cria a aprovacao, coloca a tarefa em WAITING_APPROVAL e aguarda a decisao (ou expira).
    async fn request_approval(&self, ctx: &SharedContext, tool: &str, input: &Value, risk: RiskLevel, resources: &[String], summary: &str) -> DotsResult<bool> {
        let ttl = self.approval_ttl().await;
        let (eid, tid, did, t, i, r, res, s) = (ctx.execution_id.clone(), ctx.task_id.clone(), ctx.dot_id.clone(), tool.to_string(), input.clone(), risk, resources.to_vec(), summary.to_string());
        let approval = self
            .storage
            .call(move |st| st.approvals().create(&eid, &tid, &did, &t, &i, r, &res, &s, ttl))
            .await?;
        let rx = self.registry.wait_approval(&approval.id);
        let tid = ctx.task_id.clone();
        let _ = self.storage.call(move |st| st.tasks().set_status(&tid, TaskStatus::WaitingApproval, None)).await;
        self.events.emit("task.changed", json!({ "id": ctx.task_id, "dot_id": ctx.dot_id, "status": "WAITING_APPROVAL" }));
        self.events.emit("approval.requested", json!(approval));
        let _ = ctx.events.send(ExecutionEvent::PermissionRequested { approval_id: approval.id.clone(), tool: tool.to_string(), risk }).await;
        tracing::info!(approval = %approval.id, tool, risk = risk.as_str(), "aguardando aprovacao do usuario");

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
        let tid = ctx.task_id.clone();
        let _ = self.storage.call(move |st| st.tasks().set_status(&tid, TaskStatus::Running, None)).await;
        self.events.emit("task.changed", json!({ "id": ctx.task_id, "dot_id": ctx.dot_id, "status": "RUNNING" }));
        let _ = ctx.events.send(ExecutionEvent::PermissionResolved { approval_id: approval.id.clone(), approved }).await;
        Ok(approved)
    }

    /// Decisao do usuario (via IPC). Para perguntas, `reason` e a resposta.
    pub async fn decide_approval(&self, id: &str, approved: bool, reason: Option<String>) -> DotsResult<dots_core::Approval> {
        let (i, r) = (id.to_string(), reason.clone());
        let approval = self.storage.call(move |st| st.approvals().resolve(&i, approved, r.as_deref())).await?;
        self.registry.resolve_approval(id, approved);
        self.events.emit("approval.resolved", json!(approval));
        let (etype, key) = if approved { (et::APPROVAL_GRANTED, format!("approval:{id}:granted")) } else { (et::APPROVAL_REJECTED, format!("approval:{id}:rejected")) };
        let _ = self.emit_event(etype, "user", json!({ "approval_id": id, "dot_id": approval.dot_id, "dot_name": approval.dot_name, "task_id": approval.task_id, "tool": approval.tool, "kind": approval.kind.as_str() }), None, Some(key), 0).await;
        if approval.kind == ApprovalKind::Question {
            self.on_question_answered(&approval).await;
        }
        let (a, i2) = (if approved { "approval.approve" } else { "approval.reject" }, id.to_string());
        let _ = self.storage.call(move |st| st.audit().record("user", a, "approval", Some(&i2), json!({ "reason": reason })).map(|_| ())).await;
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
            "dots_list_team" => self.list_team(ctx).await,
            "dots_ask_user" => self.ask_user(ctx, &s("question"), args.get("options").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()).await,
            "dots_answer_question" => self.answer_question(ctx, &s("question_id"), &s("answer")).await,
            "dots_browser_open" => {
                let browser = self.browsers.get(&ctx.dot_id, &self.browser_options().await).await?;
                match browser.open(&s("url")).await {
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

    async fn audit_tool(&self, ctx: &SharedContext, tool: &str, input: &Value, outcome: &str, summary: &str) {
        let actor = format!("dot:{}", ctx.dot_id);
        let action = format!("tool.{}", tool.trim_start_matches("mcp__dots__"));
        let eid = ctx.execution_id.clone();
        let details = json!({ "outcome": outcome, "summary": summary, "input": truncate_json(input), "task_id": ctx.task_id, "dot_name": ctx.dot_name });
        let _ = self.storage.call(move |st| st.audit().record(&actor, &action, "execution", Some(&eid), details).map(|_| ())).await;
    }
}

fn truncate_json(v: &Value) -> Value {
    let s = v.to_string();
    if s.len() > 2000 { Value::String(format!("{}…", &s[..2000])) } else { v.clone() }
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

