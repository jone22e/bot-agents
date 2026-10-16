//! Agendamentos explicitos: calcula o proximo horario e acorda apenas nele.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Local, TimeZone};
use dots_core::automation::event_types as et;
use dots_core::{DotsError, DotsResult, MessageRole, Schedule, ScheduleCreate, ScheduleKind, TaskCreate, TaskKind, Timestamp};
use serde_json::json;

use crate::policy::SharedContext;
use crate::service::RuntimeService;
use crate::tools::ToolOutput;

/// Proxima ocorrencia apos `after` (horario local do usuario).
pub fn next_run(kind: ScheduleKind, spec: &str, after: &Timestamp) -> DotsResult<Timestamp> {
    let local = after.with_timezone(&Local);
    match kind {
        ScheduleKind::Every => {
            let minutes: i64 = spec.trim().parse().map_err(|_| DotsError::Validation("intervalo em minutos invalido".into()))?;
            if minutes < 1 {
                return Err(DotsError::Validation("intervalo minimo e 1 minuto".into()));
            }
            Ok(*after + chrono::Duration::minutes(minutes))
        }
        ScheduleKind::Daily => {
            let (h, m) = parse_hm(spec)?;
            let mut candidate = Local.with_ymd_and_hms(local.year(), local.month(), local.day(), h, m, 0).single().ok_or_else(|| DotsError::Validation("horario invalido".into()))?;
            if candidate <= local {
                candidate += chrono::Duration::days(1);
            }
            Ok(candidate.with_timezone(&chrono::Utc))
        }
        ScheduleKind::Weekly => {
            let mut parts = spec.split_whitespace();
            let dow: u32 = parts.next().and_then(|d| d.parse().ok()).filter(|d| *d <= 6).ok_or_else(|| DotsError::Validation("dia da semana invalido (0-6)".into()))?;
            let (h, m) = parse_hm(parts.next().unwrap_or(""))?;
            let today = local.weekday().num_days_from_sunday();
            let mut delta = (dow as i64 - today as i64).rem_euclid(7);
            let mut candidate = Local.with_ymd_and_hms(local.year(), local.month(), local.day(), h, m, 0).single().ok_or_else(|| DotsError::Validation("horario invalido".into()))? + chrono::Duration::days(delta);
            if candidate <= local {
                delta += 7;
                candidate = Local.with_ymd_and_hms(local.year(), local.month(), local.day(), h, m, 0).single().unwrap() + chrono::Duration::days(delta);
            }
            Ok(candidate.with_timezone(&chrono::Utc))
        }
        ScheduleKind::Cron => {
            let cron = croner::Cron::from_str(spec.trim()).map_err(|e| DotsError::Validation(format!("expressao cron invalida: {e}")))?;
            let next = cron.find_next_occurrence(&local, false).map_err(|e| DotsError::Validation(format!("cron sem proxima ocorrencia: {e}")))?;
            Ok(next.with_timezone(&chrono::Utc))
        }
    }
}

fn parse_hm(s: &str) -> DotsResult<(u32, u32)> {
    let (h, m) = s.trim().split_once(':').ok_or_else(|| DotsError::Validation("use o formato HH:MM".into()))?;
    let h: u32 = h.parse().map_err(|_| DotsError::Validation("hora invalida".into()))?;
    let m: u32 = m.parse().map_err(|_| DotsError::Validation("minuto invalido".into()))?;
    if h > 23 || m > 59 {
        return Err(DotsError::Validation("horario fora do intervalo".into()));
    }
    Ok((h, m))
}

pub fn describe(s: &Schedule) -> String {
    match s.kind {
        ScheduleKind::Every => format!("a cada {} min", s.spec),
        ScheduleKind::Daily => format!("todo dia as {}", s.spec),
        ScheduleKind::Weekly => {
            let names = ["domingo", "segunda", "terca", "quarta", "quinta", "sexta", "sabado"];
            let mut p = s.spec.split_whitespace();
            let d: usize = p.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            format!("toda {} as {}", names.get(d).unwrap_or(&"?"), p.next().unwrap_or("?"))
        }
        ScheduleKind::Cron => format!("cron {}", s.spec),
    }
}

