//! Avaliacao de desempenho por DOT, calculada no runtime a partir das tarefas encerradas no periodo,
//! das execucoes (custo), das aprovacoes (intervencao humana) e da auditoria de ferramentas (acoes revertidas).
//!
//! Porte fiel de `apps/desktop/src/lib/performance.ts`: as regras e os nomes dos campos sao os mesmos,
//! para que o Flexi web e o app desktop mostrem os mesmos numeros.

use std::collections::{BTreeMap, HashMap, HashSet};

use dots_core::{Approval, AuditLog, Execution, Task, TaskStatus, Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DotPerformance {
    pub dot_id: String,
    pub dot_name: String,
    /// Tarefas encerradas no periodo (concluidas, falhas e canceladas).
    pub finished: u32,
    pub completed: u32,
    pub failed: u32,
    /// Tarefas que precisaram de mais de uma tentativa ou cujo titulo ja tinha falhado antes no periodo.
    pub recurrent: u32,
    /// Custo somado das execucoes iniciadas no periodo (USD).
    pub cost_usd: f64,
    pub executions: u32,
    /// Mediana do tempo entre criacao e encerramento das tarefas concluidas, em ms.
    pub resolution_ms: Option<f64>,
    /// Acoes negadas pela politica, rejeitadas pelo usuario ou desfeitas via git (restore, reset, clean, stash).
    pub reverted: u32,
    /// Tarefas em que o DOT precisou de permissao ou resposta do usuario.
    pub human_tasks: u32,
}

impl DotPerformance {
    fn empty(dot_id: &str, dot_name: &str) -> Self {
        Self {
            dot_id: dot_id.to_string(),
            dot_name: dot_name.to_string(),
            finished: 0,
            completed: 0,
            failed: 0,
            recurrent: 0,
            cost_usd: 0.0,
            executions: 0,
            resolution_ms: None,
            reverted: 0,
            human_tasks: 0,
        }
    }
}

pub struct PerformanceInput<'a> {
    pub tasks: &'a [Task],
    pub executions: &'a [Execution],
    pub approvals: &'a [Approval],
    pub audit: &'a [AuditLog],
    pub dot_names: &'a HashMap<String, String>,
    pub start: Timestamp,
    pub end: Timestamp,
}

const REVERT_GIT_OPS: [&str; 4] = ["restore", "reset", "clean", "stash"];

fn in_period(t: Option<&Timestamp>, start: &Timestamp, end: &Timestamp) -> bool {
    matches!(t, Some(t) if t >= start && t < end)
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut s = values.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = s.len() / 2;
    Some(if s.len() % 2 == 1 { s[mid] } else { (s[mid - 1] + s[mid]) / 2.0 })
}

/// Entrada de auditoria que representa uma acao revertida: bloqueada/rejeitada ou desfeita via git.
pub fn is_reverted_action(a: &AuditLog) -> bool {
    if !a.action.starts_with("tool.") {
        return false;
    }
    let outcome = a.details.get("outcome").and_then(|v| v.as_str()).unwrap_or("");
    if outcome == "rejected" || outcome == "denied" {
        return true;
    }
    let op = a.details.get("input").and_then(|i| i.get("op")).and_then(|v| v.as_str()).unwrap_or("");
    a.action == "tool.dots_git" && outcome == "ok" && REVERT_GIT_OPS.contains(&op)
}

fn finished_at(t: &Task) -> Timestamp {
    t.finished_at.unwrap_or(t.updated_at)
}

pub fn compute_performance(input: &PerformanceInput<'_>) -> Vec<DotPerformance> {
    let (start, end) = (&input.start, &input.end);
    let mut rows: BTreeMap<String, DotPerformance> = BTreeMap::new();
    let name_of = |id: &str| input.dot_names.get(id).cloned().unwrap_or_else(|| "(removido)".to_string());
    fn row<'r>(rows: &'r mut BTreeMap<String, DotPerformance>, id: &str, name: String) -> &'r mut DotPerformance {
        rows.entry(id.to_string()).or_insert_with(|| DotPerformance::empty(id, &name))
    }

    let mut durations: HashMap<String, Vec<f64>> = HashMap::new();
    let mut failed_titles: HashMap<String, HashSet<String>> = HashMap::new();
    let mut finished: Vec<&Task> = input
        .tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled) && in_period(Some(&finished_at(t)), start, end))
        .collect();
    finished.sort_by_key(|t| finished_at(t));
    for t in finished {
        let name = name_of(&t.dot_id);
        let r = row(&mut rows, &t.dot_id, name);
        r.finished += 1;
        let seen = failed_titles.entry(t.dot_id.clone()).or_default();
        let title = t.title.trim().to_lowercase();
        if t.attempts > 1 || (!title.is_empty() && seen.contains(&title)) {
            r.recurrent += 1;
        }
        if t.status == TaskStatus::Completed {
            r.completed += 1;
            let ms = (finished_at(t) - t.created_at).num_milliseconds();
            if ms >= 0 {
                durations.entry(t.dot_id.clone()).or_default().push(ms as f64);
            }
        } else {
            r.failed += 1;
            if !title.is_empty() {
                seen.insert(title);
            }
        }
    }
    for (dot_id, list) in &durations {
        let name = name_of(dot_id);
        row(&mut rows, dot_id, name).resolution_ms = median(list);
    }
    for e in input.executions {
        if !in_period(Some(&e.started_at), start, end) {
            continue;
        }
        let name = name_of(&e.dot_id);
        let r = row(&mut rows, &e.dot_id, name);
        r.executions += 1;
        r.cost_usd += e.cost_usd.unwrap_or(0.0);
    }
    let mut human: HashMap<String, HashSet<String>> = HashMap::new();
    for a in input.approvals {
        if !in_period(Some(&a.requested_at), start, end) {
            continue;
        }
        human.entry(a.dot_id.clone()).or_default().insert(a.task_id.clone());
    }
    for (dot_id, set) in &human {
        let name = name_of(dot_id);
        row(&mut rows, dot_id, name).human_tasks = set.len() as u32;
    }
    for a in input.audit {
        let Some(dot_id) = a.actor.strip_prefix("dot:") else { continue };
        if !in_period(Some(&a.created_at), start, end) || !is_reverted_action(a) {
            continue;
        }
        let name = name_of(dot_id);
        row(&mut rows, dot_id, name).reverted += 1;
    }
    let mut out: Vec<DotPerformance> = rows.into_values().collect();
    out.sort_by(|a, b| b.finished.cmp(&a.finished).then_with(|| a.dot_name.cmp(&b.dot_name)));
    out
}

