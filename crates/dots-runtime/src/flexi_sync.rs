//! Sincronizacao entre o `dotsd` e o Flexi web por REST (ADR 0016; substitui o canal WebSocket do ADR 0015).
//!
//! O runtime so faz chamadas de saida, com a sessao da conta do usuario (`account.flexi_connection_id`):
//! registra os DOTs compartilhados, busca a cada poucos segundos as mensagens e decisoes pendentes, responde
//! pela conversa unica de cada DOT (tipo grupo) e devolve respostas, aprovacoes e desempenho. Nada entra na
//! maquina: funciona num computador Ubuntu ligado 24 h sem porta aberta.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dots_core::{AppConnection, Dot, DotStatus, DotsError, DotsResult, MessageRole, TaskStatus, Timestamp};
use dots_flexi::{FlexiClient, FlexiError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::flexi::ACCOUNT_CONNECTION_SETTING;
use crate::service::RuntimeService;

/// Prefixo das rotas que o Flexi expoe ao app (autenticadas pelo cookie da conta do usuario).
pub const API_PREFIX: &str = "/api/agentes-dots/app";
const POLL_INTERVAL: Duration = Duration::from_secs(4);
const MIN_CYCLE_GAP: Duration = Duration::from_millis(700);
const IDLE_RETRY: Duration = Duration::from_secs(30);
const BACKOFF_MIN: u64 = 5;
const BACKOFF_MAX: u64 = 60;
/// Desempenho vai a cada 5 min, ou 60 s depois de uma tarefa terminar.
const PERFORMANCE_INTERVAL: Duration = Duration::from_secs(300);
const PERFORMANCE_DEBOUNCE: Duration = Duration::from_secs(60);
const APPROVALS_LIMIT: u32 = 100;
const CONVERSATION_SCAN: u32 = 200;
/// Evento interno (IPC `apps.flexi.link_reconnect`): sincroniza na hora.
pub const RECONNECT_EVENT: &str = "flexi_link.reconnect";
const REGISTRY_EVENTS: [&str; 2] = ["dots.changed", "teams.changed"];
const APPROVAL_EVENTS: [&str; 2] = ["approval.requested", "approval.resolved"];

/// Estado da sincronizacao, em memoria, para a interface do app (`apps.flexi.link_status`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LinkStatus {
    pub connected: bool,
    pub since: Option<Timestamp>,
    pub last_sync: Option<Timestamp>,
    pub last_error: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Default)]
pub struct LinkState {
    inner: std::sync::Mutex<LinkStatus>,
}

