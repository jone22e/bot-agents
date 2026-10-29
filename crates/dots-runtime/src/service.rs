//! Implementacao dos metodos IPC. Toda mutacao passa por aqui, grava auditoria
//! (nos repositorios) e emite evento para os clientes conectados.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use dots_core::automation::event_types as et;
use dots_core::{DotCreate, DotStatus, DotUpdate, DotsError, DotsResult, EngineKind, MemoryCreate, MemoryKind, MemoryUpdate, MessageRole, RuntimeMode, RuntimeStatus, ScheduleCreate, ScheduleUpdate, SubscriptionCreate, SubscriptionUpdate, TaskCreate, TaskKind, TaskStatus, Timestamp, ToolPolicyUpdate};
use dots_engines::{DetectOptions, EngineManager};
use dots_storage::repos::TaskFilter;
use dots_ipc::server::EventSink;
use dots_storage::StorageHandle;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::paths::DataLayout;
use crate::shutdown::Shutdown;

pub struct RuntimeService {
    pub storage: StorageHandle,
    pub events: EventSink,
    pub clients: Arc<AtomicU32>,
    pub layout: DataLayout,
    pub socket_path: PathBuf,
    pub started_at: Timestamp,
    pub mode: RuntimeMode,
    pub shutdown: Shutdown,
    pub schema_version: u32,
    pub registry: crate::registry::ExecutionRegistry,
    pub engines: EngineManager,
    pub scheduler_notify: tokio::sync::Notify,
    pub paused: std::sync::atomic::AtomicBool,
    pub home: PathBuf,
    pub path_dirs: Vec<PathBuf>,
    pub dotsd_path: PathBuf,
    pub schedules_notify: tokio::sync::Notify,
    pub autonomy_lock: tokio::sync::Mutex<()>,
    pub delegation_finish_lock: tokio::sync::Mutex<()>,
    pub watcher: Arc<std::sync::Mutex<crate::watcher::Watcher>>,
    /// Profundidade de encadeamento de eventos por tarefa (em memoria).
    pub event_depths: std::sync::Mutex<std::collections::HashMap<String, u32>>,
    pub flexi: crate::flexi::FlexiRegistry,
    pub browsers: crate::browser::BrowserPool,
    pub usage: crate::usage::UsageCache,
    pub cli_updates: crate::cli_updates::UpdatesCache,
    /// Sincronizacao REST com o Flexi web (ADR 0016).
    pub flexi_link: crate::flexi_sync::LinkState,
}

#[derive(Deserialize)]
struct MfaParams {
    id: String,
    code: String,
}

#[derive(Deserialize)]
struct ReconnectParams {
    id: String,
    #[serde(default)]
    senha: Option<String>,
    #[serde(default)]
    remember_password: Option<bool>,
}

#[derive(Deserialize)]
struct AssignRoleParams {
    dot_id: String,
    connection_id: String,
    #[serde(default)]
    template_id: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    role: Option<dots_core::DotRole>,
}
#[derive(Deserialize)]
struct IntegrationParams {
    dot_id: String,
    kind: String,
    #[serde(default)]
    connection_id: Option<String>,
}

#[derive(Deserialize)]
struct TokenParams {
    token: String,
}

#[derive(Deserialize, Default)]
struct ForceParams {
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize)]
struct KindParams {
    kind: EngineKind,
}

#[derive(Deserialize)]
struct SubUpdateParams {
    id: String,
    patch: SubscriptionUpdate,
}

#[derive(Deserialize)]
struct SchUpdateParams {
    id: String,
    patch: ScheduleUpdate,
}

#[derive(Deserialize, Default)]
struct EventListParams {
    #[serde(default)]
    event_type: Option<String>,
    #[serde(default)]
    limit: u32,
}

#[derive(Deserialize, Default)]
struct LinesParams {
    #[serde(default = "default_lines")]
    lines: u32,
}
fn default_lines() -> u32 {
    200
}

#[derive(Deserialize)]
struct GatewayCallParams {
    token: String,
    tool: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Deserialize, Default)]
struct TaskListParams {
    #[serde(default)]
    dot_id: Option<String>,
    #[serde(default)]
    status: Option<Vec<TaskStatus>>,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    limit: u32,
}

#[derive(Deserialize)]
struct EventsParams {
    execution_id: String,
    #[serde(default)]
    after_seq: u64,
    #[serde(default = "default_limit")]
    limit: u32,
}

#[derive(Deserialize)]
struct ExecutionPeriodParams {
    start_at: Timestamp,
    end_at: Timestamp,
}

#[derive(Deserialize)]
struct ConvCreateParams {
    dot_id: String,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Deserialize)]
struct ConvSendParams {
    conversation_id: String,
    content: String,
    #[serde(default)]
    attachments: Vec<String>,
}

/// Anexo da conversa: um arquivo do computador (`path`) ou bytes colados (`data`, base64).
#[derive(Deserialize)]
struct ConvAttachParams {
    conversation_id: String,
    name: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    data: Option<String>,
}

#[derive(Deserialize, Default)]
struct DotIdParams {
    #[serde(default)]
    dot_id: Option<String>,
}

#[derive(Deserialize)]
struct DecideParams {
    id: String,
    approved: bool,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Deserialize)]
struct MemListParams {
    dot_id: String,
    #[serde(default)]
    kind: Option<MemoryKind>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: u32,
}

#[derive(Deserialize)]
struct MemUpdateParams {
    id: String,
    patch: MemoryUpdate,
}

#[derive(Deserialize)]
struct PolicySetParams {
    dot_id: String,
    patch: ToolPolicyUpdate,
}

#[derive(Deserialize)]
struct WorkspaceParams {
    dot_id: String,
    path: String,
    #[serde(default = "default_true")]
    can_read: bool,
    #[serde(default)]
    can_write: bool,
}
fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
struct PauseParams {
    paused: bool,
}

#[derive(Deserialize)]
struct EventsParamsLite {
    id: String,
    #[serde(default)]
    limit: u32,
}

/// `id` ausente = limpar todas as pendentes.
#[derive(Deserialize, Default)]
struct NotificationClearParams {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Deserialize, Default)]
struct ApprovalListParams {
    #[serde(default)]
    pending_only: bool,
    #[serde(default)]
    limit: u32,
}

const ACTOR_USER: &str = "user";

pub(crate) fn parse_params<T: for<'de> Deserialize<'de>>(params: Value) -> DotsResult<T> {
    p(params)
}

fn p<T: for<'de> Deserialize<'de>>(params: Value) -> DotsResult<T> {
    serde_json::from_value(params).map_err(|e| DotsError::Validation(format!("parametros invalidos: {e}")))
}

#[derive(Deserialize)]
struct IdParams {
    id: String,
}

#[derive(Deserialize, Default)]
struct ListDotsParams {
    #[serde(default)]
    include_archived: bool,
}

#[derive(Deserialize)]
struct UpdateDotParams {
    id: String,
    patch: DotUpdate,
}

#[derive(Deserialize)]
struct DuplicateParams {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct KeyParams {
    key: String,
}

#[derive(Deserialize)]
struct SetSettingParams {
    key: String,
    value: Value,
}

#[derive(Deserialize, Default)]
struct DetectParams {
    #[serde(default)]
    kind: Option<EngineKind>,
}

#[derive(Deserialize)]
struct OverrideParams {
    kind: EngineKind,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Deserialize)]
