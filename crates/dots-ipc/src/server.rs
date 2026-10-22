use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use dots_core::{DotsError, DotsResult, PROTOCOL_VERSION};
use futures_util::{SinkExt, StreamExt};
use subtle::ConstantTimeEq;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tokio_util::codec::{Framed, LinesCodec};

use crate::paths::SocketPaths;
use crate::protocol::{Event, Frame, HelloParams, HelloResult, METHOD_HELLO, MAX_FRAME_BYTES, Request, Response};

pub type HandlerFuture = Pin<Box<dyn Future<Output = DotsResult<serde_json::Value>> + Send>>;

/// Trata uma chamada autenticada. Implementado pelo runtime.
pub trait Handler: Send + Sync + 'static {
    fn handle(&self, method: String, params: serde_json::Value) -> HandlerFuture;
}

impl<F> Handler for F
where
    F: Fn(String, serde_json::Value) -> HandlerFuture + Send + Sync + 'static,
{
    fn handle(&self, method: String, params: serde_json::Value) -> HandlerFuture {
        (self)(method, params)
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub version: String,
    pub event_buffer: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { version: env!("CARGO_PKG_VERSION").to_string(), event_buffer: 256 }
    }
}

pub struct IpcServer {
    listener: UnixListener,
    paths: SocketPaths,
    token: String,
    config: ServerConfig,
    started: Instant,
    events: broadcast::Sender<Event>,
    seq: Arc<AtomicU64>,
    clients: Arc<AtomicU32>,
}

/// Emissor de eventos para o runtime publicar notificacoes.
#[derive(Clone)]
pub struct EventSink {
    tx: broadcast::Sender<Event>,
    seq: Arc<AtomicU64>,
}

impl EventSink {
    pub fn emit(&self, event: &str, payload: serde_json::Value) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.tx.send(Event { event: event.to_string(), payload, seq });
    }

    /// Assina os eventos do runtime (mesmo fluxo que os clientes IPC recebem).
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

