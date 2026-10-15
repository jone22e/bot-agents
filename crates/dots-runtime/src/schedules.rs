//! Agendamentos explicitos: calcula o proximo horario e acorda apenas nele.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Local, TimeZone};
use dots_core::automation::event_types as et;
use dots_core::{DotsError, DotsResult, Schedule, ScheduleKind, TaskCreate, TaskKind, Timestamp};
use serde_json::json;

use crate::service::RuntimeService;

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
            let next = next_run(s.kind, &s.spec, &dots_core::now()).ok();
            let (id, n) = (s.id.clone(), next);
            let _ = service.storage.call(move |st| st.schedules().mark_run(&id, n.as_ref())).await;
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