struct EnabledParams {
    kind: EngineKind,
    enabled: bool,
}

#[derive(Deserialize)]
struct ModelParams {
    kind: EngineKind,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize, Default)]
struct LimitParams {
    #[serde(default = "default_limit")]
    limit: u32,
}

/// Periodo da avaliacao de desempenho em dias (7 ou 30; ausente vira 7).
#[derive(Deserialize, Default)]
struct DaysParams {
    #[serde(default)]
    days: u64,
}
fn default_limit() -> u32 {
    100
}

impl RuntimeService {
    pub async fn dispatch(&self, method: &str, params: Value) -> DotsResult<Value> {
        tracing::debug!(method, "ipc");
        match method {
            "runtime.status" => Ok(json!(self.status())),
            "runtime.shutdown" => {
                self.events.emit("runtime.shutting_down", json!({}));
                self.shutdown.trigger("solicitado via IPC");
                Ok(json!({ "ok": true }))
            }
            "runtime.backup" => {
                let path = self.storage.call(|s| s.backup_now("manual")).await?;
                self.audit("runtime.backup", "runtime", None, json!({ "path": path })).await?;
                Ok(json!({ "path": path }))
            }
            "runtime.backups" => {
                let list = self.storage.call(|s| s.list_backups()).await?;
                Ok(json!(list))
            }
            "installation.get" => {
                let inst = self.storage.call(|s| s.installation().get()).await?;
                Ok(json!(inst))
            }
            "overview.get" => self.overview().await,

            "settings.list" => Ok(json!(self.storage.call(|s| s.settings().list()).await?)),
            "settings.get" => {
                let KeyParams { key } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.settings().get(&key)).await?))
            }
            "settings.set" => {
                let SetSettingParams { key, value } = p(params)?;
                let k = key.clone();
                let setting = self.storage.call(move |s| s.settings().set(&k, &value)).await?;
                self.events.emit("settings.changed", json!({ "key": key }));
                Ok(json!(setting))
            }

            "dots.list" => {
                let ListDotsParams { include_archived } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.dots().list(include_archived)).await?))
            }
            "dots.get" => {
                let IdParams { id } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.dots().get(&id)).await?))
            }
            "dots.start_autonomy" => {
                let IdParams { id } = p(params)?;
                self.start_autonomy(&id).await?;
                Ok(json!({ "ok": true }))
            }
            "dots.workspaces" => {
                let IdParams { id } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.dots().workspaces(&id)).await?))
            }
            "dots.create" => {
                let input: DotCreate = p(params)?;
                let ws_root = self.layout.workspaces_dir.clone();
                let dot = self
                    .storage
                    .call(move |s| {
                        // Workspace padrao: workspaces/<id>. O id so existe apos o insert,
                        // entao cria-se com placeholder e ajusta-se na sequencia.
                        let created = s.dots().create(ACTOR_USER, input, None)?;
                        if created.workspace_path.is_none() {
                            let path = ws_root.join(&created.id);
                            std::fs::create_dir_all(&path)?;
                            return s.dots().update(
                                ACTOR_USER,
                                &created.id,
                                DotUpdate { workspace_path: Some(Some(path.to_string_lossy().to_string())), ..Default::default() },
                            );
                        }
                        Ok(created)
                    })
                    .await?;
                self.events.emit("dots.changed", json!({ "id": dot.id, "action": "create" }));
                Ok(json!(dot))
            }
            "dots.update" => {
                let UpdateDotParams { id, patch } = p(params)?;
                let id2 = id.clone();
                // Gerente que troca de assistente ou modelo leva os funcionarios da equipe junto.
                let (dot, members) = self
                    .storage
                    .call(move |s| {
                        let before = s.dots().get(&id2)?;
                        let dot = s.dots().update(ACTOR_USER, &id2, patch)?;
                        let mut members = vec![];
                        if dot.primary_engine != before.primary_engine || dot.model != before.model {
                            if let Some(team) = s.teams().managed_by(&id2)? {
                                for m in team.member_ids {
                                    s.dots().update(ACTOR_USER, &m, DotUpdate { primary_engine: Some(dot.primary_engine), model: Some(dot.model.clone()), ..Default::default() })?;
                                    members.push(m);
                                }
                            }
                        }
                        Ok((dot, members))
                    })
                    .await?;
                self.events.emit("dots.changed", json!({ "id": id, "action": "update" }));
                for m in members {
                    self.events.emit("dots.changed", json!({ "id": m, "action": "update" }));
                }
                Ok(json!(dot))
            }
            "dots.duplicate" => {
                let DuplicateParams { id, name } = p(params)?;
                let ws_root = self.layout.workspaces_dir.clone();
                let dot = self
                    .storage
                    .call(move |s| {
                        let copy = s.dots().duplicate(ACTOR_USER, &id, name, None)?;
                        let path = ws_root.join(&copy.id);
                        std::fs::create_dir_all(&path)?;
                        s.dots().update(
                            ACTOR_USER,
                            &copy.id,
                            DotUpdate { workspace_path: Some(Some(path.to_string_lossy().to_string())), ..Default::default() },
                        )
                    })
                    .await?;
                self.events.emit("dots.changed", json!({ "id": dot.id, "action": "duplicate" }));
                Ok(json!(dot))
            }
            "dots.pause" | "dots.resume" | "dots.archive" => {
                let IdParams { id } = p(params)?;
                let status = match method {
                    "dots.pause" => DotStatus::Paused,
                    "dots.resume" => DotStatus::Active,
                    _ => DotStatus::Archived,
                };
                let id2 = id.clone();
                // Pausar o gerente pausa os funcionarios ativos; retomar o gerente retoma os pausados.
                let (dot, changed_members) = self.storage.call(move |s| {
                    let members = match status {
                        DotStatus::Paused | DotStatus::Active => s.teams().managed_by(&id2)?.map(|t| t.member_ids).unwrap_or_default(),
                        DotStatus::Archived => vec![],
                    };
                    let dot = s.dots().set_status(ACTOR_USER, &id2, status)?;
                    let from = if status == DotStatus::Paused { DotStatus::Active } else { DotStatus::Paused };
                    let mut changed_members = vec![];
                    for member_id in members {
                        if s.dots().get(&member_id)?.status == from {
                            s.dots().set_status(ACTOR_USER, &member_id, status)?;
                            changed_members.push(member_id);
                        }
                    }
                    Ok((dot, changed_members))
                }).await?;
                self.events.emit("dots.changed", json!({ "id": id, "action": status.as_str() }));
                for member_id in changed_members {
                    self.events.emit("dots.changed", json!({ "id": member_id, "action": status.as_str() }));
                }
                Ok(json!(dot))
            }
            "dots.delete" => {
                let IdParams { id } = p(params)?;
                let id2 = id.clone();
                self.storage.call(move |s| s.dots().delete(ACTOR_USER, &id2)).await?;
                self.browsers.close(&id).await;
                let _ = std::fs::remove_dir_all(self.layout.browsers_dir.join(&id));
                self.events.emit("dots.changed", json!({ "id": id, "action": "delete" }));
                Ok(json!({ "ok": true }))
            }

            "engines.list" => Ok(json!(self.storage.call(|s| s.engines().list()).await?)),
            "engines.detect" => {
                let DetectParams { kind } = p(params).unwrap_or_default();
                Ok(json!(self.detect_engines(kind).await?))
            }
            "engines.set_override" => {
                let OverrideParams { kind, path } = p(params)?;
                let cfg = self.storage.call(move |s| s.engines().set_executable_override(kind, path.as_deref())).await?;
                self.audit("engine.set_override", "engine", Some(kind.as_str()), json!({ "path": cfg.executable_override })).await?;
                let det = self.detect_engines(Some(kind)).await?;
                Ok(json!(det))
            }
            "engines.set_enabled" => {
                let EnabledParams { kind, enabled } = p(params)?;
                let cfg = self.storage.call(move |s| s.engines().set_enabled(kind, enabled)).await?;
                self.audit("engine.set_enabled", "engine", Some(kind.as_str()), json!({ "enabled": enabled })).await?;
                self.events.emit("engines.changed", json!({ "kind": kind }));
                Ok(json!(cfg))
            }
            "engines.set_model" => {
                let ModelParams { kind, model } = p(params)?;
                let cfg = self.storage.call(move |s| s.engines().set_model(kind, model.as_deref())).await?;
                self.audit("engine.set_model", "engine", Some(kind.as_str()), json!({ "model": cfg.model })).await?;
                self.events.emit("engines.changed", json!({ "kind": kind }));
                Ok(json!(cfg))
            }

            // ---- Fase 2 ----
            "gateway.call" => {
                let GatewayCallParams { token, tool, arguments } = p(params)?;
                if let Some(ctx) = self.registry.context_by_token(&token) {
                    ctx.gateway_seen.store(true, Ordering::SeqCst);
                }
                let out = self.gateway_call(&token, &tool, arguments).await?;
                Ok(out.to_mcp_result())
            }
            "tasks.list" => {
                let TaskListParams { dot_id, status, conversation_id, limit } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.tasks().list(&TaskFilter { dot_id, status, conversation_id, limit })).await?))
            }
            "tasks.get" => {
                let IdParams { id } = p(params)?;
                let task = self.storage.call({ let id = id.clone(); move |s| s.tasks().get(&id) }).await?;
                let execs = self.storage.call(move |s| s.executions().list_for_task(&id)).await?;
                Ok(json!({ "task": task, "executions": execs }))
            }
            "tasks.create" => {
                let input: TaskCreate = p(params)?;
                let task = self.create_task(input).await?;
                Ok(json!(task))
            }
            "tasks.cancel" => {
                let IdParams { id } = p(params)?;
                self.cancel_task(&id).await?;
                self.audit("task.cancel", "task", Some(&id), json!({})).await?;
                Ok(json!({ "ok": true }))
            }
            "tasks.children" => {
                let IdParams { id } = p(params)?;
                let (children, delegations, checkpoints) = self.storage.call(move |s| Ok((s.tasks().children(&id)?, s.delegations().for_parent(&id)?, s.checkpoints().list(&id)?))).await?;
                Ok(json!({ "children": children, "delegations": delegations, "checkpoints": checkpoints }))
            }
            "events.list" => {
                let EventListParams { event_type, limit } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.events().list(event_type.as_deref(), limit)).await?))
            }
            "subscriptions.list" => {
                let DotIdParams { dot_id } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.subscriptions().list(dot_id.as_deref())).await?))
            }
            "subscriptions.create" => {
                let input: SubscriptionCreate = p(params)?;
                let sub = self.storage.call(move |s| s.subscriptions().create(input)).await?;
                self.audit("subscription.create", "dot", Some(&sub.dot_id), json!({ "id": sub.id, "type": sub.event_type })).await?;
                self.refresh_watcher().await;
                self.events.emit("automation.changed", json!({ "dot_id": sub.dot_id }));
                Ok(json!(sub))
            }
            "subscriptions.update" => {
                let SubUpdateParams { id, patch } = p(params)?;
                let sub = self.storage.call(move |s| s.subscriptions().update(&id, patch)).await?;
                self.refresh_watcher().await;
                self.events.emit("automation.changed", json!({ "dot_id": sub.dot_id }));
                Ok(json!(sub))
            }
            "subscriptions.delete" => {
                let IdParams { id } = p(params)?;
                let sub = self.storage.call({ let id = id.clone(); move |s| { let sub = s.subscriptions().get(&id)?; s.subscriptions().delete(&id)?; Ok(sub) } }).await?;
                self.audit("subscription.delete", "dot", Some(&sub.dot_id), json!({ "id": id })).await?;
                self.refresh_watcher().await;
                self.events.emit("automation.changed", json!({ "dot_id": sub.dot_id }));
                Ok(json!({ "ok": true }))
            }
            "goals.list" => {
                let DotIdParams { dot_id } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.goals().list(dot_id.as_deref(), false)).await?))
            }
            "goals.history" => {
                let IdParams { id } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.goals().history(&id, 100)).await?))
            }
            "schedules.list" => {
                let DotIdParams { dot_id } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.schedules().list(dot_id.as_deref())).await?))
            }
            "schedules.create" => {
                let input: ScheduleCreate = p(params)?;
                let next = crate::schedules::next_run(input.kind, &input.spec, &dots_core::now())?;
                let sch = self.storage.call(move |s| s.schedules().create(input, Some(&next))).await?;
                self.audit("schedule.create", "dot", Some(&sch.dot_id), json!({ "id": sch.id, "kind": sch.kind.as_str(), "spec": sch.spec })).await?;
                self.schedules_notify.notify_one();
                self.events.emit("automation.changed", json!({ "dot_id": sch.dot_id }));
                Ok(json!(sch))
            }
            "schedules.update" => {
                let SchUpdateParams { id, patch } = p(params)?;
                let cur = self.storage.call({ let id = id.clone(); move |s| s.schedules().get(&id) }).await?;
                let kind = patch.kind.unwrap_or(cur.kind);
                let spec = patch.spec.clone().unwrap_or(cur.spec.clone());
                let next = crate::schedules::next_run(kind, &spec, &dots_core::now())?;
                let sch = self.storage.call(move |s| s.schedules().update(&id, patch, Some(&next))).await?;
                self.schedules_notify.notify_one();
                self.events.emit("automation.changed", json!({ "dot_id": sch.dot_id }));
                Ok(json!(sch))
            }
            "schedules.delete" => {
                let IdParams { id } = p(params)?;
                let sch = self.storage.call({ let id = id.clone(); move |s| { let sch = s.schedules().get(&id)?; s.schedules().delete(&id)?; Ok(sch) } }).await?;
                self.audit("schedule.delete", "dot", Some(&sch.dot_id), json!({ "id": id })).await?;
                self.schedules_notify.notify_one();
                self.events.emit("automation.changed", json!({ "dot_id": sch.dot_id }));
                Ok(json!({ "ok": true }))
            }
            "delegations.list" => {
                let LimitParams { limit } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.delegations().list_recent(limit)).await?))
            }
            "stats.get" => Ok(json!(self.stats().await?)),
            "performance.summary" => {
                let DaysParams { days } = p(params).unwrap_or_default();
                self.performance_summary(days).await
            }
            "runtime.logs" => {
                let LinesParams { lines } = p(params).unwrap_or_default();
                Ok(json!(self.tail_log(lines as usize)))
            }
            "tasks.retry" => {
                let IdParams { id } = p(params)?;
                let task = self.storage.call({ let id = id.clone(); move |s| s.tasks().requeue(&id) }).await?;
                self.events.emit("task.changed", json!({ "id": id, "dot_id": task.dot_id, "status": "QUEUED" }));
                self.scheduler_notify.notify_one();
                Ok(json!(task))
            }
            "executions.get" => {
                let IdParams { id } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.executions().get(&id)).await?))
            }
            "executions.recent" => {
                let LimitParams { limit } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.executions().list_recent(limit.min(500))).await?))
            }
            "executions.in_period" => {
                let ExecutionPeriodParams { start_at, end_at } = p(params)?;
                if start_at >= end_at {
                    return Err(DotsError::Validation("periodo de execucoes invalido".into()));
                }
                Ok(json!(self.storage.call(move |s| s.executions().list_overlapping(&start_at, &end_at)).await?))
            }
            "executions.tool_calls_in_period" => {
                let ExecutionPeriodParams { start_at, end_at } = p(params)?;
                if start_at >= end_at {
                    return Err(DotsError::Validation("periodo de execucoes invalido".into()));
                }
                Ok(json!(self.storage.call(move |s| s.executions().tool_calls_by_dot(&start_at, &end_at)).await?))
            }
            "executions.observed_activity_in_period" => {
                let ExecutionPeriodParams { start_at, end_at } = p(params)?;
                if start_at >= end_at {
                    return Err(DotsError::Validation("periodo de execucoes invalido".into()));
                }
                Ok(json!(self.storage.call(move |s| s.executions().observed_activity(&start_at, &end_at)).await?))
            }
            "executions.events" => {
                let EventsParams { execution_id, after_seq, limit } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.executions().events(&execution_id, after_seq, limit.min(5000))).await?))
            }
            "executions.changes" => {
                let IdParams { id } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.changes().list(Some(&id), None, 500)).await?))
            }
            "runtime.pause_all" => {
                let PauseParams { paused } = p(params)?;
                self.paused.store(paused, Ordering::SeqCst);
                if !paused {
                    self.scheduler_notify.notify_one();
                }
                self.audit(if paused { "runtime.pause_all" } else { "runtime.resume_all" }, "runtime", None, json!({})).await?;
                self.events.emit("runtime.paused", json!({ "paused": paused }));
                Ok(json!({ "paused": paused }))
            }
            "contacts.list" => {
                let role_names = self.flexi_role_names();
                let contacts = self.storage.call(move |s| {
                    let dots = s.dots().list(false)?;
                    let active = s.tasks().list(&TaskFilter { status: Some(vec![TaskStatus::Queued, TaskStatus::Running, TaskStatus::WaitingApproval, TaskStatus::WaitingUser]), limit: 1000, ..Default::default() })?;
                    let pending = s.approvals().list(true, 500)?;
                    // Pedidos com tarefa ativa de decisao no gerente nao exigem acao imediata do usuario.
                    let routed: std::collections::HashSet<String> = pending
                        .iter()
                        .filter(|a| active.iter().any(|t| t.origin == format!("{}:{}", if a.kind == dots_core::ApprovalKind::Question { "question" } else { "approval" }, a.id)))
                        .map(|a| a.id.clone())
                        .collect();
                    let mut out = vec![];
                    for dot in dots {
                        let conv = s.conversations().main(&dot.id)?;
                        let mine: Vec<&dots_core::Task> = active.iter().filter(|t| t.dot_id == dot.id).collect();
                        let reqs = pending.iter().filter(|a| a.dot_id == dot.id && !routed.contains(&a.id)).count() as u32;
                        let asked_manager = pending.iter().find(|a| a.dot_id == dot.id && routed.contains(&a.id)).map(|a| a.kind == dots_core::ApprovalKind::Question);
                        let last_status = s.tasks().list(&TaskFilter { dot_id: Some(dot.id.clone()), limit: 1, ..Default::default() })?.first().map(|t| t.status);
                        let last_failed = last_status == Some(TaskStatus::Failed);
                        let (state, text) = if dot.status == DotStatus::Paused {
                            let queued = mine.iter().filter(|t| t.status == TaskStatus::Queued).count();
                            let text = if mine.iter().any(|t| t.status == TaskStatus::Running) {
                                "Pausado · tarefa atual em andamento".to_string()
                            } else if reqs > 0 || mine.iter().any(|t| t.status == TaskStatus::WaitingUser || t.status == TaskStatus::WaitingApproval) {
                                "Pausado · aguardando resposta".to_string()
                            } else if queued > 0 {
                                format!("Pausado · {queued} na fila")
                            } else {
                                "Pausado".to_string()
                            };
                            ("idle", text)
                        } else if reqs > 0 {
                            ("attention", if pending.iter().any(|a| a.dot_id == dot.id && !routed.contains(&a.id) && a.kind == dots_core::ApprovalKind::Question) { "Fez uma pergunta" } else { "Pede sua permissao" }.to_string())
                        } else if let Some(is_question) = asked_manager {
                            ("busy", if is_question { "Perguntou ao gerente" } else { "Pediu permissao ao gerente" }.to_string())
                        } else if let Some(t) = mine.iter().find(|t| t.status == TaskStatus::WaitingUser) {
                            ("attention", format!("Precisa de voce: {}", t.title))
                        } else if let Some(t) = mine.iter().find(|t| t.status == TaskStatus::Running) {
                            ("busy", format!("Trabalhando: {}", t.title))
                        } else if !mine.is_empty() {
                            ("busy", format!("{} na fila", mine.len()))
                        } else {
                            ("idle", "Livre".to_string())
                        };
                        // Sem papel de origem, o cargo de um funcionario de equipe e a funcao dele (a que o gerente definiu ao contratar).
                        let title = match s.apps().integration(&dot.id, crate::flexi::KIND)?.and_then(|i| i.role_template).and_then(|t| role_names.get(&t).cloned()) {
                            Some(t) => Some(t),
                            None if !dot.description.trim().is_empty() && s.teams().member_of(&dot.id)?.is_some() => Some(dot.description.trim().to_string()),
                            None => None,
                        };
                        // Batimento (como no ovrgit): trabalhando = firme; esperando alguem = lento; na fila = em espera; falhou = linha reta.
                        let pulse = if dot.status == DotStatus::Paused {
                            if mine.iter().any(|t| t.status == TaskStatus::Running) { "live" } else { "idle" }
                        } else if reqs > 0 || asked_manager.is_some() || mine.iter().any(|t| t.status == TaskStatus::WaitingUser || t.status == TaskStatus::WaitingApproval) {
                            "waiting"
                        } else if mine.iter().any(|t| t.status == TaskStatus::Running) {
                            "live"
                        } else if !mine.is_empty() {
                            "standby"
                        } else if dot.status == DotStatus::Active && last_failed {
                            "error"
                        } else {
                            "idle"
                        };
                        out.push(dots_core::Contact { conversation_id: conv.id, last_message: conv.last_message, last_role: conv.last_role, last_at: conv.updated_at, state: state.into(), state_text: text, pending_requests: reqs, title, pulse: pulse.into(), done: last_status == Some(TaskStatus::Completed), dot });
                    }
                    out.sort_by(|a, b| b.last_at.cmp(&a.last_at));
                    Ok(out)
                }).await?;
                Ok(json!(contacts))
            }
            "conversations.main" => {
                let DotIdParams { dot_id } = p(params)?;
                let dot_id = dot_id.ok_or_else(|| DotsError::Validation("dot_id obrigatorio".into()))?;
                Ok(json!(self.storage.call(move |s| s.conversations().main(&dot_id)).await?))
            }
            "conversations.list" => {
                let DotIdParams { dot_id } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.conversations().list(dot_id.as_deref())).await?))
            }
            "conversations.create" => {
                let ConvCreateParams { dot_id, title } = p(params)?;
                let conv = self.storage.call(move |s| s.conversations().create(&dot_id, title.as_deref().unwrap_or("Nova conversa"))).await?;
                self.events.emit("conversation.changed", json!({ "id": conv.id, "dot_id": conv.dot_id }));
                Ok(json!(conv))
            }
            "conversations.get" => {
                let IdParams { id } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.conversations().get(&id)).await?))
            }
            "conversations.delete" => {
                let IdParams { id } = p(params)?;
                self.storage.call({ let id = id.clone(); move |s| s.conversations().delete(&id) }).await?;
                self.events.emit("conversation.changed", json!({ "id": id, "deleted": true }));
                Ok(json!({ "ok": true }))
            }
            "conversations.messages" => {
                let EventsParamsLite { id, limit } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.conversations().messages(&id, limit)).await?))
            }
            "conversations.send" => {
                let ConvSendParams { conversation_id, content, attachments } = p(params)?;
                let (msg, task) = self.send_conversation_message(&conversation_id, content, attachments, json!({})).await?;
                Ok(json!({ "message": msg, "task": task }))
            }
            "conversations.attach" => {
                let ConvAttachParams { conversation_id, name, path, data } = p(params)?;
                let conv = self.storage.call({ let c = conversation_id.clone(); move |s| s.conversations().get(&c) }).await?;
                let (dot, dirs) = self.storage.call({ let d = conv.dot_id.clone(); move |s| Ok((s.dots().get(&d)?, s.dots().workspaces(&d)?)) }).await?;
                let workspace = PathBuf::from(dot.workspace_path.ok_or_else(|| DotsError::Validation("o DOT nao tem workspace".into()))?);
                let readable: Vec<PathBuf> = std::iter::once(workspace.clone()).chain(dirs.into_iter().filter(|w| w.can_read).map(|w| PathBuf::from(w.path))).collect();
                Ok(save_attachment(&workspace, &readable, &name, path.as_deref(), data.as_deref())?)
            }
            "approvals.list" => {
                let ApprovalListParams { pending_only, limit } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.approvals().list(pending_only, limit)).await?))
            }
            "notifications.list" => {
                let ApprovalListParams { pending_only, limit } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.notifications().list(pending_only, limit)).await?))
            }
            "notifications.clear" => {
                let NotificationClearParams { id } = p(params).unwrap_or_default();
                let cleared = self.storage.call(move |s| s.notifications().clear(id.as_deref())).await?;
                if cleared > 0 {
                    self.events.emit("notification.changed", json!({ "cleared": cleared }));
                }
                Ok(json!({ "cleared": cleared }))
            }
            "approvals.decide" => {
                let DecideParams { id, approved, reason } = p(params)?;
                Ok(json!(self.decide_approval(&id, approved, reason).await?))
            }
            "browser.status" => {
                let DotIdParams { dot_id } = p(params)?;
                Ok(self.browser_status(&dot_id.ok_or_else(|| DotsError::Validation("dot_id obrigatorio".into()))?).await)
            }
            "browser.watch" => {
                let DotIdParams { dot_id } = p(params)?;
                self.browser_watch(&dot_id.ok_or_else(|| DotsError::Validation("dot_id obrigatorio".into()))?).await
            }
            "browser.unwatch" => {
                let DotIdParams { dot_id } = p(params)?;
                Ok(self.browser_unwatch(&dot_id.ok_or_else(|| DotsError::Validation("dot_id obrigatorio".into()))?))
            }
            "memories.list" => {
                let MemListParams { dot_id, kind, query, limit } = p(params)?;
                Ok(json!(self.storage.call(move |s| match query.filter(|q| !q.trim().is_empty()) {
                    Some(q) => s.memories().search(&dot_id, &q, &kind.map(|k| vec![k]).unwrap_or_default(), if limit == 0 { 50 } else { limit }),
                    None => s.memories().list(&dot_id, kind, limit),
                }).await?))
            }
            "memories.create" => {
                let input: MemoryCreate = p(params)?;
                let dot_id = input.dot_id.clone();
                let m = self.storage.call(move |s| s.memories().create(input)).await?;
                self.audit("memory.create", "memory", Some(&m.id), json!({ "dot_id": dot_id })).await?;
                self.events.emit("memory.changed", json!({ "dot_id": dot_id }));
                Ok(json!(m))
            }
            "memories.update" => {
                let MemUpdateParams { id, patch } = p(params)?;
                let m = self.storage.call(move |s| s.memories().update(&id, patch)).await?;
                self.events.emit("memory.changed", json!({ "dot_id": m.dot_id }));
                Ok(json!(m))
            }
            "memories.delete" => {
                let IdParams { id } = p(params)?;
                let m = self.storage.call({ let id = id.clone(); move |s| { let m = s.memories().get(&id)?; s.memories().delete(&id)?; Ok(m) } }).await?;
                self.audit("memory.delete", "memory", Some(&id), json!({ "dot_id": m.dot_id })).await?;
                self.events.emit("memory.changed", json!({ "dot_id": m.dot_id }));
                Ok(json!({ "ok": true }))
            }
            "policies.get" => {
                let DotIdParams { dot_id } = p(params)?;
                let dot_id = dot_id.ok_or_else(|| DotsError::Validation("dot_id obrigatorio".into()))?;
                Ok(json!(self.storage.call(move |s| s.policies().get(&dot_id)).await?))
            }
            "policies.set" => {
                let PolicySetParams { dot_id, patch } = p(params)?;
                let pol = self.storage.call({ let d = dot_id.clone(); move |s| s.policies().update(&d, patch) }).await?;
                self.audit("policy.update", "dot", Some(&dot_id), json!(pol)).await?;
                self.events.emit("dots.changed", json!({ "id": dot_id, "action": "policy" }));
                Ok(json!(pol))
            }
            "policies.learned" => {
                let DotIdParams { dot_id } = p(params)?;
                let dot_id = dot_id.ok_or_else(|| DotsError::Validation("dot_id obrigatorio".into()))?;
                Ok(json!(self.storage.call(move |s| s.learned().list(&dot_id)).await?))
            }
            "policies.forget" => {
                let IdParams { id } = p(params)?;
                let dot = self.storage.call({ let id = id.clone(); move |s| s.learned().forget(&id) }).await?;
                self.audit("policy.forget_learned", "dot", dot.as_deref(), json!({ "id": id })).await?;
                self.events.emit("learned.changed", json!({ "dot_id": dot }));
                Ok(json!({ "ok": true }))
            }
            "workspaces.add" => {
                let WorkspaceParams { dot_id, path, can_read, can_write } = p(params)?;
                let pb = PathBuf::from(&path);
                if !pb.is_absolute() || !pb.is_dir() {
                    return Err(DotsError::Validation("informe um diretorio absoluto existente".into()));
                }
                let ws = self.storage.call({ let d = dot_id.clone(); move |s| s.dots().add_workspace(ACTOR_USER, &d, &path, can_read, can_write) }).await?;
                self.refresh_watcher().await;
                self.events.emit("dots.changed", json!({ "id": dot_id, "action": "workspace" }));
                Ok(json!(ws))
            }
            "workspaces.remove" => {
                let WorkspaceParams { dot_id, path, .. } = p(params)?;
                self.storage.call({ let d = dot_id.clone(); move |s| s.dots().remove_workspace(ACTOR_USER, &d, &path) }).await?;
                self.events.emit("dots.changed", json!({ "id": dot_id, "action": "workspace" }));
                Ok(json!({ "ok": true }))
            }
            "engines.agy_status" | "engines.agy_setup" | "engines.agy_remove" => {
                let cfg = self.storage.call(|s| s.engines().get(EngineKind::Agy)).await?;
                let agy = cfg
                    .and_then(|c| c.executable_override.or_else(|| c.last_detection.and_then(|d| d.executable_path)))
                    .ok_or_else(|| DotsError::Engine("AGY nao detectado".into()))?;
                let agy = PathBuf::from(agy);
                let st = match method {
                    "engines.agy_setup" => {
                        let r = crate::agy_setup::apply(&agy, &self.home, &self.path_dirs, &self.dotsd_path, &self.layout.root).await?;
                        self.audit("engine.agy_setup", "engine", Some("agy"), json!({ "settings": r.settings_path })).await?;
                        r
                    }
                    "engines.agy_remove" => {
                        let r = crate::agy_setup::remove(&agy, &self.home, &self.path_dirs).await?;
                        self.audit("engine.agy_remove", "engine", Some("agy"), json!({})).await?;
                        r
                    }
                    _ => crate::agy_setup::status(&agy, &self.home, &self.path_dirs).await?,
                };
                Ok(json!(st))
            }
            // ---- Fase Flexi ----
            "apps.list" => Ok(json!(self.storage.call(|s| s.apps().list(None)).await?)),
            "apps.flexi.connect" => {
                let input: dots_core::FlexiConnectInput = p(params)?;
                Ok(json!(self.flexi_connect(input, None).await?))
            }
            "apps.flexi.reconnect" => {
                let ReconnectParams { id, senha, remember_password } = p(params)?;
                let conn = self.storage.call({ let id = id.clone(); move |s| s.apps().get(&id) }).await?;
                match senha.filter(|s| !s.is_empty()) {
                    Some(senha) => Ok(json!(self.flexi_connect(dots_core::FlexiConnectInput { label: None, base_url: Some(conn.base_url.clone()), server: conn.server.clone(), email: conn.email.clone(), senha, remember_password: remember_password.unwrap_or(conn.remember_password) }, Some(id)).await?)),
                    None => Ok(json!(self.flexi_check(&id).await?)),
                }
            }
            "apps.flexi.mfa" => {
                let MfaParams { id, code } = p(params)?;
                Ok(json!(self.flexi_verify_mfa(&id, &code).await?))
            }
            "apps.flexi.check" => {
                let IdParams { id } = p(params)?;
                Ok(json!(self.flexi_check(&id).await?))
            }
            "apps.flexi.remove" => {
                let IdParams { id } = p(params)?;
                self.flexi_remove(&id).await?;
                Ok(json!({ "ok": true }))
            }
            "apps.flexi.browser_start" => {
                let base_url = params.get("base_url").and_then(|v| v.as_str()).map(String::from);
                self.flexi_browser_login_start(base_url).await
            }
            "apps.flexi.browser_poll" => {
                let token = params.get("token").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                self.flexi_browser_login_poll(&token).await
            }
            "apps.flexi.browser_cancel" => {
                let token = params.get("token").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                self.flexi_browser_login_cancel(&token).await;
                Ok(json!({ "ok": true }))
            }
            "apps.flexi.photo" => {
                let IdParams { id } = p(params)?;
                self.flexi_profile_photo(&id).await
            }
            "apps.flexi.templates" => Ok(json!(self.flexi_templates(None).await)),
            "apps.flexi.link_status" => Ok(json!(self.flexi_link.status())),
            "apps.flexi.users" => Ok(json!(self.flexi_users().await?)),
            "apps.flexi.link_reconnect" => {
                // Sincroniza com o Flexi web na hora; a tarefa de fundo ouve o evento.
                self.events.emit(crate::flexi_sync::RECONNECT_EVENT, json!({}));
                self.audit("apps.flexi.link_reconnect", "app", None, json!({})).await?;
                Ok(json!(self.flexi_link.status()))
            }
            "apps.flexi.create_team" => {
                let IdParams { id } = p(params)?;
                Ok(self.flexi_create_team(&id).await?)
            }
            "dots.integration" => {
                let IntegrationParams { dot_id, kind, .. } = p(params)?;
                Ok(json!(self.storage.call(move |s| s.apps().integration(&dot_id, &kind)).await?))
            }
            "dots.set_integration" => {
                let IntegrationParams { dot_id, kind, connection_id } = p(params)?;
                let r = self.storage.call({ let (d, k) = (dot_id.clone(), kind.clone()); move |s| s.apps().set_integration(&d, &k, connection_id.as_deref(), None, None) }).await?;
                self.audit("dot.integration", "dot", Some(&dot_id), json!({ "kind": kind, "connection_id": r.as_ref().map(|i| i.connection_id.clone()) })).await?;
                self.events.emit("dots.changed", json!({ "id": dot_id, "action": "integration" }));
                Ok(json!(r))
            }
            m if m.starts_with("teams.") || m == "dots.team" => {
                self.handle_teams(m, params).await?.ok_or_else(|| DotsError::NotFound(format!("metodo {m}")))
            }
            m if m.starts_with("dismissals.") => {
                self.handle_dismissals(m, params).await?.ok_or_else(|| DotsError::NotFound(format!("metodo {m}")))
            }
            m if m.starts_with("group.") => {
                self.handle_group(m, params).await?.ok_or_else(|| DotsError::NotFound(format!("metodo {m}")))
            }
            "usage.get" => {
                let ForceParams { force } = p(params).unwrap_or_default();
                Ok(json!(self.usage(force).await))
            }
            "cli.updates" => {
                let ForceParams { force } = p(params).unwrap_or_default();
                Ok(json!(self.cli_updates(force).await))
            }
            "cli.update" => {
                let KindParams { kind } = p(params)?;
                let after = self.cli_update(kind).await?;
                self.audit("cli.update", "engine", Some(kind.as_str()), json!({ "version": after.as_ref().map(|a| a.current.clone()) })).await?;
                Ok(json!(after))
            }
            "cli.install" => {
                let KindParams { kind } = p(params)?;
                let after = self.cli_install(kind).await?;
                self.audit("cli.install", "engine", Some(kind.as_str()), json!({ "version": after.as_ref().map(|a| a.current.clone()) })).await?;
                Ok(json!(after))
            }
            "apps.flexi.assign_role" => {
                let AssignRoleParams { dot_id, connection_id, template_id, group, role } = p(params)?;
                let r = self.flexi_assign_role(&dot_id, &connection_id, template_id.as_deref(), group.as_deref(), role).await?;
                self.events.emit("dots.changed", json!({ "id": dot_id, "action": "integration" }));
                Ok(json!(r))
            }
            "gateway.tools" => {
                let TokenParams { token } = p(params)?;
                let ctx = self.registry.context_by_token(&token).ok_or_else(|| DotsError::Unauthorized("token de execucao invalido".into()))?;
                let mut tools = crate::tools::tool_definitions();
                tools.extend(self.flexi_tools_for_dot(&ctx.dot_id).await?);
                crate::tools::add_approval_assessment_schema(&mut tools);
                ctx.gateway_seen.store(true, Ordering::SeqCst);
                Ok(json!({ "tools": tools }))
            }
            "audit.list" => {
                let LimitParams { limit } = p(params).unwrap_or_default();
                Ok(json!(self.storage.call(move |s| s.audit().list_recent(limit.min(1000))).await?))
            }
            _ => Err(DotsError::Protocol(format!("metodo desconhecido: {method}"))),
        }
    }

    /// Enfileira uma tarefa e acorda o scheduler.
    /// Mensagem do usuario numa conversa: grava, abre a tarefa de chat e avisa a interface. `extra_meta` entra nos
    /// metadados da mensagem (ex.: origem no Flexi web) e recebe `task_id` depois que a tarefa existe.
    pub async fn send_conversation_message(&self, conversation_id: &str, content: String, attachments: Vec<String>, extra_meta: Value) -> DotsResult<(dots_core::Message, dots_core::Task)> {
        let conversation_id = conversation_id.to_string();
        let conv = self.storage.call({ let c = conversation_id.clone(); move |s| s.conversations().get(&c) }).await?;
        let mut meta = match extra_meta { Value::Object(m) => Value::Object(m), _ => json!({}) };
        if !attachments.is_empty() {
            meta["attachments"] = json!(attachments);
        }
        let msg = self.storage.call({ let (c, t, m) = (conversation_id.clone(), content.clone(), meta.clone()); move |s| s.conversations().add_message(&c, MessageRole::User, &t, None, None, m) }).await?;
        if conv.message_count == 0 {
            let (c, title) = (conversation_id.clone(), content.chars().take(60).collect::<String>());
            let _ = self.storage.call(move |s| s.conversations().set_title(&c, &title)).await;
        }
        let task = self
            .create_task(TaskCreate { dot_id: conv.dot_id.clone(), prompt: content.clone(), kind: Some(TaskKind::Chat), conversation_id: Some(conversation_id.clone()), attachments, ..Default::default() })
            .await?;
        if meta.as_object().is_some_and(|m| !m.is_empty()) {
            meta["task_id"] = json!(task.id);
            let _ = self.storage.call({ let (m, c, meta) = (msg.id.clone(), content, meta); move |s| s.conversations().update_message_content(&m, &c, Some(meta)) }).await;
        }
        self.events.emit("message.changed", json!({ "conversation_id": conversation_id, "message_id": msg.id }));
        let _ = self.emit_event(et::USER_MESSAGE, "user", json!({ "dot_id": conv.dot_id, "conversation_id": conversation_id, "task_id": task.id, "message_id": msg.id }), None, Some(format!("msg:{}", msg.id)), 0).await;
        Ok((msg, task))
    }

    pub async fn create_task(&self, input: TaskCreate) -> DotsResult<dots_core::Task> {
        self.create_task_with_depth(input, 0).await
    }

    /// `event_depth` e a profundidade de encadeamento de eventos que originou a tarefa.
    pub async fn create_task_with_depth(&self, mut input: TaskCreate, event_depth: u32) -> DotsResult<dots_core::Task> {
        let dot = self.storage.call({ let d = input.dot_id.clone(); move |s| s.dots().get(&d) }).await?;
        if dot.status == DotStatus::Archived {
            return Err(DotsError::Validation("DOT arquivado nao recebe tarefas".into()));
        }
        // Tudo acontece na conversa do DOT: trabalhos sem conversa entram na principal com uma nota de origem.
        if input.conversation_id.is_none() {
            let main = self.storage.call({ let d = input.dot_id.clone(); move |s| s.conversations().main(&d) }).await?;
            let origin = input.origin.clone().unwrap_or_else(|| "user".into());
            let note = match origin.split(':').next().unwrap_or("user") {
                "user" => None,
                "event" => Some(format!("Gatilho: {}", input.title.clone().unwrap_or_default().trim_start_matches("Gatilho: ").to_string())),
                "schedule" => Some(input.title.clone().unwrap_or_else(|| "Horario marcado".into())),
                "delegation" => Some(input.title.clone().unwrap_or_else(|| "Pedido de outro DOT".into())),
                "followup" => Some("Resultados chegaram; continuando".into()),
                "autonomy" => Some("Iniciando o objetivo automaticamente".into()),
                _ => None,
            };
            let (cid, role, content, meta) = (main.id.clone(), if note.is_some() { MessageRole::System } else { MessageRole::User }, note.clone().unwrap_or_else(|| input.prompt.clone()), json!({ "origin": origin, "prompt": input.prompt.chars().take(4000).collect::<String>() }));
            let _ = self.storage.call(move |s| s.conversations().add_message(&cid, role, &content, None, None, meta)).await;
            self.events.emit("message.changed", json!({ "conversation_id": main.id, "dot_id": input.dot_id }));
            input.conversation_id = Some(main.id);
        }
        let engine = dot.primary_engine;
        let task = self.storage.call(move |s| s.tasks().create(input, engine)).await?;
        self.event_depths.lock().unwrap().insert(task.id.clone(), event_depth);
        self.audit("task.create", "task", Some(&task.id), json!({ "dot_id": task.dot_id, "kind": task.kind.as_str(), "origin": task.origin })).await?;
        self.events.emit("task.changed", json!({ "id": task.id, "dot_id": task.dot_id, "status": "QUEUED" }));
        self.scheduler_notify.notify_one();
        Ok(task)
    }

    /// Reconfigura o observador de arquivos conforme gatilhos e pastas atuais.
    pub async fn refresh_watcher(&self) {
        let paths = crate::watcher::compute_paths(self).await;
        if let Ok(mut w) = self.watcher.lock() {
            w.set_paths(paths);
        }
    }

    pub fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            pid: std::process::id(),
            version: crate::VERSION.into(),
            protocol_version: dots_core::PROTOCOL_VERSION,
            started_at: self.started_at,
            uptime_seconds: (dots_core::now() - self.started_at).num_seconds().max(0) as u64,
            data_dir: self.layout.root.to_string_lossy().to_string(),
            socket_path: self.socket_path.to_string_lossy().to_string(),
            schema_version: self.schema_version,
            mode: self.mode,
            connected_clients: self.clients.load(Ordering::SeqCst),
        }
    }

    async fn overview(&self) -> DotsResult<Value> {
        let (counts, engines, task_counts, recent, approvals, running_dots, failures) = self
            .storage
            .call(|s| {
                let counts = s.dots().count_by_status()?;
                let engines = s.engines().list()?;
                let task_counts = s.tasks().count_by_status()?;
                let recent = s.executions().list_recent(10)?;
                let approvals = s.approvals().pending_count()?;
                let running = s.executions().running()?;
                let running_dots = running.iter().map(|e| e.dot_id.clone()).collect::<std::collections::HashSet<_>>().len() as u32;
                let since = dots_core::now() - chrono::Duration::hours(24);
                let failures = s.tasks().list(&TaskFilter { status: Some(vec![TaskStatus::Failed]), limit: 500, ..Default::default() })?.into_iter().filter(|t| t.finished_at.map(|f| f > since).unwrap_or(false)).count() as u32;
                Ok((counts, engines, task_counts, recent, approvals, running_dots, failures))
            })
            .await?;
        let get = |st: DotStatus| counts.iter().find(|(s, _)| *s == st).map(|(_, n)| *n).unwrap_or(0);
        let tget = |st: TaskStatus| task_counts.iter().find(|(s, _)| *s == st).map(|(_, n)| *n).unwrap_or(0);
        Ok(json!({
            "dots_total": counts.iter().map(|(_, n)| n).sum::<u32>(),
            "dots_active": get(DotStatus::Active),
            "dots_paused": get(DotStatus::Paused),
            "dots_archived": get(DotStatus::Archived),
            "dots_running": running_dots,
            "tasks_pending": tget(TaskStatus::Queued) + tget(TaskStatus::WaitingUser) + tget(TaskStatus::WaitingApproval),
            "tasks_running": tget(TaskStatus::Running),
            "executions_recent": recent,
            "failures": failures,
            "approvals_pending": approvals,
            "engines": engines,
            "runtime": self.status(),
            "paused": self.paused.load(Ordering::SeqCst),
        }))
    }

    /// Executa a deteccao (todos ou um motor), persiste e emite `engines.changed`.
    pub async fn detect_engines(&self, only: Option<EngineKind>) -> DotsResult<Vec<dots_core::EngineDetection>> {
        let configs = self.storage.call(|s| s.engines().list()).await?;
        let overrides: Vec<(EngineKind, Option<String>)> =
            configs.iter().map(|c| (c.kind, c.executable_override.clone())).collect();
        let mut opts = DetectOptions::default();
        if let Some(home) = crate::paths::home_dir() {
            opts.dirs = dots_engines::candidate_dirs(&home);
            opts.home = home;
        }
        let results = match only {
            Some(kind) => {
                let ov = overrides.iter().find(|(k, _)| *k == kind).and_then(|(_, p)| p.clone());
                vec![dots_engines::detect_one(kind, ov.as_deref(), &opts).await]
            }
            None => dots_engines::detect_all(&overrides, &opts).await,
        };
        for det in &results {
            let d = det.clone();
            self.storage.call(move |s| s.engines().save_detection(&d)).await?;
            tracing::info!(engine = det.kind.as_str(), installed = det.installed, version = ?det.version, auth = det.auth_status.as_str(), "motor detectado");
        }
        self.events.emit("engines.changed", json!({ "kinds": results.iter().map(|d| d.kind).collect::<Vec<_>>() }));
        Ok(results)
    }

    pub async fn audit(&self, action: &str, entity: &str, entity_id: Option<&str>, details: Value) -> DotsResult<()> {
        let (action, entity, entity_id) = (action.to_string(), entity.to_string(), entity_id.map(String::from));
        self.storage
            .call(move |s| s.audit().record("user", &action, &entity, entity_id.as_deref(), details).map(|_| ()))
            .await
    }
}

