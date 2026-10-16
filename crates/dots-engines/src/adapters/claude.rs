//! Claude Code (`claude -p --output-format stream-json`). Formato verificado em 2.1.300.

use dots_core::{DotsResult, EngineKind, ExecutionEvent, FailureKind};
use serde_json::Value;

use super::{AgentEngine, ParserState, add_dirs, base_env, classify_failure};
use crate::request::{ExecutionRequest, PreparedCommand};

pub struct ClaudeAdapter;

impl AgentEngine for ClaudeAdapter {
    fn kind(&self) -> EngineKind {
        EngineKind::Claude
    }

    fn prepare(&self, req: &ExecutionRequest) -> DotsResult<PreparedCommand> {
        let mut args: Vec<String> = vec![
            "-p".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--include-partial-messages".into(),
            "--no-chrome".into(),
            "--settings".into(),
            r#"{"disableAllHooks":true}"#.into(),
            "--permission-mode".into(),
            if req.native_write { "acceptEdits".into() } else { "default".into() },
            "--tools".into(),
            if req.native_write { "default".into() } else { "".into() },
        ];
        if let Some(m) = &req.model {
            args.push("--model".into());
            args.push(m.clone());
        }
        add_dirs(&mut args, "--add-dir", &req.extra_dirs);
        if let Some(g) = &req.gateway {
            let env: serde_json::Map<String, Value> = g.env.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
            let cfg = serde_json::json!({ "mcpServers": { g.server_name.clone(): { "command": g.command.to_string_lossy(), "args": g.args, "env": env } } });
            args.push("--strict-mcp-config".into());
            args.push("--mcp-config".into());
            args.push(cfg.to_string());
            args.push("--permission-prompt-tool".into());
            args.push(format!("mcp__{}__permission_prompt", g.server_name));
        } else {
            args.push("--strict-mcp-config".into());
            args.push("--permission-prompts".into());
            args.push("none".into());
        }
        if !req.system_instructions.trim().is_empty() {
            args.push("--append-system-prompt".into());
            args.push(req.system_instructions.clone());
        }
        match (&req.resume_session, &req.new_session_id) {
            (Some(s), _) => {
                args.push("--resume".into());
                args.push(s.clone());
            }
            (None, Some(id)) => {
                args.push("--session-id".into());
                args.push(id.clone());
            }
            _ => {}
        }
        if let Some(t) = req.max_turns {
            args.push("--max-turns".into());
            args.push(t.to_string());
        }
        if let Some(b) = req.max_budget_usd {
            args.push("--max-budget-usd".into());
            args.push(format!("{b:.2}"));
        }
        Ok(PreparedCommand {
            program: req.executable.clone(),
            args,
            env: base_env(req),
            cwd: req.workspace.clone(),
            stdin: Some(req.prompt.clone()),
        })
    }

