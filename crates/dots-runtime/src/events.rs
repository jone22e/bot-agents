//! Dispatcher de eventos locais: grava o evento (com dedupe) e cria tarefas para os gatilhos.

use dots_core::automation::event_types as et;
use dots_core::{DotsResult, EventRecord, EventSubscription, TaskCreate, TaskKind};
use serde_json::{Value, json};

use crate::service::RuntimeService;

pub const MAX_EVENT_DEPTH: u32 = 4;

/// Glob simples: `*` = qualquer trecho sem `/`, `**` = qualquer trecho, `?` = um caractere.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    fn rec(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') if p.get(1) == Some(&'*') => {
                let rest = if p.get(2) == Some(&'/') { &p[3..] } else { &p[2..] };
                (0..=s.len()).any(|i| rec(rest, &s[i..]))
            }
            Some('*') => (0..=s.len()).take_while(|&i| i == 0 || s[i - 1] != '/').any(|i| rec(&p[1..], &s[i..])),
            Some('?') => !s.is_empty() && s[0] != '/' && rec(&p[1..], &s[1..]),
            Some(c) => s.first() == Some(c) && rec(&p[1..], &s[1..]),
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    if rec(&p, &s) {
        return true;
    }
    // Padrao sem "/" casa contra o nome do arquivo.
    if !pattern.contains('/') {
        if let Some(name) = path.rsplit('/').next() {
            let n: Vec<char> = name.chars().collect();
            return rec(&p, &n);
        }
    }
    false
}

fn matches(sub: &EventSubscription, ev: &EventRecord) -> bool {
    if let Some(g) = sub.filter.path_glob.as_deref().filter(|g| !g.trim().is_empty()) {
        let path = ev.payload.get("path").and_then(|p| p.as_str()).unwrap_or("");
        if !glob_match(g.trim(), path) {
            return false;
        }
    }
    if let Some(src) = sub.filter.source_dot_id.as_deref().filter(|s| !s.is_empty()) {
        let dot = ev.payload.get("dot_id").and_then(|d| d.as_str()).unwrap_or("");
        if dot != src {
            return false;
        }
    }
    // Um DOT nao reage ao proprio trabalho (evita realimentacao).
    if let Some(dot) = ev.payload.get("dot_id").and_then(|d| d.as_str()) {
        if dot == sub.dot_id && matches!(ev.event_type.as_str(), et::TASK_COMPLETED | et::TASK_FAILED | et::TOOL_FINISHED) {
            return false;
        }
    }
    true
}

pub fn render_template(template: &str, ev: &EventRecord) -> String {
    let payload_str = serde_json::to_string_pretty(&ev.payload).unwrap_or_default();
    let get = |k: &str| ev.payload.get(k).map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).unwrap_or_default();
    let mut out = template
        .replace("{{path}}", &get("path"))
        .replace("{{type}}", &ev.event_type)
        .replace("{{source}}", &ev.source)
        .replace("{{sha}}", &get("sha"))
        .replace("{{message}}", &get("message"))
        .replace("{{title}}", &get("title"))
        .replace("{{result}}", &get("result"))
        .replace("{{payload}}", &payload_str);
    if !template.contains("{{") {
        out.push_str(&format!("\n\n<event type=\"{}\">\n{}\n</event>", ev.event_type, payload_str));
    }
    out
}

impl RuntimeService {
    /// Grava e despacha um evento. Duplicados (mesma chave) sao ignorados e retornam `None`.
    pub async fn emit_event(&self, event_type: &str, source: &str, payload: Value, correlation_id: Option<String>, idempotency_key: Option<String>, depth: u32) -> DotsResult<Option<EventRecord>> {
        let (t, s, c, k) = (event_type.to_string(), source.to_string(), correlation_id.clone(), idempotency_key.clone());
        let p = payload.clone();
        let Some(ev) = self.storage.call(move |st| st.events().insert(&t, &s, &p, c.as_deref(), k.as_deref(), depth)).await? else {
            tracing::debug!(event_type, key = ?idempotency_key, "evento duplicado ignorado");
            return Ok(None);
        };
        self.events.emit("event.recorded", json!({ "id": ev.id, "type": ev.event_type, "source": ev.source }));
        if et::SUBSCRIBABLE.contains(&event_type) {
            if depth >= MAX_EVENT_DEPTH {
                tracing::warn!(event_type, depth, "profundidade maxima de encadeamento; gatilhos ignorados");
            } else if let Err(e) = self.dispatch_event(&ev).await {
                tracing::warn!(error = %e, "falha ao despachar gatilhos");
            }
        }
        Ok(Some(ev))
    }