impl IpcServer {
    /// Cria diretorio/token, remove socket antigo e faz bind com permissao 0600.
    pub fn bind(paths: SocketPaths, config: ServerConfig) -> DotsResult<Self> {
        use std::os::unix::fs::PermissionsExt;
        paths.ensure_dir()?;
        let token = paths.write_new_token()?;
        if paths.socket_path.exists() {
            std::fs::remove_file(&paths.socket_path)?;
        }
        let listener = UnixListener::bind(&paths.socket_path)
            .map_err(|e| DotsError::Io(format!("bind em {}: {e}", paths.socket_path.display())))?;
        std::fs::set_permissions(&paths.socket_path, std::fs::Permissions::from_mode(0o600))?;
        let (events, _) = broadcast::channel(config.event_buffer);
        Ok(Self {
            listener,
            paths,
            token,
            config,
            started: Instant::now(),
            events,
            seq: Arc::new(AtomicU64::new(0)),
            clients: Arc::new(AtomicU32::new(0)),
        })
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn paths(&self) -> &SocketPaths {
        &self.paths
    }

    pub fn event_sink(&self) -> EventSink {
        EventSink { tx: self.events.clone(), seq: self.seq.clone() }
    }

    pub fn connected_clients(&self) -> Arc<AtomicU32> {
        self.clients.clone()
    }

    /// Loop de aceitacao. Termina quando `shutdown` resolver.
    pub async fn serve<H: Handler>(self, handler: Arc<H>, shutdown: impl Future<Output = ()>) {
        let server = Arc::new(ServerShared {
            token: self.token.clone(),
            version: self.config.version.clone(),
            started: self.started,
            events: self.events.clone(),
            clients: self.clients.clone(),
        });
        let my_uid = unsafe { libc::getuid() };
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                accepted = self.listener.accept() => {
                    match accepted {
                        Ok((stream, _)) => {
                            match stream.peer_cred() {
                                Ok(cred) if cred.uid() == my_uid => {
                                    let s = server.clone();
                                    let h = handler.clone();
                                    tokio::spawn(async move { s.handle_connection(stream, h).await });
                                }
                                Ok(cred) => tracing::warn!(uid = cred.uid(), "conexao IPC recusada: uid diferente"),
                                Err(e) => tracing::warn!(error = %e, "conexao IPC recusada: sem credenciais do peer"),
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "falha no accept do socket IPC");
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            }
        }
        let _ = std::fs::remove_file(&self.paths.socket_path);
        let _ = std::fs::remove_file(&self.paths.token_path);
    }
}

struct ServerShared {
    token: String,
    version: String,
    started: Instant,
    events: broadcast::Sender<Event>,
    clients: Arc<AtomicU32>,
}

impl ServerShared {
    async fn handle_connection<H: Handler>(self: Arc<Self>, stream: UnixStream, handler: Arc<H>) {
        let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(MAX_FRAME_BYTES));

        // 1) hello obrigatorio como primeiro frame
        let first = match framed.next().await {
            Some(Ok(line)) => line,
            _ => return,
        };
        let req: Request = match serde_json::from_str(&first) {
            Ok(r) => r,
            Err(_) => {
                let _ = send(&mut framed, &Response::err(0, DotsError::Protocol("frame invalido".into()))).await;
                return;
            }
        };
        if req.method != METHOD_HELLO {
            let _ = send(&mut framed, &Response::err(req.id, DotsError::Unauthorized("hello obrigatorio".into()))).await;
            return;
        }
        let params: HelloParams = match serde_json::from_value(req.params) {
            Ok(p) => p,
            Err(_) => {
                let _ = send(&mut framed, &Response::err(req.id, DotsError::Protocol("hello invalido".into()))).await;
                return;
            }
        };
        let ok: bool = params.token.as_bytes().ct_eq(self.token.as_bytes()).into();
        if !ok {
            tracing::warn!(client = %params.client, "hello recusado: token invalido");
            let _ = send(&mut framed, &Response::err(req.id, DotsError::Unauthorized("token invalido".into()))).await;
            return;
        }
        if params.proto != PROTOCOL_VERSION {
            let _ = send(
                &mut framed,
                &Response::err(req.id, DotsError::Protocol(format!("protocolo {} nao suportado (servidor {})", params.proto, PROTOCOL_VERSION))),
            )
            .await;
            return;
        }
        let hello = HelloResult {
            pid: std::process::id(),
            version: self.version.clone(),
            proto: PROTOCOL_VERSION,
            uptime_seconds: self.started.elapsed().as_secs(),
        };
        if send(&mut framed, &Response::ok(req.id, serde_json::to_value(hello).unwrap())).await.is_err() {
            return;
        }
        self.clients.fetch_add(1, Ordering::SeqCst);
        tracing::debug!(client = %params.client, "cliente IPC autenticado");

        // 2) loop: requisicoes -> handler; eventos -> cliente
        let mut events = self.events.subscribe();
        let (resp_tx, mut resp_rx) = tokio::sync::mpsc::channel::<Response>(64);
        loop {
            tokio::select! {
                line = framed.next() => {
                    match line {
                        Some(Ok(line)) => {
                            let req: Request = match serde_json::from_str(&line) {
                                Ok(r) => r,
                                Err(e) => {
                                    let _ = send(&mut framed, &Response::err(0, DotsError::Protocol(format!("frame invalido: {e}")))).await;
                                    continue;
                                }
                            };
                            let h = handler.clone();
                            let tx = resp_tx.clone();
                            tokio::spawn(async move {
                                let id = req.id;
                                let resp = match h.handle(req.method, req.params).await {
                                    Ok(v) => Response::ok(id, v),
                                    Err(e) => Response::err(id, e),
                                };
                                let _ = tx.send(resp).await;
                            });
                        }
                        Some(Err(e)) => { tracing::warn!(error = %e, "erro de leitura IPC; encerrando conexao"); break; }
                        None => break,
                    }
                }
                Some(resp) = resp_rx.recv() => {
                    if send(&mut framed, &resp).await.is_err() { break; }
                }
                ev = events.recv() => {
                    match ev {
                        Ok(ev) => { if send(&mut framed, &Frame::Event(ev)).await.is_err() { break; } }
                        Err(broadcast::error::RecvError::Lagged(n)) => tracing::warn!(lost = n, "cliente IPC perdeu eventos"),
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
        self.clients.fetch_sub(1, Ordering::SeqCst);
        tracing::debug!(client = %params.client, "cliente IPC desconectado");
    }
}

async fn send<T: serde::Serialize>(framed: &mut Framed<UnixStream, LinesCodec>, value: &T) -> Result<(), ()> {
    let line = serde_json::to_string(value).map_err(|_| ())?;
    framed.send(line).await.map_err(|_| ())
}
