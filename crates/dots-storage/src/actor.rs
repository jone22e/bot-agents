use std::sync::mpsc;
use std::thread::JoinHandle;

use dots_core::{DotsError, DotsResult};

use crate::{Storage, StorageOptions};

type Job = Box<dyn FnOnce(&mut Storage) + Send + 'static>;

enum Message {
    Run(Job),
    Stop,
}

/// Handle clonavel para o ator de armazenamento. Todas as operacoes rodam
/// em uma unica thread que possui a `Connection`, serializando escritas.
#[derive(Clone)]
pub struct StorageHandle {
    tx: mpsc::Sender<Message>,
}

pub struct StorageActor {
    pub handle: StorageHandle,
    thread: Option<JoinHandle<()>>,
}

impl StorageHandle {
    /// Abre o banco na thread do ator. Falhas de abertura/migracao sao retornadas aqui.
    pub fn spawn(options: StorageOptions) -> DotsResult<StorageActor> {
        let (tx, rx) = mpsc::channel::<Message>();
        let (ready_tx, ready_rx) = mpsc::channel::<DotsResult<()>>();
        let thread = std::thread::Builder::new()
            .name("dots-storage".into())
            .spawn(move || {
                let mut storage = match Storage::open(options) {
                    Ok(s) => {
                        let _ = ready_tx.send(Ok(()));
                        s
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                while let Ok(msg) = rx.recv() {
                    match msg {
                        Message::Run(job) => job(&mut storage),
                        Message::Stop => break,
                    }
                }
                if let Err(e) = storage.checkpoint() {
                    tracing::warn!(error = %e, "checkpoint final do WAL falhou");
                }
            })
            .map_err(|e| DotsError::Internal(format!("falha ao criar thread de storage: {e}")))?;
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(DotsError::Internal("thread de storage encerrou antes de abrir".into())),
        }
        Ok(StorageActor { handle: StorageHandle { tx }, thread: Some(thread) })
    }

    /// Executa `f` na thread do ator e aguarda o resultado (assincrono).
    pub async fn call<T, F>(&self, f: F) -> DotsResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Storage) -> DotsResult<T> + Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Message::Run(Box::new(move |s| {
                let _ = tx.send(f(s));
            })))
            .map_err(|_| DotsError::Internal("ator de storage encerrado".into()))?;
        rx.await
            .map_err(|_| DotsError::Internal("ator de storage nao respondeu".into()))?
    }

    /// Versao sincrona de [`Self::call`] (uso em testes e encerramento).
    pub fn call_blocking<T, F>(&self, f: F) -> DotsResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Storage) -> DotsResult<T> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Message::Run(Box::new(move |s| {
                let _ = tx.send(f(s));
            })))
            .map_err(|_| DotsError::Internal("ator de storage encerrado".into()))?;
        rx.recv()
            .map_err(|_| DotsError::Internal("ator de storage nao respondeu".into()))?
    }
}

impl StorageActor {
    /// Encerra o ator apos processar as mensagens ja enfileiradas e faz o
    /// checkpoint final do WAL. Chamadas posteriores em qualquer handle falham.
    pub fn shutdown(mut self) {
        let _ = self.handle.tx.send(Message::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
