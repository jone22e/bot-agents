//! Metas (ADR 0017): ferramentas `dots_goal_*` com que o gerente registra a meta que o usuario declarou na
//! conversa, lanca o progresso a cada rodada e altera alvo ou prazo quando o usuario muda de ideia (sem recriar). Cada acao deixa uma nota na conversa (origem `goal:<id>`) e
//! emite `goal.changed`, que o `flexi_sync` espelha no Flexi web.

use chrono::Local;
use dots_core::{DotsResult, Goal, GoalCreate, GoalStatus, MessageRole, Timestamp};
use dots_storage::repos::GoalEdit;
use serde_json::json;

use crate::policy::SharedContext;
use crate::service::RuntimeService;
use crate::tools::ToolOutput;

pub const GOAL_EVENT: &str = "goal.changed";
const DOT_MAX_ACTIVE_GOALS: usize = 10;
const HISTORY_LIMIT: u32 = 200;

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v:.2}").replace('.', ",") }
}

/// "7 de 10 envios" (ou "7 de 10" sem unidade).
pub fn describe(g: &Goal) -> String {
    let unit = if g.unit.trim().is_empty() { String::new() } else { format!(" {}", g.unit.trim()) };
    format!("{}: {} de {}{}", g.title, fmt_num(g.current), fmt_num(g.target), unit)
}

fn local_date(t: Option<Timestamp>) -> String {
    t.map(|t| t.with_timezone(&Local).format("%d/%m %H:%M").to_string()).unwrap_or_else(|| "sem prazo".into())
}

/// Prazo aceito pelo DOT: data (AAAA-MM-DD, fim do dia local) ou data e hora (AAAA-MM-DD HH:MM, local).
pub fn parse_deadline(spec: &str) -> Result<Option<Timestamp>, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Ok(None);
    }
    use chrono::{NaiveDate, NaiveDateTime, TimeZone};
    let naive = NaiveDateTime::parse_from_str(spec, "%Y-%m-%d %H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(spec, "%Y-%m-%dT%H:%M"))
        .or_else(|_| NaiveDate::parse_from_str(spec, "%Y-%m-%d").map(|d| d.and_hms_opt(23, 59, 59).unwrap()))
        .map_err(|_| "prazo invalido: use AAAA-MM-DD ou AAAA-MM-DD HH:MM".to_string())?;
    let local = Local.from_local_datetime(&naive).single().ok_or_else(|| "prazo invalido".to_string())?;
    Ok(Some(local.with_timezone(&chrono::Utc)))
}

impl RuntimeService {
    /// Funcionario de equipe nao registra metas: quem acompanha a meta e o gerente dele.
    async fn goal_manager_block(&self, ctx: &SharedContext) -> DotsResult<Option<String>> {
        let d = ctx.dot_id.clone();
        let manager = self
            .storage
            .call(move |st| {
                if st.teams().managed_by(&d)?.is_some() {
                    return Ok(None);
                }
                let Some(team) = st.teams().member_of(&d)? else { return Ok(None) };
                let Some(mid) = team.manager_dot_id.filter(|m| m != &d) else { return Ok(None) };
                Ok(Some(st.dots().get(&mid)?.name))
            })
            .await?;
        Ok(manager.map(|m| format!("so o gerente registra e acompanha metas. Voce e funcionario de {m}: relate os numeros no resultado da tarefa (ele chega ao seu gerente) e nao use dots_goal_*.")))
    }

    async fn goal_note(&self, ctx: &SharedContext, text: String, goal: &Goal) {
        let Ok(task) = self.storage.call({ let t = ctx.task_id.clone(); move |st| st.tasks().get(&t) }).await else { return };
        let Some(cid) = task.conversation_id else { return };
        let meta = json!({ "origin": format!("goal:{}", goal.id), "prompt": describe(goal) });
        let c = cid.clone();
        if self.storage.call(move |st| st.conversations().add_message(&c, MessageRole::System, &text, None, None, meta)).await.is_ok() {
            self.events.emit("message.changed", json!({ "conversation_id": cid, "dot_id": ctx.dot_id }));
        }
    }