    fn parse_line(&self, line: &str, st: &mut ParserState) -> Vec<ExecutionEvent> {
        let line = line.trim();
        if line.is_empty() {
            return vec![];
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return vec![ExecutionEvent::Raw { line: line.to_string() }],
        };
        let t = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match t {
            "system" => {
                let sub = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
                match sub {
                    "init" => {
                        st.session_id = v.get("session_id").and_then(|x| x.as_str()).map(String::from);
                        vec![ExecutionEvent::Started { engine_session_id: st.session_id.clone() }]
                    }
                    "permission_denied" => vec![ExecutionEvent::Notice { level: "warn".into(), text: format!("permissao negada: {}", v.get("tool_name").and_then(|x| x.as_str()).unwrap_or("?")) }],
                    "api_retry" => vec![ExecutionEvent::Notice { level: "warn".into(), text: format!("nova tentativa de API ({})", v.get("error").and_then(|x| x.as_str()).unwrap_or("?")) }],
                    "model_refusal_fallback" => vec![ExecutionEvent::Notice { level: "warn".into(), text: format!("fallback de modelo para {}", v.get("fallback_model").and_then(|x| x.as_str()).unwrap_or("?")) }],
                    "informational" => vec![ExecutionEvent::Notice { level: v.get("level").and_then(|x| x.as_str()).unwrap_or("info").into(), text: v.get("content").and_then(|x| x.as_str()).unwrap_or("").into() }],
                    _ => vec![],
                }
            }
            "stream_event" => {
                let delta = v.get("event").and_then(|e| e.get("delta"));
                match delta.and_then(|d| d.get("type")).and_then(|x| x.as_str()) {
                    Some("text_delta") => vec![ExecutionEvent::TextDelta { text: delta.and_then(|d| d.get("text")).and_then(|x| x.as_str()).unwrap_or("").into() }],
                    _ => vec![],
                }
            }
            "assistant" => {
                let mut out = vec![];
                // `message.usage` e da chamada individual: entrada + cache = tamanho do contexto neste ponto.
                // (O `usage` do evento `result` e a soma de todas as chamadas, nao serve para isso.)
                if let Some(u) = v.get("message").and_then(|m| m.get("usage")) {
                    let n = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
                    let total = n("input_tokens") + n("cache_creation_input_tokens") + n("cache_read_input_tokens");
                    if total > 0 {
                        st.context_tokens = Some(total);
                    }
                }
                if let Some(content) = v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                    for c in content {
                        match c.get("type").and_then(|x| x.as_str()) {
                            Some("text") => {
                                let text = c.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
                                st.last_text = text.clone();
                                out.push(ExecutionEvent::Text { text });
                            }
                            Some("tool_use") => out.push(ExecutionEvent::ToolCall {
                                call_id: c.get("id").and_then(|x| x.as_str()).unwrap_or("").into(),
                                name: c.get("name").and_then(|x| x.as_str()).unwrap_or("").into(),
                                input: c.get("input").cloned().unwrap_or(Value::Null),
                            }),
                            Some("thinking") => {
                                let text = c.get("thinking").and_then(|x| x.as_str()).unwrap_or("").to_string();
                                if !text.is_empty() {
                                    out.push(ExecutionEvent::Reasoning { text });
                                }
                            }
                            _ => {}
                        }
                    }
                }
                out
            }
            "user" => {
                let mut out = vec![];
                if let Some(content) = v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
                    for c in content {
                        if c.get("type").and_then(|x| x.as_str()) == Some("tool_result") {
                            let text = match c.get("content") {
                                Some(Value::String(s)) => s.clone(),
                                Some(Value::Array(arr)) => arr.iter().filter_map(|x| x.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("\n"),
                                _ => String::new(),
                            };
                            out.push(ExecutionEvent::ToolResult {
                                call_id: c.get("tool_use_id").and_then(|x| x.as_str()).unwrap_or("").into(),
                                name: String::new(),
                                ok: !c.get("is_error").and_then(|x| x.as_bool()).unwrap_or(false),
                                output: text,
                            });
                        }
                    }
                }
                out
            }
            "result" => {
                let usage = v.get("usage").cloned().unwrap_or(Value::Null);
                st.usage = usage.clone();
                st.cost_usd = v.get("total_cost_usd").and_then(|x| x.as_f64());
                let mut out = vec![ExecutionEvent::Usage {
                    input_tokens: usage.get("input_tokens").and_then(|x| x.as_u64()),
                    output_tokens: usage.get("output_tokens").and_then(|x| x.as_u64()),
                    cost_usd: st.cost_usd,
                    context_tokens: st.context_tokens,
                }];
                if let Some(denials) = v.get("permission_denials").and_then(|d| d.as_array()) {
                    if !denials.is_empty() {
                        out.push(ExecutionEvent::Notice { level: "warn".into(), text: format!("{} permissao(oes) negada(s)", denials.len()) });
                    }
                }
                let is_error = v.get("is_error").and_then(|x| x.as_bool()).unwrap_or(false);
                let sub = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
                let text = v.get("result").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if is_error || sub != "success" {
                    st.failed = true;
                    let kind = if sub.contains("budget") { FailureKind::Budget } else if sub.contains("max_turns") { FailureKind::Engine } else { classify_failure(&text) };
                    out.push(ExecutionEvent::Failed { kind, message: if text.is_empty() { format!("Claude terminou com {sub}") } else { text } });
                } else {
                    st.completed = true;
                    if !text.is_empty() {
                        st.last_text = text.clone();
                    }
                    out.push(ExecutionEvent::Completed { result_text: st.last_text.clone() });
                }
                out
            }
            "rate_limit_event" => vec![],
            _ => vec![ExecutionEvent::Raw { line: line.to_string() }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::test_support::{request, run_fixture};

    #[test]
    fn prepare_builds_verified_flags() {
        let mut req = request(EngineKind::Claude);
        let cmd = ClaudeAdapter.prepare(&req).unwrap();
        let a = &cmd.args;
        assert_eq!(&a[0..3], &["-p", "--output-format", "stream-json"]);
        assert!(a.contains(&"--tools".to_string()));
        let ti = a.iter().position(|x| x == "--tools").unwrap();
        assert_eq!(a[ti + 1], "");
        assert!(a.contains(&"--strict-mcp-config".to_string()));
        let mi = a.iter().position(|x| x == "--mcp-config").unwrap();
        let cfg: Value = serde_json::from_str(&a[mi + 1]).unwrap();
        assert_eq!(cfg["mcpServers"]["dots"]["command"], "/opt/dotsd");
        assert_eq!(cfg["mcpServers"]["dots"]["env"]["DOTS_EXECUTION_TOKEN"], "tok");
        assert!(a.contains(&"mcp__dots__permission_prompt".to_string()));
        assert!(a.contains(&"--session-id".to_string()));
        assert!(a.contains(&"--append-system-prompt".to_string()));
        assert!(a.contains(&"--max-budget-usd".to_string()));
        assert!(!a.contains(&"--bare".to_string()));
        assert!(!a.contains(&"--model".to_string()), "sem modelo definido o CLI usa o proprio padrao");
        assert_eq!(cmd.stdin.as_deref(), Some("Faca X"));
        req.resume_session = Some("sess-1".into());
        req.native_write = true;
        req.model = Some("claude-opus-5-5".into());
        let a2 = ClaudeAdapter.prepare(&req).unwrap().args;
        let mi = a2.iter().position(|x| x == "--model").unwrap();
        assert_eq!(a2[mi + 1], "claude-opus-5-5");
        assert!(a2.contains(&"--resume".to_string()) && !a2.contains(&"--session-id".to_string()));
        let ti = a2.iter().position(|x| x == "--tools").unwrap();
        assert_eq!(a2[ti + 1], "default");
        assert!(a2.contains(&"acceptEdits".to_string()));
    }

    #[test]
    fn parses_real_fixture_with_safety_notices() {
        let (evs, st) = run_fixture(EngineKind::Claude, "claude-print-ok.jsonl");
        assert!(matches!(evs[0], ExecutionEvent::Started { engine_session_id: Some(ref s) } if s == "a6f1a7f1-30e2-4825-bb7f-7e35cc804e6b"));
        assert!(evs.iter().any(|e| matches!(e, ExecutionEvent::Notice { .. })), "system/* viram avisos");
        assert!(evs.iter().any(|e| matches!(e, ExecutionEvent::Text { text } if text == "OK")));
        assert!(matches!(evs.last(), Some(ExecutionEvent::Completed { result_text }) if result_text == "OK"));
        assert!(st.completed && !st.failed);
        assert!(st.cost_usd.unwrap() > 0.0);
        // Unica mensagem `assistant` da fixture: input 2 + cache_creation 6579 + cache_read 0 (o `result` soma 12443+649+4 e nao e usado).
        assert_eq!(st.context_tokens, Some(2 + 6579), "contexto medido pela ultima chamada, nao pela soma do result");
        assert!(evs.iter().any(|e| matches!(e, ExecutionEvent::Usage { context_tokens: Some(6581), input_tokens: Some(4), .. })));
    }

    #[test]
    fn parses_tool_use_and_error_result() {
        let a = ClaudeAdapter;
        let mut st = ParserState::default();
        let tu = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"mcp__dots__dots_echo","input":{"text":"oi"}}]}}"#;
        let tr = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":[{"type":"text","text":"ECHO: oi"}]}]}}"#;
        let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"ol"}}}"#;
        let err = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"Not logged in. Please run /login","usage":{}}"#;
        assert!(matches!(a.parse_line(tu, &mut st)[0], ExecutionEvent::ToolCall { ref name, .. } if name == "mcp__dots__dots_echo"));
        assert!(matches!(a.parse_line(tr, &mut st)[0], ExecutionEvent::ToolResult { ok: true, ref output, .. } if output == "ECHO: oi"));
        assert!(matches!(a.parse_line(delta, &mut st)[0], ExecutionEvent::TextDelta { ref text } if text == "ol"));
        let evs = a.parse_line(err, &mut st);
        assert!(evs.iter().any(|e| matches!(e, ExecutionEvent::Failed { kind: FailureKind::Auth, .. })));
        assert!(st.failed);
    }
}
