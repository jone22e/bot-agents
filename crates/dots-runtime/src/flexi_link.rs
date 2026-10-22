//! Canal persistente entre o `dotsd` e o Flexi web (ADR 0015).
//!
//! O runtime abre um WebSocket em `{base_url}/api/ws/agentes-dots` com o cookie de sessao da conta do usuario
//! (`account.flexi_connection_id`). O Flexi usa esse socket para saber que o app esta ligado e para pedir,
//! ao vivo, a Central de Aprovacoes e a Avaliacao de desempenho; as decisoes tomadas no Flexi web voltam
//! pelo mesmo canal. Nada e copiado para o banco do Flexi.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dots_core::{AppConnection, DotsError, DotsResult, Timestamp};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::flexi::ACCOUNT_CONNECTION_SETTING;
use crate::service::RuntimeService;

pub const WS_PATH: &str = "/api/ws/agentes-dots";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_RETRY: Duration = Duration::from_secs(30);
const BACKOFF_MIN: u64 = 5;
const BACKOFF_MAX: u64 = 60;
/// Eventos do runtime que mudam o que o Flexi mostra (pendentes e quantidade de DOTs).
const STATE_EVENTS: [&str; 4] = ["approval.requested", "approval.resolved", "dots.changed", "teams.changed"];

/// Estado do canal, em memoria, para a interface do app (`apps.flexi.link_status`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LinkStatus {
    pub connected: bool,
    pub since: Option<Timestamp>,
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

impl RuntimeService {
    fn set_link(&self, connected: bool, base_url: Option<String>, error: Option<String>) {
        let next = LinkStatus { connected, since: if connected { Some(dots_core::now()) } else { None }, last_error: error, base_url };
        let changed = {
            let mut cur = match self.flexi_link.inner.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let changed = cur.connected != next.connected || cur.last_error != next.last_error;
            if changed {
                *cur = next.clone();
            } else if connected {
                cur.base_url = next.base_url.clone();
            }
            changed
        };
        if changed {
            self.events.emit("flexi_link.changed", json!(next));
        }
    }

    /// Contadores que o Flexi mostra no icone: aprovacoes pendentes (permissoes + perguntas) e DOTs ativos.
    async fn flexi_link_counts(&self) -> DotsResult<(u32, u32)> {
        self.storage
            .call(|s| {
                let pending = s.approvals().pending_count()? + s.approvals().pending_questions_count()?;
                let dots = s.dots().list(false)?.len() as u32;
                Ok((pending, dots))
            })
            .await
    }

    /// Conta do usuario (ligada pelo "Logar com Flexi") e o cookie de sessao dela.
    async fn flexi_link_account(&self) -> DotsResult<Option<(AppConnection, String)>> {
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
        Ok(client.session_cookie().map(|cookie| (conn, cookie)))
    }