    fn goal_changed(&self, goal: &Goal) {
        self.events.emit(GOAL_EVENT, json!({ "id": goal.id, "dot_id": goal.dot_id, "status": goal.status.as_str() }));
    }

    pub async fn goal_create_for_dot(&self, ctx: &SharedContext, title: &str, target: f64, unit: &str, current: f64, deadline: &str) -> DotsResult<ToolOutput> {
        if let Some(block) = self.goal_manager_block(ctx).await? {
            return Ok(ToolOutput::err(block));
        }
        let period_end = match parse_deadline(deadline) {
            Ok(d) => d,
            Err(e) => return Ok(ToolOutput::err(e)),
        };
        let existing = self.storage.call({ let d = ctx.dot_id.clone(); move |st| st.goals().list(Some(&d), true) }).await?;
        if let Some(same) = existing.iter().find(|g| g.title.trim().eq_ignore_ascii_case(title.trim())) {
            return Ok(ToolOutput::ok(if (same.target - target).abs() < 1e-9 {
                format!("esta meta ja existe ({}): {}; lance o progresso com dots_goal_progress", same.id, describe(same))
            } else {
                format!("ja existe uma meta ativa com este titulo ({}): {}; para mudar o alvo ou o prazo use dots_goal_update em vez de criar outra", same.id, describe(same))
            }));
        }
        if existing.len() >= DOT_MAX_ACTIVE_GOALS {
            return Ok(ToolOutput::err(format!("limite de {DOT_MAX_ACTIVE_GOALS} metas ativas: encerre uma com dots_goal_close (veja dots_goal_list)")));
        }
        let input = GoalCreate { dot_id: ctx.dot_id.clone(), title: title.trim().chars().take(160).collect(), unit: unit.trim().chars().take(30).collect(), target, current, period_end, task_id: Some(ctx.task_id.clone()) };
        let goal = match self.storage.call(move |st| st.goals().create(input)).await {
            Ok(g) => g,
            Err(dots_core::DotsError::Validation(m)) => return Ok(ToolOutput::err(m)),
            Err(e) => return Err(e),
        };
        self.audit("goal.create", "dot", Some(&goal.dot_id), json!({ "id": goal.id, "title": goal.title, "target": goal.target, "by": "dot", "task_id": ctx.task_id })).await?;
        self.goal_changed(&goal);
        self.goal_note(ctx, format!("{} criou a meta: {} (prazo: {})", ctx.dot_name, describe(&goal), local_date(goal.period_end)), &goal).await;
        Ok(ToolOutput::ok(format!("meta registrada ({}): {}; prazo {}. Lance o progresso a cada rodada com dots_goal_progress (valor acumulado).", goal.id, describe(&goal), local_date(goal.period_end))))
    }

