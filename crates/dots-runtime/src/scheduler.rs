//! Agendador: acorda por notificacao (nunca por polling de modelo), respeita status do DOT,
//! concorrencia por DOT e global, e expira aprovacoes.

use std::sync::Arc;
use std::time::Duration;

use dots_core::{DotStatus, TaskStatus};
use serde_json::json;

use crate::service::RuntimeService;

pub async fn run(service: Arc<RuntimeService>) {
    loop {
        let wake = service.scheduler_notify.notified();
        tokio::pin!(wake);
        tokio::select! {
            _ = &mut wake => {},
            _ = tokio::time::sleep(Duration::from_secs(30)) => {},
            _ = service.shutdown.clone().wait() => break,
        }
        if service.shutdown.is_triggered() {
            break;
        }
        if let Ok(expired) = service.storage.call(|s| s.approvals().expire_due()).await {
            for id in expired {
                service.registry.resolve_approval(&id, false);
                service.events.emit("approval.resolved", json!({ "id": id, "status": "expired" }));
                service.on_question_expired(&id).await;
            }
        }
        if let Err(e) = dispatch(&service).await {
            tracing::warn!(error = %e, "scheduler: falha ao despachar");
        }
    }
    tracing::info!("scheduler encerrado");
}

async fn dispatch(service: &Arc<RuntimeService>) -> dots_core::DotsResult<()> {
    if service.paused.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    let queued = service.storage.call(|s| s.tasks().queued(50)).await?;
    for task in queued {
        if service.engines.available_slots() == 0 {
            break;
        }
        if task.cancel_requested {
            let id = task.id.clone();
            let _ = service.storage.call(move |s| s.tasks().set_status(&id, TaskStatus::Cancelled, Some("cancelada antes de iniciar"))).await;
            service.events.emit("task.changed", json!({ "id": task.id, "dot_id": task.dot_id, "status": "CANCELLED" }));
            continue;
        }
        let dot_id = task.dot_id.clone();
        let (dot, running) = service
            .storage
            .call(move |s| {
                let dot = s.dots().get(&dot_id)?;
                let running = s.tasks().running_count_for_dot(&dot_id)?;
                Ok((dot, running))
            })
            .await?;
        if dot.status != DotStatus::Active {
            continue;
        }
        if running >= dot.limits.max_concurrent_tasks {
            continue;
        }
        tracing::info!(task = %task.id, dot = %dot.name, "despachando tarefa");
        let svc = service.clone();
        tokio::spawn(async move { svc.run_task(task).await });
        // Evita despachar duas tarefas do mesmo DOT antes da primeira registrar RUNNING.
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}