/// Loop do agendador: dorme ate o proximo `next_run_at` (ou ate ser acordado) e dispara os vencidos.
pub async fn run(service: Arc<RuntimeService>) {
    loop {
        let next = service.storage.call(|st| st.schedules().next_due()).await.ok().flatten();
        let sleep_for = match next {
            Some(t) => (t - dots_core::now()).to_std().unwrap_or(Duration::ZERO).min(Duration::from_secs(3600)),
            None => Duration::from_secs(3600),
        };
        let wake = service.schedules_notify.notified();
        tokio::pin!(wake);
        tokio::select! {
            _ = &mut wake => {},
            _ = tokio::time::sleep(sleep_for) => {},
            _ = service.shutdown.clone().wait() => break,
        }
        if service.shutdown.is_triggered() {
            break;
        }
        let now = dots_core::now();
        let due = match service.storage.call(move |st| st.schedules().due(&now)).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "agendador: falha ao consultar");
                continue;
            }
        };
        for s in due {
            let _autonomy_guard = service.autonomy_lock.lock().await;
            let next = next_run(s.kind, &s.spec, &dots_core::now()).ok();
            let (id, n) = (s.id.clone(), next);
            let _ = service.storage.call(move |st| st.schedules().mark_run(&id, n.as_ref())).await;
            match service.skip_autonomy_schedule(&s).await {
                Ok(true) => continue,
                Err(e) => {
                    tracing::warn!(schedule = %s.id, error = %e, "falha ao verificar agenda automatica");
                    continue;
                }
                Ok(false) => {}
            }
            let key = format!("schedule:{}:{}", s.id, s.next_run_at.map(|t| t.timestamp()).unwrap_or(0));
            let ev = service.emit_event(et::SCHEDULE_DUE, "scheduler", json!({ "schedule_id": s.id, "dot_id": s.dot_id, "prompt": s.prompt }), None, Some(key), 0).await;
            if let Ok(Some(ev)) = ev {
                let input = TaskCreate { dot_id: s.dot_id.clone(), prompt: s.prompt.clone(), title: Some(format!("Agendado: {}", describe(&s))), kind: Some(TaskKind::Task), origin: Some(format!("schedule:{}", s.id)), correlation_id: Some(ev.id), ..Default::default() };
                match service.create_task(input).await {
                    Ok(t) => tracing::info!(schedule = %s.id, task = %t.id, "agendamento disparou tarefa"),
                    Err(e) => tracing::warn!(schedule = %s.id, error = %e, "agendamento nao criou tarefa"),
                }
            }
        }
    }
}

/// Limites dos agendamentos criados pelo proprio DOT (evitam um laco de tarefas).
const DOT_MAX_SCHEDULES: usize = 10;
const DOT_MIN_EVERY_MINUTES: i64 = 5;

fn local_time(t: Option<Timestamp>) -> String {
    t.map(|t| t.with_timezone(&Local).format("%d/%m %H:%M").to_string()).unwrap_or_else(|| "?".into())
}

/// Ferramentas `dots_schedule_*`: o DOT agenda o proprio trabalho (o que prometeu para depois) e avisa na conversa.
impl RuntimeService {
    /// Nota na conversa da tarefa em andamento (o usuario ve o que o DOT agendou ou cancelou).
    async fn schedule_note(&self, ctx: &SharedContext, text: String, sch: &Schedule) {
        let Ok(task) = self.storage.call({ let t = ctx.task_id.clone(); move |st| st.tasks().get(&t) }).await else { return };
        let Some(cid) = task.conversation_id else { return };
        let meta = json!({ "origin": format!("schedule:{}", sch.id), "prompt": sch.prompt.chars().take(4000).collect::<String>() });
        let c = cid.clone();
        if self.storage.call(move |st| st.conversations().add_message(&c, MessageRole::System, &text, None, None, meta)).await.is_ok() {
            self.events.emit("message.changed", json!({ "conversation_id": cid, "dot_id": ctx.dot_id }));
        }
    }