    /// Atende um pedido do Flexi web. Metodos fora do contrato devolvem erro de validacao.
    pub async fn flexi_link_handle(&self, method: &str, params: Value) -> DotsResult<Value> {
        match method {
            "approvals.list" => {
                let pending_only = params.get("pending_only").and_then(Value::as_bool).unwrap_or(false);
                let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(200).clamp(1, 1000) as u32;
                Ok(json!(self.storage.call(move |s| s.approvals().list(pending_only, limit)).await?))
            }
            "approvals.decide" => {
                let id = params.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or_else(|| DotsError::Validation("id obrigatorio".into()))?.to_string();
                let approved = params.get("approved").and_then(Value::as_bool).ok_or_else(|| DotsError::Validation("approved obrigatorio".into()))?;
                let reason = params.get("reason").and_then(Value::as_str).map(String::from).filter(|s| !s.trim().is_empty());
                let approval = self.decide_approval_by(&id, approved, reason, "user", "o usuario (pelo Flexi)").await?;
                let (aid, kind) = (id.clone(), approval.kind.as_str().to_string());
                let _ = self.storage.call(move |s| s.audit().record("user", "approval.decide_via_flexi_web", "approval", Some(&aid), json!({ "approved": approved, "kind": kind })).map(|_| ())).await;
                Ok(json!(approval))
            }
            "performance.summary" => {
                let days = params.get("days").and_then(Value::as_u64).unwrap_or(7);
                let days = if days == 30 { 30 } else { 7 };
                let end = dots_core::now();
                let start = end - chrono::Duration::days(days as i64);
                let (s0, e0) = (start, end);
                let (tasks, executions, approvals, audit, dots) = self
                    .storage
                    .call(move |s| {
                        let tasks = s.tasks().list(&dots_storage::repos::TaskFilter { dot_id: None, status: None, conversation_id: None, limit: 2000 })?;
                        let executions = s.executions().list_overlapping(&s0, &e0)?;
                        let approvals = s.approvals().list(false, 200)?;
                        let audit = s.audit().list_recent(2000)?;
                        let dots = s.dots().list(true)?;
                        Ok((tasks, executions, approvals, audit, dots))
                    })
                    .await?;
                let dot_names: HashMap<String, String> = dots.into_iter().map(|d| (d.id, d.name)).collect();
                let rows = crate::performance::compute_performance(&crate::performance::PerformanceInput { tasks: &tasks, executions: &executions, approvals: &approvals, audit: &audit, dot_names: &dot_names, start, end });
                let total = crate::performance::total_performance(&rows);
                Ok(json!({ "start": start, "end": end, "days": days, "total": total, "dots": rows }))
            }
            "status" => {
                let (pending, dots) = self.flexi_link_counts().await?;
                Ok(json!({ "pending_approvals": pending, "dots": dots, "version": crate::VERSION, "started_at": self.started_at }))
            }
            other => Err(DotsError::Validation(format!("metodo desconhecido: {other}"))),
        }
    }
}

/// Mensagens que o Flexi envia pelo socket.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Incoming {
    Ready,
    Request {
        id: String,
        method: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(other)]
    Other,
}

pub fn hello_message(version: &str, pending: u32, dots: u32) -> String {
    json!({ "type": "hello", "app": "dots", "version": version, "pending_approvals": pending, "dots": dots }).to_string()
}

pub fn state_message(pending: u32, dots: u32) -> String {
    json!({ "type": "state", "pending_approvals": pending, "dots": dots }).to_string()
}

pub fn response_message(id: &str, result: DotsResult<Value>) -> String {
    match result {
        Ok(result) => json!({ "type": "response", "id": id, "ok": true, "result": result }).to_string(),
        Err(e) => json!({ "type": "response", "id": id, "ok": false, "error": e.to_string() }).to_string(),
    }
}

/// `https://host/base` -> `wss://host/base/api/ws/agentes-dots` (e `http` -> `ws`).
pub fn ws_url(base_url: &str) -> DotsResult<String> {
    let mut url = reqwest::Url::parse(base_url.trim_end_matches('/')).map_err(|e| DotsError::Validation(format!("URL do Flexi invalida: {e}")))?;
    let scheme = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        other => return Err(DotsError::Validation(format!("esquema nao suportado: {other}"))),
    };
    url.set_scheme(scheme).map_err(|_| DotsError::Validation("nao foi possivel montar a URL do WebSocket".into()))?;
    let path = format!("{}{}", url.path().trim_end_matches('/'), WS_PATH);
    url.set_path(&path);
    url.set_query(None);
    Ok(url.to_string())
}

enum SessionEnd {
    /// Fechado pelo Flexi ou pelo runtime; reconecta em seguida.
    Closed(String),
    /// 401/403 no handshake: a sessao do Flexi expirou.
    Unauthorized(String),
    Failed(String),
}

