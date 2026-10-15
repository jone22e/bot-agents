//! Central de operacoes: numeros agregados e amostragem dos processos de motor.

use std::path::Path;

use dots_core::{DotsResult, EngineUsage, ProcessSample, RuntimeStats, TaskStatus};
use dots_storage::repos::TaskFilter;

use crate::service::RuntimeService;

fn sample(pid: u32) -> Option<(f64, u64)> {
    let out = std::process::Command::new("/bin/ps").args(["-o", "%cpu=,rss=", "-p", &pid.to_string()]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let mut it = s.split_whitespace();
    let cpu: f64 = it.next()?.parse().ok()?;
    let rss_kb: u64 = it.next()?.parse().ok()?;
    Some((cpu, rss_kb * 1024))
}

impl RuntimeService {
    pub async fn stats(&self) -> DotsResult<RuntimeStats> {
        let since = dots_core::now() - chrono::Duration::hours(24);
        let paused = self.paused.load(std::sync::atomic::Ordering::SeqCst);
        let (dots_active, task_counts, running, approvals, questions, failures, events, usage, last_error) = self
            .storage
            .call(move |st| {
                let dots_active = st.dots().count_by_status()?.into_iter().find(|(s, _)| *s == dots_core::DotStatus::Active).map(|(_, n)| n).unwrap_or(0);
                let task_counts = st.tasks().count_by_status()?;
                let running = st.executions().running()?;
                let approvals = st.approvals().pending_count()?;
                let questions = st.approvals().pending_questions_count()?;
                let failed = st.tasks().list(&TaskFilter { status: Some(vec![TaskStatus::Failed]), limit: 500, ..Default::default() })?;
                let failures = failed.iter().filter(|t| t.finished_at.map(|f| f > since).unwrap_or(false)).count() as u32;
                let last_error = failed.iter().filter(|t| t.finished_at.map(|f| f > since).unwrap_or(false)).max_by_key(|t| t.finished_at).and_then(|t| t.error.clone().map(|e| format!("{}: {e}", t.title)));
                let events = st.events().count_since(&since)?;
                let recent = st.executions().list_recent(500)?;
                let mut by: std::collections::BTreeMap<String, EngineUsage> = std::collections::BTreeMap::new();
                for e in recent.iter().filter(|e| e.started_at > since) {
                    let u = by.entry(e.engine.as_str().to_string()).or_insert(EngineUsage { engine: e.engine.as_str().into(), executions: 0, input_tokens: 0, output_tokens: 0, cost_usd: 0.0, total_seconds: 0 });
                    u.executions += 1;
                    u.input_tokens += e.usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    u.output_tokens += e.usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    u.cost_usd += e.cost_usd.unwrap_or(0.0);
                    let end = e.finished_at.unwrap_or_else(dots_core::now);
                    u.total_seconds += (end - e.started_at).num_seconds().max(0) as u64;
                }
                Ok((dots_active, task_counts, running, approvals, questions, failures, events, by.into_values().collect::<Vec<_>>(), last_error))
            })
            .await?;
        let get = |st: TaskStatus| task_counts.iter().find(|(s, _)| *s == st).map(|(_, n)| *n).unwrap_or(0);
        let processes = running
            .iter()
            .filter_map(|e| {
                let pid = e.pid?;
                let (cpu, rss) = sample(pid)?;
                Some(ProcessSample { execution_id: e.id.clone(), dot_id: e.dot_id.clone(), pid, cpu_percent: cpu, rss_bytes: rss, elapsed_seconds: (dots_core::now() - e.started_at).num_seconds().max(0) as u64 })
            })
            .collect();
        Ok(RuntimeStats {
            dots_active,
            running: get(TaskStatus::Running),
            queued: get(TaskStatus::Queued),
            blocked: get(TaskStatus::WaitingUser) + get(TaskStatus::WaitingApproval) + get(TaskStatus::WaitingExternal),
            approvals_pending: approvals,
            questions_pending: questions,
            failures_24h: failures,
            events_24h: events,
            paused,
            engine_usage: usage,
            processes,
            last_error,
        })
    }

    pub fn tail_log(&self, lines: usize) -> Vec<String> {
        let dir: &Path = &self.layout.logs_dir;
        let mut files: Vec<_> = std::fs::read_dir(dir).ok().into_iter().flatten().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("dotsd.log")).unwrap_or(false)).collect();
        files.sort();
        let Some(last) = files.last() else { return vec![] };
        let content = std::fs::read_to_string(last).unwrap_or_default();
        let all: Vec<&str> = content.lines().collect();
        all.iter().rev().take(lines.clamp(1, 2000)).rev().map(|s| s.to_string()).collect()
    }
}