    pub async fn schedule_create_for_dot(&self, ctx: &SharedContext, kind: &str, spec: &str, prompt: &str) -> DotsResult<ToolOutput> {
        let Ok(kind) = ScheduleKind::parse(kind.trim()) else {
            return Ok(ToolOutput::err("kind invalido: use every, daily, weekly ou cron".into()));
        };
        let (spec, prompt) = (spec.trim().to_string(), prompt.trim().to_string());
        if prompt.is_empty() {
            return Ok(ToolOutput::err("informe o pedido (prompt) de cada disparo".into()));
        }
        if kind == ScheduleKind::Every && spec.parse::<i64>().is_ok_and(|m| m < DOT_MIN_EVERY_MINUTES) {
            return Ok(ToolOutput::err(format!("intervalo minimo para agendamento do DOT: {DOT_MIN_EVERY_MINUTES} minutos")));
        }
        let next = match next_run(kind, &spec, &dots_core::now()) {
            Ok(n) => n,
            Err(e) => return Ok(ToolOutput::err(e.to_string())),
        };
        let existing = self.storage.call({ let d = ctx.dot_id.clone(); move |st| st.schedules().list(Some(&d)) }).await?;
        if let Some(same) = existing.iter().find(|x| x.enabled && x.kind == kind && x.spec == spec && x.prompt == prompt) {
            return Ok(ToolOutput::ok(format!("este agendamento ja existe ({}): {}; proximo disparo {}", same.id, describe(same), local_time(same.next_run_at))));
        }
        if existing.iter().filter(|x| x.enabled).count() >= DOT_MAX_SCHEDULES {
            return Ok(ToolOutput::err(format!("limite de {DOT_MAX_SCHEDULES} agendamentos ativos: cancele um com dots_schedule_cancel (veja dots_schedule_list)")));
        }
        let input = ScheduleCreate { dot_id: ctx.dot_id.clone(), kind, spec, prompt };
        let sch = self.storage.call(move |st| st.schedules().create(input, Some(&next))).await?;
        self.audit("schedule.create", "dot", Some(&sch.dot_id), json!({ "id": sch.id, "kind": sch.kind.as_str(), "spec": sch.spec, "by": "dot", "task_id": ctx.task_id })).await?;
        self.schedules_notify.notify_one();
        self.events.emit("automation.changed", json!({ "dot_id": sch.dot_id }));
        self.schedule_note(ctx, format!("{} agendou: {} (próximo: {})", ctx.dot_name, describe(&sch), local_time(sch.next_run_at)), &sch).await;
        Ok(ToolOutput::ok(format!("agendado ({}): {}; proximo disparo {}. O usuario ve e pode apagar no seu perfil; para cancelar use dots_schedule_cancel.", sch.id, describe(&sch), local_time(sch.next_run_at))))
    }

    pub async fn schedule_list_for_dot(&self, ctx: &SharedContext) -> DotsResult<ToolOutput> {
        let list = self.storage.call({ let d = ctx.dot_id.clone(); move |st| st.schedules().list(Some(&d)) }).await?;
        if list.is_empty() {
            return Ok(ToolOutput::ok("nenhum agendamento".into()));
        }
        Ok(ToolOutput::ok(
            list.iter()
                .map(|x| format!("- {} | {} | {} | proximo: {} | pedido: {}", x.id, describe(x), if x.enabled { "ativo" } else { "desligado pelo usuario" }, local_time(x.next_run_at), x.prompt.chars().take(200).collect::<String>()))
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }

    pub async fn schedule_cancel_for_dot(&self, ctx: &SharedContext, id: &str) -> DotsResult<ToolOutput> {
        let sch = match self.storage.call({ let id = id.trim().to_string(); move |st| st.schedules().get(&id) }).await {
            Ok(s) if s.dot_id == ctx.dot_id => s,
            _ => return Ok(ToolOutput::err(format!("agendamento '{id}' nao encontrado entre os seus (veja dots_schedule_list)"))),
        };
        self.storage.call({ let id = sch.id.clone(); move |st| st.schedules().delete(&id) }).await?;
        self.audit("schedule.delete", "dot", Some(&sch.dot_id), json!({ "id": sch.id, "by": "dot", "task_id": ctx.task_id })).await?;
        self.schedules_notify.notify_one();
        self.events.emit("automation.changed", json!({ "dot_id": sch.dot_id }));
        self.schedule_note(ctx, format!("{} cancelou o agendamento: {}", ctx.dot_name, describe(&sch)), &sch).await;
        Ok(ToolOutput::ok(format!("agendamento cancelado: {}", describe(&sch))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_next_runs() {
        let now = dots_core::now();
        assert_eq!((next_run(ScheduleKind::Every, "15", &now).unwrap() - now).num_minutes(), 15);
        let d = next_run(ScheduleKind::Daily, "09:30", &now).unwrap();
        assert!(d > now && (d - now).num_hours() <= 24);
        assert_eq!(d.with_timezone(&Local).format("%H:%M").to_string(), "09:30");
        let w = next_run(ScheduleKind::Weekly, "1 08:00", &now).unwrap();
        assert!(w > now && (w - now).num_days() <= 7);
        assert_eq!(w.with_timezone(&Local).weekday().num_days_from_sunday(), 1);
        let c = next_run(ScheduleKind::Cron, "*/5 * * * *", &now).unwrap();
        assert!(c > now && (c - now).num_minutes() <= 5);
        assert!(next_run(ScheduleKind::Cron, "nope", &now).is_err());
        assert!(next_run(ScheduleKind::Daily, "25:00", &now).is_err());
        assert!(next_run(ScheduleKind::Every, "0", &now).is_err());
    }
}