/// Tamanho maximo de um anexo da conversa.
const MAX_ATTACHMENT_BYTES: u64 = 50 * 1024 * 1024;

/// Guarda um anexo da conversa onde o DOT pode ler. Arquivo que ja esta numa pasta legivel do DOT fica onde esta;
/// os demais (arrastados de fora, colados) sao copiados para `<workspace>/anexos/`.
fn save_attachment(workspace: &std::path::Path, readable: &[PathBuf], name: &str, path: Option<&str>, data: Option<&str>) -> DotsResult<Value> {
    use base64::Engine as _;
    // So o nome do arquivo, sem caracteres estranhos; nomes longos perdem o comeco (a extensao fica).
    let chars: Vec<char> = std::path::Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' })
        .collect();
    let clean: String = chars[chars.len().saturating_sub(100)..].iter().collect::<String>().trim().to_string();
    let clean = if clean.is_empty() || clean.starts_with('.') { format!("anexo{clean}") } else { clean };
    let bytes = match (path, data) {
        (Some(src), _) => {
            let src = std::fs::canonicalize(src).map_err(|e| DotsError::Validation(format!("arquivo {src}: {e}")))?;
            let meta = std::fs::metadata(&src)?;
            if !meta.is_file() {
                return Err(DotsError::Validation(format!("{} nao e um arquivo", src.display())));
            }
            let inside = readable.iter().any(|d| std::fs::canonicalize(d).map(|d| src.starts_with(d)).unwrap_or(false));
            if inside && !src.components().any(|c| c.as_os_str() == ".dots") {
                return Ok(json!({ "path": src.to_string_lossy(), "name": clean, "size": meta.len() }));
            }
            if meta.len() > MAX_ATTACHMENT_BYTES {
                return Err(DotsError::Validation("anexo maior que 50 MB".into()));
            }
            std::fs::read(&src)?
        }
        (None, Some(b64)) => base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|e| DotsError::Validation(format!("anexo invalido: {e}")))?,
        (None, None) => return Err(DotsError::Validation("informe path ou data".into())),
    };
    if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err(DotsError::Validation("anexo maior que 50 MB".into()));
    }
    let dir = workspace.join("anexos");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut dest = dir.join(format!("{stamp}-{clean}"));
    let mut n = 2;
    while dest.exists() {
        dest = dir.join(format!("{stamp}-{n}-{clean}"));
        n += 1;
    }
    std::fs::write(&dest, &bytes)?;
    Ok(json!({ "path": dest.to_string_lossy(), "name": clean, "size": bytes.len() }))
}

