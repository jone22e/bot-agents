use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dots_core::{DotsError, DotsResult, PROTOCOL_VERSION};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use tokio::net::UnixStream;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::codec::{Framed, LinesCodec};

use crate::protocol::{Event, Frame, HelloParams, HelloResult, METHOD_HELLO, MAX_FRAME_BYTES, Request, Response};

pub type EventReceiver = mpsc::Receiver<Event>;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>>;

/// Cliente com conexao persistente e requisicoes multiplexadas por `id`.
#[derive(Clone)]
pub struct IpcClient {
    tx: mpsc::Sender<String>,
    pending: Pending,
    next_id: Arc<AtomicU64>,
    hello: HelloResult,
}

impl IpcClient {
    /// Conecta, autentica com `hello` e inicia a task de leitura.
    /// Eventos sao entregues no receiver retornado.
    pub async fn connect(socket_path: &Path, token: &str, client_name: &str) -> DotsResult<(Self, EventReceiver)> {
        let stream = UnixStream::connect(socket_path)
            .await
            .map_err(|e| DotsError::RuntimeUnavailable(format!("socket {}: {e}", socket_path.display())))?;
        let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(MAX_FRAME_BYTES));

        let hello = Request {
            id: 1,
            method: METHOD_HELLO.into(),
            params: serde_json::to_value(HelloParams {
                token: token.to_string(),
                client: client_name.to_string(),
                proto: PROTOCOL_VERSION,
            })?,
        };
        framed
            .send(serde_json::to_string(&hello)?)
            .await
            .map_err(|e| DotsError::RuntimeUnavailable(format!("envio do hello: {e}")))?;
        let line = match framed.next().await {
            Some(Ok(l)) => l,
            Some(Err(e)) => return Err(DotsError::RuntimeUnavailable(format!("resposta do hello: {e}"))),
            None => return Err(DotsError::RuntimeUnavailable("runtime fechou a conexao no hello".into())),
        };
        let resp: Response = serde_json::from_str(&line)?;
        if let Some(err) = resp.error {
            return Err(err.into());
        }
        let hello: HelloResult = serde_json::from_value(resp.result.unwrap_or_default())?;

        let (tx, mut rx) = mpsc::channel::<String>(64);
        let (ev_tx, ev_rx) = mpsc::channel::<Event>(256);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let pending_reader = pending.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    out = rx.recv() => {
                        match out {
                            Some(line) => { if framed.send(line).await.is_err() { break; } }
                            None => break,
                        }
                    }
                    incoming = framed.next() => {
                        match incoming {
                            Some(Ok(line)) => {
                                match serde_json::from_str::<Frame>(&line) {
                                    Ok(Frame::Response(resp)) => {
                                        if let Some(waiter) = pending_reader.lock().await.remove(&resp.id) {
                                            let _ = waiter.send(resp);
                                        }
                                    }
                                    Ok(Frame::Event(ev)) => { let _ = ev_tx.send(ev).await; }
                                    Ok(Frame::Request(_)) => tracing::warn!("cliente recebeu request inesperado"),
                                    Err(e) => tracing::warn!(error = %e, "frame IPC invalido"),
                                }
                            }
                            _ => break,
                        }
                    }
                }
            }
            // Conexao caiu: falha todas as requisicoes pendentes.
            let mut map = pending_reader.lock().await;
            for (_, waiter) in map.drain() {
                let _ = waiter.send(Response::err(0, DotsError::RuntimeUnavailable("conexao com o runtime encerrada".into())));
            }
        });

        Ok((Self { tx, pending, next_id: Arc::new(AtomicU64::new(2)), hello }, ev_rx))
    }

    pub fn hello(&self) -> &HelloResult {
        &self.hello
    }

    pub async fn call(&self, method: &str, params: serde_json::Value) -> DotsResult<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let req = Request { id, method: method.to_string(), params };
        if self.tx.send(serde_json::to_string(&req)?).await.is_err() {
            self.pending.lock().await.remove(&id);
            return Err(DotsError::RuntimeUnavailable("conexao com o runtime encerrada".into()));
        }
        let resp = rx
            .await
            .map_err(|_| DotsError::RuntimeUnavailable("conexao com o runtime encerrada".into()))?;
        match (resp.result, resp.error) {
            (_, Some(e)) => Err(e.into()),
            (Some(v), None) => Ok(v),
            (None, None) => Ok(serde_json::Value::Null),
        }
    }

    pub async fn call_typed<T: DeserializeOwned>(&self, method: &str, params: serde_json::Value) -> DotsResult<T> {
        let v = self.call(method, params).await?;
        serde_json::from_value(v).map_err(|e| DotsError::Protocol(format!("resposta de {method}: {e}")))
    }

    pub fn is_connected(&self) -> bool {
        !self.tx.is_closed()
    }
}