    pub async fn goal_progress_for_dot(&self, ctx: &SharedContext, id: &str, value: Option<f64>, delta: Option<f64>, note: &str) -> DotsResult<ToolOutput> {
        if let Some(block) = self.goal_manager_block(ctx).await? {
            return Ok(ToolOutput::err(block));
        }
        let goal = match self.storage.call({ let id = id.trim().to_string(); move |st| st.goals().get(&id) }).await {
            Ok(g) if g.dot_id == ctx.dot_id => g,
            _ => return Ok(ToolOutput::err(format!("meta '{id}' nao encontrada entre as suas (veja dots_goal_list)"))),
        };
        let new_value = match (value, delta) {
            (Some(v), _) => v,
            (None, Some(d)) => goal.current + d,
            (None, None) => return Ok(ToolOutput::err("informe value (acumulado) ou delta (quanto avancou desde o ultimo lancamento)".into())),
        };
        let note: String = note.trim().chars().take(400).collect();
        let (goal, upd) = match self.storage.call({ let (id, n, t) = (goal.id.clone(), note.clone(), ctx.task_id.clone()); move |st| st.goals().progress(&id, new_value, &n, Some(&t)) }).await {
            Ok(r) => r,
            Err(dots_core::DotsError::Validation(m)) => return Ok(ToolOutput::err(m)),
            Err(e) => return Err(e),
        };
        self.audit("goal.progress", "dot", Some(&goal.dot_id), json!({ "id": goal.id, "value": upd.value, "delta": upd.delta, "by": "dot", "task_id": ctx.task_id })).await?;
        self.goal_changed(&goal);
        let sign = if upd.delta >= 0.0 { "+" } else { "" };
        let suffix = if note.is_empty() { String::new() } else { format!(" · {note}") };
        self.goal_note(ctx, format!("{} atualizou a meta: {} ({sign}{}){suffix}", ctx.dot_name, describe(&goal), fmt_num(upd.delta)), &goal).await;
        let reached = goal.current >= goal.target;
        Ok(ToolOutput::ok(format!(
            "progresso lancado: {}{}",
            describe(&goal),
            if reached { ". Alvo atingido: encerre com dots_goal_close (status done) se o periodo acabou, ou siga acompanhando." } else { "" }
        )))
    }

    /// Altera alvo, titulo, unidade ou prazo de uma meta ativa; o progresso e o historico ficam.
    pub async fn goal_update_for_dot(&self, ctx: &SharedContext, id: &str, title: &str, target: Option<f64>, unit: Option<&str>, deadline: &str) -> DotsResult<ToolOutput> {
        if let Some(block) = self.goal_manager_block(ctx).await? {
            return Ok(ToolOutput::err(block));
        }
        let goal = match self.storage.call({ let id = id.trim().to_string(); move |st| st.goals().get(&id) }).await {
            Ok(g) if g.dot_id == ctx.dot_id => g,
            _ => return Ok(ToolOutput::err(format!("meta '{id}' nao encontrada entre as suas (veja dots_goal_list)"))),
        };
        let period_end = match deadline.trim() {
            "" => None,
            "sem prazo" | "none" => Some(None),
            spec => match parse_deadline(spec) {
                Ok(d) => Some(d),
                Err(e) => return Ok(ToolOutput::err(e)),
            },
        };
        let title: String = title.trim().chars().take(160).collect();
        let unit: Option<String> = unit.map(|u| u.trim().chars().take(30).collect());
        if title.is_empty() && target.is_none() && unit.is_none() && period_end.is_none() {
            return Ok(ToolOutput::err("informe o que muda: title, target, unit ou deadline".into()));
        }
        let before = describe(&goal);
        let goal = match self
            .storage
            .call({
                let (id, title, unit) = (goal.id.clone(), title, unit);
                move |st| st.goals().update(&id, GoalEdit { title: (!title.is_empty()).then_some(title.as_str()), unit: unit.as_deref(), target, period_end })
            })
            .await
        {
            Ok(g) => g,
            Err(dots_core::DotsError::Validation(m)) => return Ok(ToolOutput::err(m)),
            Err(e) => return Err(e),
        };
        self.audit("goal.update", "dot", Some(&goal.dot_id), json!({ "id": goal.id, "title": goal.title, "target": goal.target, "unit": goal.unit, "period_end": goal.period_end, "by": "dot", "task_id": ctx.task_id })).await?;
        self.goal_changed(&goal);
        self.goal_note(ctx, format!("{} alterou a meta: {} (antes: {}; prazo: {})", ctx.dot_name, describe(&goal), before, local_date(goal.period_end)), &goal).await;
        Ok(ToolOutput::ok(format!("meta alterada ({}): {}; prazo {}. O progresso e o historico foram mantidos.", goal.id, describe(&goal), local_date(goal.period_end))))
    }