#[cfg(test)]
mod attachment_tests {
    use super::*;

    #[test]
    fn pasted_bytes_go_to_workspace_anexos() {
        let ws = tempfile::tempdir().unwrap();
        let v = save_attachment(ws.path(), &[ws.path().to_path_buf()], "../../etc/print tela?.png", None, Some("aGVsbG8=")).unwrap();
        let p = PathBuf::from(v["path"].as_str().unwrap());
        assert!(p.starts_with(ws.path().join("anexos")), "{p:?}");
        assert!(p.file_name().unwrap().to_string_lossy().ends_with("-print tela_.png"));
        assert_eq!(std::fs::read(&p).unwrap(), b"hello");
    }

    #[test]
    fn readable_file_stays_and_outside_file_is_copied() {
        let ws = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let inner = ws.path().join("nota.txt");
        std::fs::write(&inner, "a").unwrap();
        let v = save_attachment(ws.path(), &[ws.path().to_path_buf()], "nota.txt", Some(inner.to_str().unwrap()), None).unwrap();
        assert_eq!(PathBuf::from(v["path"].as_str().unwrap()), std::fs::canonicalize(&inner).unwrap());
        let ext = outside.path().join("fora.pdf");
        std::fs::write(&ext, "pdf").unwrap();
        let v = save_attachment(ws.path(), &[ws.path().to_path_buf()], "fora.pdf", Some(ext.to_str().unwrap()), None).unwrap();
        let p = PathBuf::from(v["path"].as_str().unwrap());
        assert!(p.starts_with(ws.path().join("anexos")) && p.exists());
        assert!(save_attachment(ws.path(), &[], "x", Some(outside.path().to_str().unwrap()), None).is_err(), "pasta nao e arquivo");
    }
}