    async fn dispatch_event(&self, ev: &EventRecord) -> DotsResult<()> {
        let t = ev.event_type.clone();
        let subs = self.storage.call(move |st| st.subscriptions().enabled_for_type(&t)).await?;
        for sub in subs.into_iter().filter(|s| matches(s, ev)) {
            if let Some(last) = sub.last_triggered_at {
                let elapsed = (dots_core::now() - last).num_seconds();
                if elapsed < sub.cooldown_seconds as i64 {
                    let (e, s) = (ev.id.clone(), sub.id.clone());
                    let _ = self.storage.call(move |st| st.events().record_delivery(&e, &s, None, "cooldown")).await;
                    tracing::info!(subscription = %sub.id, elapsed, "gatilho em cooldown; evento nao gera tarefa");
                    continue;
                }
            }
            // Reserva a entrega antes de criar a tarefa: duplicados concorrentes nao passam.
            let (e, s) = (ev.id.clone(), sub.id.clone());
            let fresh = self.storage.call(move |st| st.events().record_delivery(&e, &s, None, "created")).await?;
            if !fresh {
                continue;
            }
            let prompt = render_template(&sub.prompt_template, ev);
            let title = format!("Gatilho: {}", match ev.event_type.as_str() {
                et::FILE_CHANGED => format!("{} mudou", ev.payload.get("path").and_then(|p| p.as_str()).and_then(|p| p.rsplit('/').next()).unwrap_or("arquivo")),
                et::GIT_COMMIT => "novo commit".to_string(),
                et::TASK_COMPLETED => format!("{} concluiu", ev.payload.get("dot_name").and_then(|p| p.as_str()).unwrap_or("um DOT")),
                et::TASK_FAILED => format!("{} falhou", ev.payload.get("dot_name").and_then(|p| p.as_str()).unwrap_or("um DOT")),
                other => other.to_string(),
            });
            let input = TaskCreate {
                dot_id: sub.dot_id.clone(),
                prompt,
                title: Some(title),
                kind: Some(TaskKind::Task),
                retry_safe: Some(sub.retry_safe),
                origin: Some(format!("event:{}", ev.id)),
                correlation_id: ev.correlation_id.clone().or_else(|| Some(ev.id.clone())),
                ..Default::default()
            };
            match self.create_task_with_depth(input, ev.depth + 1).await {
                Ok(task) => {
                    let (e, s, tid) = (ev.id.clone(), sub.id.clone(), task.id.clone());
                    let _ = self.storage.call(move |st| {
                        st.subscriptions().touch(&s)?;
                        st.events().set_delivery_task(&e, &s, &tid)
                    }).await;
                    tracing::info!(subscription = %sub.id, task = %task.id, event = %ev.id, "gatilho criou tarefa");
                }
                Err(e) => tracing::warn!(subscription = %sub.id, error = %e, "gatilho nao criou tarefa"),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob() {
        assert!(glob_match("*.md", "/a/b/c.md"));
        assert!(!glob_match("*.md", "/a/b/c.txt"));
        assert!(glob_match("docs/**", "docs/x/y.md"));
        assert!(glob_match("/w/docs/*.md", "/w/docs/a.md"));
        assert!(!glob_match("/w/docs/*.md", "/w/docs/sub/a.md"));
        assert!(glob_match("**/*.ts", "/x/y/z.ts"));
        assert!(glob_match("a?c", "abc"));
    }

    #[test]
    fn template_render() {
        let ev = EventRecord { id: "e".into(), event_type: "file.changed".into(), source: "w".into(), occurred_at: dots_core::now(), payload: json!({"path": "/x/a.md"}), correlation_id: None, idempotency_key: None, depth: 0 };
        assert_eq!(render_template("Revise {{path}}", &ev), "Revise /x/a.md");
        assert!(render_template("Revise", &ev).contains("<event"));
    }
}
