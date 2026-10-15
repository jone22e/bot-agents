use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dots_core::{DotsError, DotsResult, EngineKind, ExecutionEvent};
use tokio::sync::{Semaphore, mpsc};

use crate::adapters::{ParserState, adapter_for};
use crate::request::ExecutionRequest;
use crate::runner::{RunnerEvent, spawn_prepared};

pub type ExecutionStream = mpsc::Receiver<ExecutionEvent>;

/// Controle de uma execucao em andamento.
pub struct ExecutionHandle {
    pub pid: u32,
    process: crate::runner::ProcessHandle,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl ExecutionHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
        self.process.cancel();
    }
}

/// Gerencia adaptadores e limites de concorrencia por motor e global.
pub struct EngineManager {
    global: Arc<Semaphore>,
    per_engine: HashMap<EngineKind, Arc<Semaphore>>,
    grace: Duration,
}

impl EngineManager {
    pub fn new(max_global: usize, max_per_engine: usize) -> Self {
        let mut per_engine = HashMap::new();
        for k in EngineKind::ALL {
            per_engine.insert(k, Arc::new(Semaphore::new(max_per_engine.max(1))));
        }
        Self { global: Arc::new(Semaphore::new(max_global.max(1))), per_engine, grace: Duration::from_secs(5) }
    }

    pub fn available_slots(&self) -> usize {
        self.global.available_permits()
    }

    /// Inicia a execucao assim que houver vaga. Os eventos normalizados chegam pelo stream;
    /// o ultimo e sempre `Completed` ou `Failed`.
    pub async fn execute(&self, req: ExecutionRequest) -> DotsResult<(ExecutionHandle, ExecutionStream)> {
        let global = self.global.clone().acquire_owned().await.map_err(|_| DotsError::Internal("semaforo fechado".into()))?;
        let engine_sem = self.per_engine.get(&req.engine).cloned().ok_or_else(|| DotsError::Engine("motor desconhecido".into()))?;
        let engine_permit = engine_sem.acquire_owned().await.map_err(|_| DotsError::Internal("semaforo fechado".into()))?;

        let adapter = adapter_for(req.engine);
        let prepared = adapter.prepare(&req)?;
        tracing::info!(execution = %req.execution_id, engine = req.engine.as_str(), program = %prepared.program.display(), "iniciando motor");
        tracing::debug!(args = ?prepared.args, "argumentos do motor");
        let (process, mut raw) = spawn_prepared(prepared, req.timeout, self.grace).await?;
        let pid = process.pid;
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled_task = cancelled.clone();

        let (tx, rx) = mpsc::channel::<ExecutionEvent>(1024);
        tokio::spawn(async move {
            let _permits = (global, engine_permit);
            let mut st = ParserState::default();
            while let Some(ev) = raw.recv().await {
                match ev {
                    RunnerEvent::Stdout(line) => {
                        for e in adapter.parse_line(&line, &mut st) {
                            if tx.send(e).await.is_err() {
                                return;
                            }
                        }
                    }
                    RunnerEvent::Stderr(line) => {
                        st.note_stderr(&line);
                        tracing::debug!(%line, "stderr do motor");
                    }
                    RunnerEvent::Exited { code, timed_out, killed } => {
                        let was_cancelled = cancelled_task.load(std::sync::atomic::Ordering::SeqCst);
                        tracing::info!(code = ?code, timed_out, killed, cancelled = was_cancelled, "motor encerrou");
                        for e in adapter.finish(code, timed_out, was_cancelled, &mut st) {
                            let _ = tx.send(e).await;
                        }
                        // Garantia: se o adaptador marcou completed mas nao emitiu Completed, nada a fazer;
                        // o consumidor trata o fim do stream.
                        break;
                    }
                }
            }
        });
        Ok((ExecutionHandle { pid, process, cancelled }, rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// CLI falso que reproduz a captura real do Claude.
    fn fake_claude(dir: &std::path::Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/engines/fixtures/claude-print-ok.jsonl");
        let p = dir.join("claude");
        std::fs::write(&p, format!("#!/bin/sh\ncat >/dev/null\nwhile IFS= read -r l; do echo \"$l\"; sleep 0.01; done < {fixture}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[tokio::test]
    async fn executes_fake_engine_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let exe = fake_claude(dir.path());
        let mut req = crate::adapters::test_support::request(EngineKind::Claude);
        req.executable = exe;
        req.workspace = dir.path().to_path_buf();
        req.path_dirs = vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")];
        let m = EngineManager::new(2, 1);
        let (h, mut rx) = m.execute(req).await.unwrap();
        assert!(h.pid > 0);
        let mut evs = vec![];
        while let Some(e) = rx.recv().await {
            evs.push(e);
        }
        assert!(matches!(evs.first(), Some(ExecutionEvent::Started { .. })));
        assert!(matches!(evs.last(), Some(ExecutionEvent::Completed { result_text }) if result_text == "OK"));
    }

    #[tokio::test]
    async fn cancel_yields_failed_cancelled() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("claude");
        std::fs::write(&p, "#!/bin/sh\ncat >/dev/null\necho '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"s\"}'\nsleep 30\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut req = crate::adapters::test_support::request(EngineKind::Claude);
        req.executable = p;
        req.workspace = dir.path().to_path_buf();
        let m = EngineManager::new(1, 1);
        let (h, mut rx) = m.execute(req).await.unwrap();
        let first = rx.recv().await.unwrap();
        assert!(matches!(first, ExecutionEvent::Started { .. }));
        h.cancel();
        let mut last = None;
        while let Some(e) = rx.recv().await {
            last = Some(e);
        }
        assert!(matches!(last, Some(ExecutionEvent::Failed { kind: dots_core::FailureKind::Cancelled, .. })));
    }
}