/// Soma de todos os DOTs (a mediana de tempo vira a mediana das medianas, aproximacao suficiente para o painel).
pub fn total_performance(rows: &[DotPerformance]) -> DotPerformance {
    let mut t = DotPerformance::empty("*", "Todos");
    for r in rows {
        t.finished += r.finished;
        t.completed += r.completed;
        t.failed += r.failed;
        t.recurrent += r.recurrent;
        t.cost_usd += r.cost_usd;
        t.executions += r.executions;
        t.reverted += r.reverted;
        t.human_tasks += r.human_tasks;
    }
    let medians: Vec<f64> = rows.iter().filter_map(|r| r.resolution_ms).collect();
    t.resolution_ms = median(&medians);
    t
}

impl crate::service::RuntimeService {
    /// Resumo de desempenho dos ultimos 7 ou 30 dias (qualquer outro valor vira 7), calculado aqui no runtime.
    /// Usado pelo app desktop (IPC `performance.summary`) e pelo Flexi web: o calculo fica no servidor para
    /// nao trafegar todas as tarefas pelo IPC, cujo frame e limitado a `dots_ipc::MAX_FRAME_BYTES`.
    pub async fn performance_summary(&self, days: u64) -> dots_core::DotsResult<serde_json::Value> {
        let days = if days == 30 { 30 } else { 7 };
        let end = dots_core::now();
        let start = end - chrono::Duration::days(days as i64);
        let (s0, e0) = (start, end);
        let (tasks, executions, approvals, audit, dots) = self
            .storage
            .call(move |s| {
                let tasks = s.tasks().list(&dots_storage::repos::TaskFilter { dot_id: None, status: None, conversation_id: None, limit: 2000 })?;
                let executions = s.executions().list_overlapping(&s0, &e0)?;
                let approvals = s.approvals().list(false, 200)?;
                let audit = s.audit().list_recent(2000)?;
                let dots = s.dots().list(true)?;
                Ok((tasks, executions, approvals, audit, dots))
            })
            .await?;
        let dot_names: HashMap<String, String> = dots.into_iter().map(|d| (d.id, d.name)).collect();
        let rows = compute_performance(&PerformanceInput { tasks: &tasks, executions: &executions, approvals: &approvals, audit: &audit, dot_names: &dot_names, start, end });
        let total = total_performance(&rows);
        Ok(serde_json::json!({ "start": start, "end": end, "days": days, "total": total, "dots": rows }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};
    use dots_core::{ApprovalKind, ApprovalStatus, EngineKind, ExecutionStatus, RiskLevel, TaskKind};
    use serde_json::json;

    fn ts(h: i64) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 10, 20, 0, 0, 0).unwrap() + Duration::hours(h)
    }

    fn task(id: &str, dot: &str, title: &str, status: TaskStatus, attempts: u32, created_h: i64, finished_h: i64) -> Task {
        Task {
            id: id.into(),
            dot_id: dot.into(),
            conversation_id: None,
            kind: TaskKind::Task,
            title: title.into(),
            prompt: String::new(),
            status,
            priority: 0,
            source: "user".into(),
            correlation_id: None,
            engine: None,
            retry_safe: false,
            cancel_requested: false,
            result_text: None,
            error: None,
            attempts,
            max_attempts: 3,
            origin: "user".into(),
            parent_task_id: None,
            delegation_chain: vec![],
            created_at: ts(created_h),
            updated_at: ts(finished_h),
            started_at: Some(ts(created_h)),
            finished_at: Some(ts(finished_h)),
        }
    }