async fn session(svc: &Arc<RuntimeService>, conn: &AppConnection, cookie: &str, rx: &mut broadcast::Receiver<dots_ipc::Event>) -> SessionEnd {
    let url = match ws_url(&conn.base_url) {
        Ok(u) => u,
        Err(e) => return SessionEnd::Failed(e.to_string()),
    };
    let mut request = match url.as_str().into_client_request() {
        Ok(r) => r,
        Err(e) => return SessionEnd::Failed(e.to_string()),
    };
    let cookie_header = match HeaderValue::from_str(&format!("userdata={cookie}")) {
        Ok(h) => h,
        Err(_) => return SessionEnd::Failed("cookie de sessao invalido".into()),
    };
    request.headers_mut().insert("Cookie", cookie_header);
    request.headers_mut().insert("User-Agent", HeaderValue::from_static(concat!("DOTS/", env!("CARGO_PKG_VERSION"), " (conector Flexi)")));

    let ws = match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(request)).await {
        Ok(Ok((ws, _))) => ws,
        Ok(Err(WsError::Http(resp))) => {
            let status = resp.status();
            let msg = format!("Flexi respondeu HTTP {status} ao abrir o canal");
            return if status.as_u16() == 401 || status.as_u16() == 403 { SessionEnd::Unauthorized(msg) } else { SessionEnd::Failed(msg) };
        }
        Ok(Err(e)) => return SessionEnd::Failed(e.to_string()),
        Err(_) => return SessionEnd::Failed("tempo esgotado ao abrir o canal com o Flexi".into()),
    };
    let (mut sink, mut stream) = ws.split();

    let (pending, dots) = svc.flexi_link_counts().await.unwrap_or((0, 0));
    if let Err(e) = sink.send(Message::text(hello_message(crate::VERSION, pending, dots))).await {
        return SessionEnd::Failed(e.to_string());
    }
    svc.set_link(true, Some(conn.base_url.clone()), None);
    tracing::info!(url = %url, "canal com o Flexi web aberto");

    let mut shutdown = Box::pin(svc.shutdown.clone().wait());
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                let _ = sink.send(Message::Close(None)).await;
                return SessionEnd::Closed("runtime encerrando".into());
            }
            ev = rx.recv() => match ev {
                Ok(ev) if STATE_EVENTS.contains(&ev.event.as_str()) => {
                    let (pending, dots) = svc.flexi_link_counts().await.unwrap_or((0, 0));
                    if let Err(e) = sink.send(Message::text(state_message(pending, dots))).await {
                        return SessionEnd::Failed(e.to_string());
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return SessionEnd::Closed("barramento de eventos fechado".into()),
            },
            msg = stream.next() => match msg {
                None => return SessionEnd::Closed("conexao encerrada".into()),
                Some(Err(e)) => return SessionEnd::Failed(e.to_string()),
                Some(Ok(Message::Ping(p))) => {
                    if let Err(e) = sink.send(Message::Pong(p)).await {
                        return SessionEnd::Failed(e.to_string());
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    let reason = frame.map(|f| format!("fechado pelo Flexi ({}: {})", u16::from(f.code), f.reason)).unwrap_or_else(|| "fechado pelo Flexi".into());
                    return SessionEnd::Closed(reason);
                }
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<Incoming>(text.as_str()) {
                        Ok(Incoming::Request { id, method, params }) => {
                            let result = svc.flexi_link_handle(&method, params).await;
                            if let Err(e) = &result {
                                tracing::warn!(method = %method, error = %e, "pedido do Flexi web falhou");
                            }
                            if let Err(e) = sink.send(Message::text(response_message(&id, result))).await {
                                return SessionEnd::Failed(e.to_string());
                            }
                        }
                        Ok(Incoming::Ready) => tracing::debug!("Flexi web confirmou o canal"),
                        Ok(Incoming::Other) => {}
                        Err(e) => tracing::debug!(error = %e, "mensagem do Flexi web ignorada"),
                    }
                }
                Some(Ok(_)) => {}
            },
        }
    }
}

/// Espera `IDLE_RETRY` ou um evento `apps.changed` (nova conta, sessao renovada), o que vier primeiro.
async fn wait_for_account(svc: &RuntimeService, rx: &mut broadcast::Receiver<dots_ipc::Event>) {
    let mut shutdown = Box::pin(svc.shutdown.clone().wait());
    let sleep = tokio::time::sleep(IDLE_RETRY);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut shutdown => return,
            _ = &mut sleep => return,
            ev = rx.recv() => match ev {
                Ok(ev) if ev.event == "apps.changed" => return,
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
        }
    }
}