    pub async fn goal_list_for_dot(&self, ctx: &SharedContext) -> DotsResult<ToolOutput> {
        let list = self.storage.call({ let d = ctx.dot_id.clone(); move |st| st.goals().list(Some(&d), false) }).await?;
        if list.is_empty() {
            return Ok(ToolOutput::ok("nenhuma meta registrada".into()));
        }
        Ok(ToolOutput::ok(
            list.iter()
                .take(30)
                .map(|g| format!("- {} | {} | {} | prazo: {} | atualizada: {}", g.id, describe(g), g.status.as_str(), local_date(g.period_end), local_date(Some(g.updated_at))))
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }

    pub async fn goal_close_for_dot(&self, ctx: &SharedContext, id: &str, status: &str) -> DotsResult<ToolOutput> {
        if let Some(block) = self.goal_manager_block(ctx).await? {
            return Ok(ToolOutput::err(block));
        }
        let status = match status.trim() {
            "done" | "" => GoalStatus::Done,
            "cancelled" | "canceled" => GoalStatus::Cancelled,
            o => return Ok(ToolOutput::err(format!("status invalido '{o}': use done ou cancelled"))),
        };
        let goal = match self.storage.call({ let id = id.trim().to_string(); move |st| st.goals().get(&id) }).await {
            Ok(g) if g.dot_id == ctx.dot_id => g,
            _ => return Ok(ToolOutput::err(format!("meta '{id}' nao encontrada entre as suas (veja dots_goal_list)"))),
        };
        let goal = match self.storage.call({ let id = goal.id.clone(); move |st| st.goals().close(&id, status) }).await {
            Ok(g) => g,
            Err(dots_core::DotsError::Validation(m)) => return Ok(ToolOutput::err(m)),
            Err(e) => return Err(e),
        };
        self.audit("goal.close", "dot", Some(&goal.dot_id), json!({ "id": goal.id, "status": status.as_str(), "by": "dot", "task_id": ctx.task_id })).await?;
        self.goal_changed(&goal);
        let verb = if status == GoalStatus::Done { "concluiu" } else { "cancelou" };
        self.goal_note(ctx, format!("{} {verb} a meta: {}", ctx.dot_name, describe(&goal)), &goal).await;
        Ok(ToolOutput::ok(format!("meta encerrada ({}): {}", status.as_str(), describe(&goal))))
    }

    /// Metas de todos os DOTs com o historico recente, como o Flexi web as recebe.
    pub async fn goals_snapshot(&self) -> DotsResult<Vec<serde_json::Value>> {
        self.storage
            .call(|st| {
                let goals = st.goals().list(None, false)?;
                let mut out = Vec::with_capacity(goals.len());
                for g in goals {
                    let history = st.goals().history(&g.id, HISTORY_LIMIT)?;
                    out.push(json!({
                        "id": g.id,
                        "agente_id": g.dot_id,
                        "titulo": g.title,
                        "unidade": g.unit,
                        "alvo": g.target,
                        "atual": g.current,
                        "inicio_em": g.period_start,
                        "fim_em": g.period_end,
                        "status": g.status.as_str(),
                        "criada_em": g.created_at,
                        "atualizada_em": g.updated_at,
                        "encerrada_em": g.closed_at,
                        "historico": history.iter().map(|u| json!({ "valor": u.value, "delta": u.delta, "nota": u.note, "em": u.created_at })).collect::<Vec<_>>(),
                    }));
                }
                Ok(out)
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descreve_e_formata_numeros() {
        let g = Goal { id: "g".into(), dot_id: "d".into(), title: "Envios".into(), unit: "envios".into(), target: 10.0, current: 7.0, period_start: dots_core::now(), period_end: None, status: GoalStatus::Active, task_id: None, created_at: dots_core::now(), updated_at: dots_core::now(), closed_at: None };
        assert_eq!(describe(&g), "Envios: 7 de 10 envios");
        assert_eq!(fmt_num(2.5), "2,50");
        assert!(parse_deadline("").unwrap().is_none());
        assert!(parse_deadline("2099-01-31").unwrap().is_some());
        assert!(parse_deadline("2099-01-31 18:00").unwrap().is_some());
        assert!(parse_deadline("amanha").is_err());
    }
}