    fn execution(dot: &str, cost: Option<f64>, started_h: i64) -> Execution {
        Execution {
            id: format!("e-{started_h}"),
            task_id: "t".into(),
            dot_id: dot.into(),
            engine: EngineKind::Claude,
            attempt: 1,
            status: ExecutionStatus::Completed,
            pid: None,
            exit_code: None,
            engine_session_id: None,
            workspace_path: None,
            error: None,
            events_count: 0,
            cost_usd: cost,
            usage: json!({}),
            started_at: ts(started_h),
            finished_at: None,
        }
    }

    fn approval(dot: &str, task: &str, requested_h: i64) -> Approval {
        Approval {
            id: format!("a-{task}-{requested_h}"),
            kind: ApprovalKind::Permission,
            options: vec![],
            execution_id: "e".into(),
            task_id: task.into(),
            conversation_id: None,
            dot_id: dot.into(),
            dot_name: "x".into(),
            tool: "t".into(),
            input: json!({}),
            risk: RiskLevel::Write,
            resources: vec![],
            summary: String::new(),
            status: ApprovalStatus::Pending,
            requested_at: ts(requested_h),
            expires_at: ts(requested_h + 1),
            resolved_at: None,
            reason: None,
        }
    }

    fn audit(actor: &str, action: &str, details: serde_json::Value, h: i64) -> AuditLog {
        AuditLog { id: format!("l-{h}"), actor: actor.into(), action: action.into(), entity_type: "tool".into(), entity_id: None, details, created_at: ts(h) }
    }

    #[test]
    fn reincidencia_mediana_e_ordenacao() {
        let tasks = vec![
            task("1", "a", "Publicar", TaskStatus::Failed, 1, 0, 1),
            task("2", "a", " publicar ", TaskStatus::Completed, 1, 2, 4), // titulo ja falhou -> reincidente, 2h
            task("3", "a", "Outra", TaskStatus::Completed, 2, 5, 11),      // attempts>1 -> reincidente, 6h
            task("4", "a", "Mais uma", TaskStatus::Completed, 1, 12, 16),  // 4h
            task("5", "b", "Fora", TaskStatus::Completed, 1, 0, 100),      // fora do periodo
            task("6", "b", "Cancelada", TaskStatus::Cancelled, 1, 0, 3),
            task("7", "c", "Rodando", TaskStatus::Running, 1, 0, 3),       // nao encerrada
        ];
        let mut names = HashMap::new();
        names.insert("a".to_string(), "Alfa".to_string());
        let rows = compute_performance(&PerformanceInput { tasks: &tasks, executions: &[], approvals: &[], audit: &[], dot_names: &names, start: ts(0), end: ts(48) });
        assert_eq!(rows.len(), 2);
        let a = &rows[0];
        assert_eq!((a.dot_name.as_str(), a.finished, a.completed, a.failed, a.recurrent), ("Alfa", 4, 3, 1, 2));
        assert_eq!(a.resolution_ms, Some(4.0 * 3_600_000.0));
        let b = &rows[1];
        assert_eq!((b.dot_name.as_str(), b.finished, b.failed, b.completed), ("(removido)", 1, 1, 0));
        assert_eq!(b.resolution_ms, None);
        let total = total_performance(&rows);
        assert_eq!((total.finished, total.recurrent, total.resolution_ms), (5, 2, Some(4.0 * 3_600_000.0)));
    }

    #[test]
    fn custo_intervencao_humana_e_revertidas() {
        let executions = vec![execution("a", Some(0.5), 1), execution("a", None, 2), execution("a", Some(1.0), 90)];
        let approvals = vec![approval("a", "t1", 1), approval("a", "t1", 2), approval("a", "t2", 3), approval("a", "t3", 90)];
        let audit = vec![
            audit("dot:a", "tool.fs_write", json!({ "outcome": "denied" }), 1),
            audit("dot:a", "tool.shell", json!({ "outcome": "rejected" }), 2),
            audit("dot:a", "tool.dots_git", json!({ "outcome": "ok", "input": { "op": "reset" } }), 3),
            audit("dot:a", "tool.dots_git", json!({ "outcome": "ok", "input": { "op": "commit" } }), 4),
            audit("dot:a", "tool.shell", json!({ "outcome": "ok" }), 5),
            audit("user", "approval.reject", json!({ "outcome": "rejected" }), 6),
            audit("dot:a", "tool.shell", json!({ "outcome": "denied" }), 90),
        ];
        let names = HashMap::new();
        let rows = compute_performance(&PerformanceInput { tasks: &[], executions: &executions, approvals: &approvals, audit: &audit, dot_names: &names, start: ts(0), end: ts(48) });
        assert_eq!(rows.len(), 1);
        let a = &rows[0];
        assert_eq!((a.executions, a.cost_usd, a.human_tasks, a.reverted), (2, 0.5, 2, 3));
    }

    #[test]
    fn serializa_em_camel_case() {
        let v = serde_json::to_value(DotPerformance::empty("x", "X")).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["completed", "costUsd", "dotId", "dotName", "executions", "failed", "finished", "humanTasks", "recurrent", "resolutionMs", "reverted"]);
        assert!(v["resolutionMs"].is_null());
    }
}