/// Tarefa de fundo: mantem o canal aberto enquanto houver conta do usuario com sessao.
pub async fn run(svc: Arc<RuntimeService>) {
    let mut rx = svc.events.subscribe();
    let mut backoff = BACKOFF_MIN;
    loop {
        if svc.shutdown.is_triggered() {
            svc.set_link(false, None, None);
            return;
        }
        let account = match svc.flexi_link_account().await {
            Ok(Some(a)) => a,
            Ok(None) => {
                svc.set_link(false, None, None);
                wait_for_account(&svc, &mut rx).await;
                continue;
            }
            Err(e) => {
                svc.set_link(false, None, Some(e.to_string()));
                wait_for_account(&svc, &mut rx).await;
                continue;
            }
        };
        let (conn, cookie) = account;
        // Descarta eventos antigos: so interessam mudancas durante a sessao.
        rx = rx.resubscribe();
        let end = session(&svc, &conn, &cookie, &mut rx).await;
        if svc.shutdown.is_triggered() {
            svc.set_link(false, None, None);
            return;
        }
        match end {
            SessionEnd::Closed(reason) => {
                tracing::info!(reason = %reason, "canal com o Flexi web fechado; reconectando");
                svc.set_link(false, Some(conn.base_url.clone()), None);
                backoff = BACKOFF_MIN;
                tokio::time::sleep(Duration::from_secs(BACKOFF_MIN)).await;
            }
            SessionEnd::Unauthorized(reason) => {
                tracing::warn!(reason = %reason, "sessao do Flexi recusada no canal; tentando renovar");
                svc.set_link(false, Some(conn.base_url.clone()), Some(reason));
                if let Err(e) = svc.flexi_check(&conn.id).await {
                    tracing::warn!(error = %e, "renovacao da sessao do Flexi falhou");
                }
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
            SessionEnd::Failed(reason) => {
                tracing::warn!(reason = %reason, backoff, "canal com o Flexi web falhou");
                svc.set_link(false, Some(conn.base_url.clone()), Some(reason));
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monta_url_do_websocket() {
        assert_eq!(ws_url("https://flexi.c2s-business.com").unwrap(), "wss://flexi.c2s-business.com/api/ws/agentes-dots");
        assert_eq!(ws_url("http://127.0.0.1:2012/").unwrap(), "ws://127.0.0.1:2012/api/ws/agentes-dots");
        assert_eq!(ws_url("https://host/prefixo/").unwrap(), "wss://host/prefixo/api/ws/agentes-dots");
        assert!(ws_url("ftp://x").is_err());
        assert!(ws_url("nao e url").is_err());
    }

    #[test]
    fn interpreta_mensagens_do_flexi() {
        assert_eq!(serde_json::from_str::<Incoming>(r#"{"type":"ready"}"#).unwrap(), Incoming::Ready);
        let req = serde_json::from_str::<Incoming>(r#"{"type":"request","id":"r1","method":"approvals.list","params":{"pending_only":true,"limit":50}}"#).unwrap();
        assert_eq!(req, Incoming::Request { id: "r1".into(), method: "approvals.list".into(), params: json!({ "pending_only": true, "limit": 50 }) });
        let sem_params = serde_json::from_str::<Incoming>(r#"{"type":"request","id":"r2","method":"status"}"#).unwrap();
        assert_eq!(sem_params, Incoming::Request { id: "r2".into(), method: "status".into(), params: Value::Null });
        assert_eq!(serde_json::from_str::<Incoming>(r#"{"type":"algo_novo","x":1}"#).unwrap(), Incoming::Other);
        assert!(serde_json::from_str::<Incoming>(r#"{"type":"request","method":"x"}"#).is_err());
    }

    #[test]
    fn serializa_mensagens_do_app() {
        let hello: Value = serde_json::from_str(&hello_message("1.2.3", 4, 2)).unwrap();
        assert_eq!(hello, json!({ "type": "hello", "app": "dots", "version": "1.2.3", "pending_approvals": 4, "dots": 2 }));
        let state: Value = serde_json::from_str(&state_message(0, 5)).unwrap();
        assert_eq!(state, json!({ "type": "state", "pending_approvals": 0, "dots": 5 }));
        let ok: Value = serde_json::from_str(&response_message("r1", Ok(json!([1, 2])))).unwrap();
        assert_eq!(ok, json!({ "type": "response", "id": "r1", "ok": true, "result": [1, 2] }));
        let err: Value = serde_json::from_str(&response_message("r2", Err(DotsError::Validation("metodo desconhecido: x".into())))).unwrap();
        assert_eq!(err["ok"], json!(false));
        assert_eq!(err["id"], json!("r2"));
        assert!(err["error"].as_str().unwrap().contains("metodo desconhecido: x"));
    }
}
