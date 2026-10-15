use std::sync::Arc;

use tokio::sync::Notify;

/// Sinalizador de encerramento compartilhado.
#[derive(Clone)]
pub struct Shutdown {
    notify: Arc<Notify>,
    triggered: Arc<std::sync::atomic::AtomicBool>,
}

impl Shutdown {
    pub fn new() -> Self {
        Self { notify: Arc::new(Notify::new()), triggered: Arc::new(std::sync::atomic::AtomicBool::new(false)) }
    }

    pub fn trigger(&self, reason: &str) {
        if !self.triggered.swap(true, std::sync::atomic::Ordering::SeqCst) {
            tracing::info!(reason, "encerramento solicitado");
            self.notify.notify_waiters();
            self.notify.notify_one();
        }
    }

    pub fn is_triggered(&self) -> bool {
        self.triggered.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub async fn wait(self) {
        if self.is_triggered() {
            return;
        }
        self.notify.notified().await;
    }
}

pub async fn wait_for_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM");
    let mut int = signal(SignalKind::interrupt()).expect("SIGINT");
    let mut hup = signal(SignalKind::hangup()).expect("SIGHUP");
    tokio::select! {
        _ = term.recv() => {},
        _ = int.recv() => {},
        _ = hup.recv() => {},
    }
}
