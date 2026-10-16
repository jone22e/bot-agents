//! Primeiro trabalho de cada DOT e acompanhamento periodico dos gerentes.

use std::sync::Arc;
use std::time::Duration;

use dots_core::{DotRole, DotStatus, DotsResult, Schedule, ScheduleCreate, ScheduleKind, TaskCreate, TaskKind};
use serde_json::json;

use crate::schedules::next_run;
use crate::service::RuntimeService;

const MANAGER_INTERVAL_MINUTES: &str = "60";
const MANAGER_PROMPT: &str = "Reavalie seu objetivo permanente e o estado da sua equipe. Consulte entregas e tarefas recentes, continue os próximos passos seguros e delegue quando fizer sentido. Acompanhe os resultados dos funcionários. Não repita análises concluídas sem fatos novos. Se houver bloqueio ou necessidade de autorização, informe exatamente o que falta e peça a decisão ao usuário. Respeite suas instruções permanentes e permissões.";
const MANAGER_START: &str = "Inicie ou retome seu objetivo permanente de forma autônoma. Consulte o histórico recente para não repetir o que já foi concluído. Revise o trabalho da equipe, delegue próximos passos seguros aos funcionários, acompanhe as respostas e avance no que estiver autorizado. Se estiver bloqueado, explique a causa e peça a decisão necessária ao usuário. Respeite suas instruções permanentes e permissões.";
const WORKER_START: &str = "Inicie seu objetivo permanente por conta própria. Consulte o histórico recente, escolha um primeiro passo concreto e seguro e execute o que estiver autorizado. Se depender do gerente, de dados ou de autorização, explique exatamente o que falta. Respeite suas instruções permanentes e permissões.";

fn start_key(id: &str) -> String { format!("autonomy.start.v1.{id}") }
fn manager_schedule_key(id: &str) -> String { format!("autonomy.manager_schedule.v1.{id}") }

pub async fn run(service: Arc<RuntimeService>) {
    loop {
        match service.storage.call(|st| st.dots().list(false)).await {
            Ok(dots) => {
                let now = dots_core::now();
                for dot in dots {
                    // O assistente de criacao ainda pode estar configurando politica, Flexi e equipe.
                    if dot.created_at >= service.started_at && now - dot.created_at < chrono::Duration::seconds(30) { continue; }
                    if let Err(e) = service.start_autonomy(&dot.id).await {
                        tracing::warn!(dot = %dot.id, error = %e, "inicio automatico falhou; sera tentado novamente");
                    }
                }
            }
            Err(e) => tracing::warn!(error = %e, "nao foi possivel verificar DOTs para inicio automatico"),
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(30)) => {},
            _ = service.shutdown.clone().wait() => break,
        }
    }
}

impl RuntimeService {
    /// Chamado pelo assistente apos toda a configuracao; o loop acima cobre DOTs antigos e criacoes externas.
    pub async fn start_autonomy(&self, id: &str) -> DotsResult<()> {
        let _guard = self.autonomy_lock.lock().await;
        let dot = self.storage.call({ let id = id.to_string(); move |st| st.dots().get(&id) }).await?;
        if dot.status != DotStatus::Active || dot.primary_engine.is_none() ||
            (dot.objective.trim().is_empty() && dot.instructions.trim().is_empty()) {
            return Ok(());
        }

        if dot.role == DotRole::Supervisor {
            let key = manager_schedule_key(id);
            let recorded = self.storage.call({ let key = key.clone(); move |st| st.settings().get(&key) }).await?.is_some();
            if !recorded {
                let schedules = self.storage.call({ let id = id.to_string(); move |st| st.schedules().list(Some(&id)) }).await?;
                let marker = if let Some(existing) = schedules.iter().find(|s| s.prompt == MANAGER_PROMPT) {
                    existing.id.clone()
                } else {
                    let next = next_run(ScheduleKind::Every, MANAGER_INTERVAL_MINUTES, &dots_core::now())?;
                    let input = ScheduleCreate {
                        dot_id: id.to_string(), kind: ScheduleKind::Every,
                        spec: MANAGER_INTERVAL_MINUTES.into(), prompt: MANAGER_PROMPT.into(),
                    };
                    let schedule = self.storage.call(move |st| st.schedules().create(input, Some(&next))).await?;
                    self.schedules_notify.notify_one();
                    self.events.emit("automation.changed", json!({ "dot_id": id }));
                    self.audit("schedule.create", "dot", Some(id), json!({ "id": schedule.id, "by": "autonomy" })).await?;
                    schedule.id
                };
                self.storage.call(move |st| st.settings().set(&key, &json!(marker))).await?;
            }
        }

        let key = start_key(id);
        if self.storage.call({ let key = key.clone(); move |st| st.settings().get(&key) }).await?.is_some() {
            return Ok(());
        }
        let origin = format!("autonomy:start:{id}");
        let (already_created, has_any, has_open) = self.storage.call({
            let id = id.to_string();
            let origin = origin.clone();
            move |st| Ok((st.tasks().exists_origin(&origin)?, st.tasks().has_any_for_dot(&id)?, st.tasks().has_open_for_dot(&id)?))
        }).await?;
        let marker = if already_created || (dot.role == DotRole::Supervisor && has_open) || (dot.role == DotRole::Worker && has_any) {
            "existing".to_string()
        } else {
            let prompt = if dot.role == DotRole::Supervisor { MANAGER_START } else { WORKER_START };
            let task = self.create_task(TaskCreate {
                dot_id: id.to_string(), prompt: prompt.into(),
                title: Some(if has_any { "Retomar objetivo" } else { "Iniciar trabalho" }.into()),
                kind: Some(TaskKind::Task), origin: Some(origin), ..Default::default()
            }).await?;
            task.id
        };
        self.storage.call(move |st| st.settings().set(&key, &json!(marker))).await?;
        Ok(())
    }

    /// A agenda padrao nao empilha tarefas se o gerente ja esta ocupado ou pausado.
    pub async fn skip_autonomy_schedule(&self, schedule: &Schedule) -> DotsResult<bool> {
        let key = manager_schedule_key(&schedule.dot_id);
        let (marker, dot, open) = self.storage.call({
            let id = schedule.dot_id.clone();
            move |st| Ok((st.settings().get(&key)?, st.dots().get(&id)?, st.tasks().has_open_for_dot(&id)?))
        }).await?;
        Ok(marker.as_ref().and_then(|s| s.value.as_str()) == Some(schedule.id.as_str())
            && (dot.status != DotStatus::Active || open))
    }
}