impl LinkState {
    pub fn status(&self) -> LinkStatus {
        self.inner.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// Mensagem que um usuario do Flexi deixou para um DOT compartilhado.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct InboxMessage {
    pub id: i64,
    pub agente_id: String,
    #[serde(default)]
    pub usuario_id: Option<i64>,
    #[serde(default)]
    pub autor_nome: String,
    #[serde(default)]
    pub conteudo: String,
    #[serde(default)]
    pub estado: String,
}

/// Decisao de aprovacao tomada no Flexi web, ainda nao aplicada aqui.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Decision {
    pub id: String,
    pub aprovado: bool,
    #[serde(default)]
    pub motivo: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SyncResponse {
    #[serde(default)]
    mensagens: Vec<InboxMessage>,
    #[serde(default)]
    decisoes: Vec<Decision>,
}

/// Mensagem em andamento: a tarefa que responde a mensagem `id` do Flexi.
#[derive(Debug, Clone)]
struct InFlight {
    task_id: String,
    conversation_id: String,
    agente_id: String,
    /// Ultimo (conteudo, estado, situacao) enviado ao Flexi, para nao repetir.
    last_pushed: Option<(String, String, String)>,
}

#[derive(Default)]
struct SyncState {
    started: bool,
    registry_dirty: bool,
    approvals_dirty: bool,
    perf_dirty: bool,
    last_perf_push: Option<tokio::time::Instant>,
    inflight: HashMap<i64, InFlight>,
}

enum SyncError {
    Unauthorized(String),
    Failed(String),
}

impl From<FlexiError> for SyncError {
    fn from(e: FlexiError) -> Self {
        match e {
            FlexiError::Unauthenticated(m) => SyncError::Unauthorized(m),
            other => SyncError::Failed(other.to_string()),
        }
    }
}

impl From<DotsError> for SyncError {
    fn from(e: DotsError) -> Self {
        SyncError::Failed(e.to_string())
    }
}

/// Texto que o DOT recebe: quem falou e o que disse, para a conversa de grupo fazer sentido.
pub fn prompt_for(msg: &InboxMessage) -> String {
    let autor = msg.autor_nome.trim();
    let autor = if autor.is_empty() { "Usuario do Flexi" } else { autor };
    format!("{autor} (pelo Flexi): {}", msg.conteudo.trim())
}

/// O que o Flexi guarda de cada DOT compartilhado.
pub fn agent_payload(dot: &Dot) -> Value {
    json!({
        "id": dot.id,
        "nome": dot.name,
        "descricao": dot.description,
        "status": dot.status.as_str(),
        "usuarios": dot.flexi_share.users.iter().map(|u| u.id).collect::<Vec<_>>(),
    })
}

/// Estado da resposta do DOT a partir dos metadados da mensagem dele: `(estado, terminou)`.
pub fn reply_state(metadata: &Value) -> (&'static str, bool) {
    if metadata.get("streaming").and_then(Value::as_bool).unwrap_or(false) {
        return ("processando", false);
    }
    match metadata.get("status").and_then(Value::as_str) {
        Some("completed") => ("concluida", true),
        Some(_) => ("erro", true),
        // Mensagem sem metadados de execucao (motor ainda nao comecou): continua em andamento.
        None => ("processando", false),
    }
}

/// Nomes das ferramentas usadas, guardados pelo executor em `metadata.tools`.
pub fn reply_actions(metadata: &Value) -> Vec<String> {
    metadata.get("tools").and_then(Value::as_array).map(|t| t.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Id da mensagem do Flexi guardado nos metadados da mensagem do usuario na conversa do DOT.
pub fn flexi_message_id(metadata: &Value) -> Option<i64> {
    metadata.get("flexi").and_then(|f| f.get("mensagem_id")).and_then(Value::as_i64)
}

fn task_is_terminal(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
}

fn path(suffix: &str) -> String {
    format!("{API_PREFIX}{suffix}")
}

impl RuntimeService {
    fn set_link(&self, connected: bool, base_url: Option<String>, error: Option<String>) {
        let now = dots_core::now();
        let changed = {
            let mut cur = match self.flexi_link.inner.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let changed = cur.connected != connected || cur.last_error != error;
            if changed {
                cur.connected = connected;
                cur.since = if connected { Some(now) } else { None };
                cur.last_error = error;
            }
            cur.base_url = base_url.or(cur.base_url.take());
            if connected {
                cur.last_sync = Some(now);
            }
            changed
        };
        if changed {
            self.events.emit("flexi_link.changed", json!(self.flexi_link.status()));
        }
    }

    /// Contadores que o Flexi mostra no icone: aprovacoes pendentes (permissoes + perguntas) e DOTs ativos.
    async fn flexi_sync_counts(&self) -> DotsResult<(u32, u32)> {
        self.storage
            .call(|s| {
                let pending = s.approvals().pending_count()? + s.approvals().pending_questions_count()?;
                let dots = s.dots().list(false)?.len() as u32;
                Ok((pending, dots))
            })
            .await
    }

    /// Conta do usuario (ligada pelo "Logar com Flexi") com sessao guardada.
    async fn flexi_sync_account(&self) -> DotsResult<Option<(AppConnection, FlexiClient)>> {
        let conn = self
            .storage
            .call(|s| {
                let Some(setting) = s.settings().get(ACCOUNT_CONNECTION_SETTING)? else { return Ok(None) };
                let Some(id) = setting.value.as_str().map(String::from) else { return Ok(None) };
                Ok(s.apps().get(&id).ok())
            })
            .await?;
        let Some(conn) = conn else { return Ok(None) };
        let client = self.flexi_client(&conn).await?;
        Ok(if client.session_cookie().is_some() { Some((conn, client)) } else { None })
    }

    /// DOTs compartilhados no Flexi (ligados e nao arquivados).
    async fn flexi_shared_dots(&self) -> DotsResult<Vec<Dot>> {
        Ok(self.storage.call(|s| s.dots().list(false)).await?.into_iter().filter(|d| d.flexi_share.enabled && d.status != DotStatus::Archived).collect())
    }

    /// Conversa unica do DOT com o Flexi; cria na primeira mensagem.
    async fn flexi_conversation_for(&self, dot_id: &str) -> DotsResult<String> {
        let d = dot_id.to_string();
        self.storage
            .call(move |s| {
                if let Some(id) = s.dots().flexi_conversation(&d)? {
                    if s.conversations().get(&id).is_ok() {
                        return Ok(id);
                    }
                }
                let conv = s.conversations().create(&d, "Flexi")?;
                s.dots().set_flexi_conversation(&d, Some(&conv.id))?;
                Ok(conv.id)
            })
            .await
    }

    /// Usuarios da base da conta do usuario, para escolher com quem compartilhar um DOT (IPC `apps.flexi.users`).
    pub async fn flexi_users(&self) -> DotsResult<Vec<Value>> {
        let Some((_, client)) = self.flexi_sync_account().await? else {
            return Err(DotsError::Validation("entre com sua conta do Flexi (Logar com Flexi) para escolher usuarios".into()));
        };
        let v = client
            .call("GET", "/api/usuarios", Some(&json!({ "page": 1, "pageSize": 500, "sortBy": "nome", "sortDir": "asc" })), None)
            .await
            .map_err(|e| DotsError::Validation(e.to_string()))?;
        Ok(v.get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|u| {
                        let id = u.get("id").and_then(Value::as_i64)?;
                        Some(json!({ "id": id, "nome": u.get("nome").and_then(Value::as_str).unwrap_or(""), "email": u.get("email").and_then(Value::as_str).unwrap_or("") }))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Aplica uma decisao vinda do Flexi web com a mesma politica e auditoria das decisoes no app.
    async fn apply_flexi_decision(&self, d: &Decision) -> DotsResult<dots_core::Approval> {
        let reason = d.motivo.clone().filter(|m| !m.trim().is_empty());
        let approval = self.decide_approval_by(&d.id, d.aprovado, reason, "user", "o usuario (pelo Flexi)").await?;
        let (aid, kind, approved) = (d.id.clone(), approval.kind.as_str().to_string(), d.aprovado);
        let _ = self.storage.call(move |s| s.audit().record("user", "approval.decide_via_flexi_web", "approval", Some(&aid), json!({ "approved": approved, "kind": kind })).map(|_| ())).await;
        Ok(approval)
    }
}

async fn register_agents(svc: &RuntimeService, client: &FlexiClient) -> Result<(), SyncError> {
    let dots = svc.flexi_shared_dots().await?;
    let body = json!({ "versao": crate::VERSION, "agentes": dots.iter().map(agent_payload).collect::<Vec<_>>() });
    client.call("PUT", &path("/agentes"), None, Some(&body)).await?;
    tracing::debug!(agentes = dots.len(), "DOTs compartilhados registrados no Flexi");
    Ok(())
}

async fn start_message(svc: &RuntimeService, client: &FlexiClient, st: &mut SyncState, msg: &InboxMessage) -> Result<(), SyncError> {
    let dot = svc.storage.call({ let id = msg.agente_id.clone(); move |s| s.dots().get(&id) }).await.ok();
    let Some(dot) = dot.filter(|d| d.flexi_share.enabled && d.status != DotStatus::Archived) else {
        client.call("POST", &path(&format!("/mensagens/{}/estado", msg.id)), None, Some(&json!({ "estado": "erro", "detalhe": "este agente nao esta mais compartilhado" }))).await?;
        return Ok(());
    };
    let cid = svc.flexi_conversation_for(&dot.id).await?;
    let meta = json!({ "flexi": { "mensagem_id": msg.id, "usuario_id": msg.usuario_id, "autor_nome": msg.autor_nome } });
    let (_, task) = svc.send_conversation_message(&cid, prompt_for(msg), vec![], meta).await?;
    client.call("POST", &path(&format!("/mensagens/{}/estado", msg.id)), None, Some(&json!({ "estado": "processando" }))).await?;
    st.inflight.insert(msg.id, InFlight { task_id: task.id, conversation_id: cid, agente_id: dot.id, last_pushed: None });
    Ok(())
}

/// Depois de um reinicio, reencontra a tarefa de uma mensagem que ja estava "processando".
async fn recover_message(svc: &RuntimeService, st: &mut SyncState, msg: &InboxMessage) -> DotsResult<bool> {
    let dot_id = msg.agente_id.clone();
    let Some(cid) = svc.storage.call(move |s| s.dots().flexi_conversation(&dot_id)).await.ok().flatten() else { return Ok(false) };
    let msgs = svc.storage.call({ let c = cid.clone(); move |s| s.conversations().messages(&c, CONVERSATION_SCAN) }).await?;
    let Some(task_id) = msgs.iter().rev().find(|m| m.role == MessageRole::User && flexi_message_id(&m.metadata) == Some(msg.id)).and_then(|m| m.metadata.get("task_id").and_then(Value::as_str).map(String::from)) else {
        return Ok(false);
    };
    st.inflight.insert(msg.id, InFlight { task_id, conversation_id: cid, agente_id: msg.agente_id.clone(), last_pushed: None });
    Ok(true)
}

async fn push_progress(svc: &RuntimeService, client: &FlexiClient, st: &mut SyncState) -> Result<(), SyncError> {
    let ids: Vec<i64> = st.inflight.keys().copied().collect();
    for id in ids {
        let Some(mut f) = st.inflight.get(&id).cloned() else { continue };
        let (task, msgs) = svc
            .storage
            .call({ let (t, c) = (f.task_id.clone(), f.conversation_id.clone()); move |s| Ok((s.tasks().get(&t).ok(), s.conversations().messages(&c, CONVERSATION_SCAN)?)) })
            .await?;
        let reply = msgs.iter().rev().find(|m| m.role == MessageRole::Dot && m.task_id.as_deref() == Some(&f.task_id));
        let situacao = task.as_ref().map(|t| t.status.as_str().to_string()).unwrap_or_default();
        let (dots_message_id, content, estado, done, acoes) = match (reply, task.as_ref()) {
            (Some(m), _) => {
                let (estado, done) = reply_state(&m.metadata);
                let content = if m.content.is_empty() && done { m.metadata.get("error").and_then(Value::as_str).unwrap_or("A tarefa terminou sem resposta.").to_string() } else { m.content.clone() };
                (m.id.clone(), content, estado, done, reply_actions(&m.metadata))
            }
            (None, Some(t)) if task_is_terminal(t.status) => {
                let content = t.result_text.clone().filter(|r| !r.is_empty()).or_else(|| t.error.clone()).unwrap_or_else(|| "A tarefa terminou sem resposta.".into());
                (format!("task:{}", t.id), content, if t.status == TaskStatus::Completed { "concluida" } else { "erro" }, true, vec![])
            }
            (None, Some(_)) => continue,
            (None, None) => (format!("task:{}", f.task_id), "A tarefa nao existe mais.".to_string(), "erro", true, vec![]),
        };
        let key = (content.clone(), estado.to_string(), situacao.clone());
        if f.last_pushed.as_ref() != Some(&key) {
            let body = json!({
                "agente_id": f.agente_id,
                "dots_message_id": dots_message_id,
                "conteudo": content,
                "estado": estado,
                "situacao": situacao,
                "acoes": acoes,
                "em_resposta_a": id,
            });
            client.call("POST", &path("/mensagens"), None, Some(&body)).await?;
            f.last_pushed = Some(key);
            st.inflight.insert(id, f.clone());
        }
        if done {
            client.call("POST", &path(&format!("/mensagens/{id}/estado")), None, Some(&json!({ "estado": if estado == "erro" { "erro" } else { "concluida" } }))).await?;
            st.inflight.remove(&id);
            st.perf_dirty = true;
        }
    }
    Ok(())
}

async fn cycle(svc: &RuntimeService, client: &FlexiClient, st: &mut SyncState) -> Result<(), SyncError> {
    if !st.started || st.registry_dirty {
        register_agents(svc, client).await?;
        st.registry_dirty = false;
    }

    let (pending, dots) = svc.flexi_sync_counts().await?;
    let mut body = json!({ "versao": crate::VERSION, "pendentes": pending, "dots": dots });
    if !st.started || st.approvals_dirty {
        let approvals = svc.storage.call(|s| s.approvals().list(false, APPROVALS_LIMIT)).await?;
        body["aprovacoes"] = json!(approvals);
    }
    let perf_due = match st.last_perf_push {
        None => true,
        Some(t) => t.elapsed() >= PERFORMANCE_INTERVAL || (st.perf_dirty && t.elapsed() >= PERFORMANCE_DEBOUNCE),
    };
    if perf_due {
        body["desempenho"] = json!({ "7": svc.performance_summary(7).await?, "30": svc.performance_summary(30).await? });
    }
    let resp = client.call("POST", &path("/sincronizar"), None, Some(&body)).await?;
    // So depois do aceite: um erro de rede deixa os dados para a proxima rodada.
    st.started = true;
    st.approvals_dirty = false;
    if perf_due {
        st.perf_dirty = false;
        st.last_perf_push = Some(tokio::time::Instant::now());
    }
    let resp: SyncResponse = serde_json::from_value(resp).map_err(|e| SyncError::Failed(format!("resposta inesperada do Flexi: {e}")))?;

    for d in &resp.decisoes {
        let body = match svc.apply_flexi_decision(d).await {
            Ok(a) => json!({ "aprovacao": a }),
            Err(e) => {
                tracing::warn!(approval = %d.id, error = %e, "decisao do Flexi web nao pode ser aplicada");
                let current = svc.storage.call({ let id = d.id.clone(); move |s| s.approvals().get(&id) }).await.ok();
                json!({ "erro": e.to_string(), "aprovacao": current })
            }
        };
        client.call("POST", &path(&format!("/aprovacoes/{}/aplicada", d.id)), None, Some(&body)).await?;
        st.approvals_dirty = true;
    }

    for m in &resp.mensagens {
        if st.inflight.contains_key(&m.id) {
            continue;
        }
        if m.estado == "processando" && recover_message(svc, st, m).await? {
            continue;
        }
        if let Err(e) = start_message(svc, client, st, m).await {
            match e {
                SyncError::Unauthorized(m) => return Err(SyncError::Unauthorized(m)),
                SyncError::Failed(err) => {
                    tracing::warn!(mensagem = m.id, error = %err, "mensagem do Flexi nao pode ser entregue ao DOT");
                    client.call("POST", &path(&format!("/mensagens/{}/estado", m.id)), None, Some(&json!({ "estado": "erro", "detalhe": err }))).await?;
                }
            }
        }
    }

    push_progress(svc, client, st).await
}

/// Espera o proximo ciclo: `POLL_INTERVAL` ou um evento que mude o que o Flexi precisa ver, o que vier primeiro.
async fn wait_tick(svc: &RuntimeService, rx: &mut broadcast::Receiver<dots_ipc::Event>, st: &mut SyncState, timeout: Duration) {
    let mut shutdown = Box::pin(svc.shutdown.clone().wait());
    let sleep = tokio::time::sleep(timeout);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut shutdown => return,
            _ = &mut sleep => return,
            ev = rx.recv() => match ev {
                Ok(ev) if ev.event == RECONNECT_EVENT || ev.event == "apps.changed" => return,
                Ok(ev) if REGISTRY_EVENTS.contains(&ev.event.as_str()) => { st.registry_dirty = true; return }
                Ok(ev) if APPROVAL_EVENTS.contains(&ev.event.as_str()) => { st.approvals_dirty = true; return }
                Ok(ev) if ev.event == "task.changed" => {
                    let id = ev.payload.get("id").and_then(Value::as_str).unwrap_or("");
                    let status = ev.payload.get("status").and_then(Value::as_str).unwrap_or("");
                    let tracked = st.inflight.values().any(|f| f.task_id == id);
                    if matches!(status, "COMPLETED" | "FAILED" | "CANCELLED") { st.perf_dirty = true; }
                    if tracked { return }
                }
                Ok(ev) if ev.event == "message.changed" => {
                    let cid = ev.payload.get("conversation_id").and_then(Value::as_str).unwrap_or("");
                    if st.inflight.values().any(|f| f.conversation_id == cid) { return }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
        }
    }
}

/// Tarefa de fundo: sincroniza com o Flexi enquanto houver conta do usuario com sessao.
pub async fn run(svc: Arc<RuntimeService>) {
    let mut rx = svc.events.subscribe();
    let mut st = SyncState::default();
    let mut backoff = BACKOFF_MIN;
    let mut last_cycle: Option<tokio::time::Instant> = None;
    loop {
        if svc.shutdown.is_triggered() {
            svc.set_link(false, None, None);
            return;
        }
        let account = match svc.flexi_sync_account().await {
            Ok(Some(a)) => a,
            Ok(None) => {
                svc.set_link(false, None, None);
                st.started = false;
                wait_tick(&svc, &mut rx, &mut st, IDLE_RETRY).await;
                continue;
            }
            Err(e) => {
                svc.set_link(false, None, Some(e.to_string()));
                wait_tick(&svc, &mut rx, &mut st, IDLE_RETRY).await;
                continue;
            }
        };
        let (conn, client) = account;
        if let Some(t) = last_cycle {
            if t.elapsed() < MIN_CYCLE_GAP {
                tokio::time::sleep(MIN_CYCLE_GAP - t.elapsed()).await;
            }
        }
        last_cycle = Some(tokio::time::Instant::now());
        match cycle(&svc, &client, &mut st).await {
            Ok(()) => {
                svc.set_link(true, Some(conn.base_url.clone()), None);
                backoff = BACKOFF_MIN;
                wait_tick(&svc, &mut rx, &mut st, POLL_INTERVAL).await;
            }
            Err(SyncError::Unauthorized(reason)) => {
                tracing::warn!(reason = %reason, "sessao do Flexi recusada na sincronizacao; tentando renovar");
                svc.set_link(false, Some(conn.base_url.clone()), Some(reason));
                st.started = false;
                if let Err(e) = svc.flexi_check(&conn.id).await {
                    tracing::warn!(error = %e, "renovacao da sessao do Flexi falhou");
                }
                wait_tick(&svc, &mut rx, &mut st, Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
            Err(SyncError::Failed(reason)) => {
                tracing::warn!(reason = %reason, backoff, "sincronizacao com o Flexi falhou");
                svc.set_link(false, Some(conn.base_url.clone()), Some(reason));
                wait_tick(&svc, &mut rx, &mut st, Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(autor: &str, texto: &str) -> InboxMessage {
        InboxMessage { id: 7, agente_id: "dot-1".into(), usuario_id: Some(3), autor_nome: autor.into(), conteudo: texto.into(), estado: "enviada".into() }
    }

    #[test]
    fn prompt_identifica_quem_falou() {
        assert_eq!(prompt_for(&msg("Maria Silva", "  qual o estoque do MLB1?  ")), "Maria Silva (pelo Flexi): qual o estoque do MLB1?");
        assert_eq!(prompt_for(&msg("  ", "oi")), "Usuario do Flexi (pelo Flexi): oi");
    }

    #[test]
    fn estado_da_resposta_segue_os_metadados_do_executor() {
        assert_eq!(reply_state(&json!({ "streaming": true })), ("processando", false));
        assert_eq!(reply_state(&json!({ "streaming": false, "status": "completed", "tools": ["flexi_anuncios"] })), ("concluida", true));
        assert_eq!(reply_state(&json!({ "streaming": false, "status": "failed", "error": "x" })), ("erro", true));
        assert_eq!(reply_state(&json!({})), ("processando", false));
        assert_eq!(reply_actions(&json!({ "tools": ["a", "b", 3] })), vec!["a".to_string(), "b".to_string()]);
        assert!(reply_actions(&json!({})).is_empty());
    }

    #[test]
    fn le_id_da_mensagem_do_flexi_nos_metadados() {
        assert_eq!(flexi_message_id(&json!({ "flexi": { "mensagem_id": 42 }, "task_id": "t" })), Some(42));
        assert_eq!(flexi_message_id(&json!({ "attachments": [] })), None);
    }

    #[test]
    fn interpreta_resposta_da_sincronizacao() {
        let r: SyncResponse = serde_json::from_value(json!({
            "mensagens": [{ "id": 1, "agente_id": "d", "usuario_id": 9, "autor_nome": "Ana", "conteudo": "oi", "estado": "enviada" }],
            "decisoes": [{ "id": "ap-1", "aprovado": false, "motivo": "nao agora" }]
        }))
        .unwrap();
        assert_eq!(r.mensagens.len(), 1);
        assert_eq!(r.mensagens[0].autor_nome, "Ana");
        assert_eq!(r.decisoes[0], Decision { id: "ap-1".into(), aprovado: false, motivo: Some("nao agora".into()) });
        let vazio: SyncResponse = serde_json::from_value(json!({ "status": 1 })).unwrap();
        assert!(vazio.mensagens.is_empty() && vazio.decisoes.is_empty());
    }

    #[test]
    fn monta_os_caminhos_do_app() {
        assert_eq!(path("/sincronizar"), "/api/agentes-dots/app/sincronizar");
        assert_eq!(path("/mensagens/12/estado"), "/api/agentes-dots/app/mensagens/12/estado");
    }
}
