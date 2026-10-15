//! AGY / Antigravity CLI (`agy --print=... --output-format stream-json`). Formato verificado em 1.3.5.

use dots_core::{DotsResult, EngineKind, ExecutionEvent, FailureKind};
use serde_json::Value;

use super::{AgentEngine, ParserState, add_dirs, base_env, classify_failure};
use crate::request::{ExecutionRequest, PreparedCommand};

pub struct AgyAdapter;

impl AgentEngine for AgyAdapter {
    fn kind(&self) -> EngineKind {
        EngineKind::Agy
    }

    fn prepare(&self, req: &ExecutionRequest) -> DotsResult<PreparedCommand> {
        // O prompt precisa ir colado ao flag; caso contrario o proximo argumento vira o prompt.
        let mut args: Vec<String> = vec![
            format!("--print={}", req.prompt_with_instructions()),
            "--output-format".into(),
            "stream-json".into(),
            "--print-timeout".into(),
            "0s".into(),
            "--mode".into(),
            if req.native_write { "accept-edits".into() } else { "plan".into() },
            "--disable-slash-commands".into(),
        ];
        if !req.native_write {
            args.push("--sandbox".into());
        }
        if let Some(m) = &req.model {
            args.push("--model".into());
            args.push(m.clone());
        }
        add_dirs(&mut args, "--add-dir", &req.extra_dirs);
        if let Some(s) = &req.resume_session {
            args.push("--conversation".into());
            args.push(s.clone());
        }
        Ok(PreparedCommand { program: req.executable.clone(), args, env: base_env(req), cwd: req.workspace.clone(), stdin: None })
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
        match v.get("event").and_then(|e| e.as_str()).unwrap_or("") {
            "init" => {
                st.session_id = v.get("conversation_id").and_then(|x| x.as_str()).map(String::from);
                vec![ExecutionEvent::Started { engine_session_id: st.session_id.clone() }]
            }
            "step_update" => {
                let s = v.get("step_update").cloned().unwrap_or(Value::Null);
                let step_type = s.get("step_type").and_then(|x| x.as_str()).unwrap_or("");
                let state = s.get("state").and_then(|x| x.as_str()).unwrap_or("");
                let call_id = format!("step_{}", s.get("step_index").and_then(|x| x.as_u64()).unwrap_or(0));
                let mut out = vec![];
                if let Some(u) = s.get("usage") {
                    st.usage = u.clone();
                    out.push(ExecutionEvent::Usage { input_tokens: u.get("input_tokens").and_then(|x| x.as_u64()), output_tokens: u.get("output_tokens").and_then(|x| x.as_u64()), cost_usd: None });
                }
                if step_type == "tool" {
                    let info = s.get("tool_info").cloned().unwrap_or(Value::Null);
                    let mut name = s.get("tool_name").and_then(|x| x.as_str()).unwrap_or("tool").to_string();
                    if name == "call_mcp_tool" {
                        let p = info.get("parameters");
                        name = format!("{}/{}", p.and_then(|p| p.get("ServerName")).and_then(|x| x.as_str()).unwrap_or("mcp"), p.and_then(|p| p.get("ToolName")).and_then(|x| x.as_str()).unwrap_or("tool"));
                    }
                    match state {
                        "ACTIVE" => out.push(ExecutionEvent::ToolCall { call_id, name, input: info.get("parameters").cloned().unwrap_or(Value::Null) }),
                        "DONE" => out.push(ExecutionEvent::ToolResult { call_id, name, ok: true, output: info.get("output").and_then(|x| x.as_str()).unwrap_or("").into() }),
                        "ERROR" => out.push(ExecutionEvent::ToolResult { call_id, name, ok: false, output: info.get("error").and_then(|e| e.get("message")).and_then(|x| x.as_str()).unwrap_or("erro").into() }),
                        _ => {}
                    }
                }
                out
            }
            "result" => {
                let r = v.get("result").cloned().unwrap_or(Value::Null);
                let status = r.get("status").and_then(|x| x.as_str()).unwrap_or("");
                let response = r.get("response").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
                let mut out = vec![];
                if let Some(u) = r.get("usage") {
                    st.usage = u.clone();
                }
                if let Some(denied) = r.get("denied_actions").and_then(|d| d.as_array()) {
                    if !denied.is_empty() {
                        let names: Vec<String> = denied.iter().filter_map(|d| d.get("display_name").or(d.get("action")).and_then(|x| x.as_str()).map(String::from)).collect();
                        out.push(ExecutionEvent::Notice { level: "warn".into(), text: format!("acoes negadas pelo AGY (sem permissao headless): {}", names.join(", ")) });
                    }
                }
                if status == "SUCCESS" {
                    st.completed = true;
                    if !response.is_empty() {
                        st.last_text = response.clone();
                        out.push(ExecutionEvent::Text { text: response.clone() });
                    }
                    out.push(ExecutionEvent::Completed { result_text: st.last_text.clone() });
                } else {
                    st.failed = true;
                    let msg = if response.is_empty() { format!("AGY terminou com status {status}") } else { response };
                    out.push(ExecutionEvent::Failed { kind: classify_failure(&msg), message: msg });
                }
                out
            }
            _ => vec![ExecutionEvent::Raw { line: line.to_string() }],
        }
    }

    fn finish(&self, exit_code: Option<i32>, timed_out: bool, cancelled: bool, st: &mut ParserState) -> Vec<ExecutionEvent> {
        if !st.completed && !st.failed && !timed_out && !cancelled && exit_code == Some(0) {
            st.failed = true;
            return vec![ExecutionEvent::Failed { kind: FailureKind::Engine, message: "AGY encerrou sem evento de resultado".into() }];
        }
        super::default_finish(exit_code, timed_out, cancelled, st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::test_support::{request, run_fixture};

    #[test]
    fn prepare_builds_verified_flags() {
        let req = request(EngineKind::Agy);
        let cmd = AgyAdapter.prepare(&req).unwrap();
        assert!(cmd.args[0].starts_with("--print=<dot_instructions>"));
        assert!(cmd.args.contains(&"stream-json".to_string()));
        assert!(cmd.args.contains(&"plan".to_string()));
        assert!(cmd.args.contains(&"--sandbox".to_string()));
        assert!(!cmd.args.contains(&"--model".to_string()));
        assert!(cmd.stdin.is_none());
        let mut req = req;
        req.model = Some("gemini-3-pro".into());
        let a2 = AgyAdapter.prepare(&req).unwrap().args;
        let mi = a2.iter().position(|x| x == "--model").unwrap();
        assert_eq!(a2[mi + 1], "gemini-3-pro");
    }

    #[test]
    fn parses_real_fixture_with_denied_tool() {
        let (evs, st) = run_fixture(EngineKind::Agy, "agy-print-ok.jsonl");
        assert!(matches!(evs[0], ExecutionEvent::Started { engine_session_id: Some(ref s) } if s == "858a87a1-1d11-4878-9837-6a8a76134ad1"));
        assert!(evs.iter().any(|e| matches!(e, ExecutionEvent::ToolCall { name, .. } if name == "run_command")));
        assert!(evs.iter().any(|e| matches!(e, ExecutionEvent::ToolResult { ok: false, .. })));
        assert!(evs.iter().any(|e| matches!(e, ExecutionEvent::Notice { text, .. } if text.contains("negadas"))));
        assert!(matches!(evs.last(), Some(ExecutionEvent::Completed { .. })));
        assert!(st.completed);
    }

    #[test]
    fn parses_mcp_call() {
        let a = AgyAdapter;
        let mut st = ParserState::default();
        let l = r#"{"event":"step_update","step_update":{"conversation_id":"c","step_index":5,"state":"DONE","step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"name":"call_mcp_tool","parameters":{"Arguments":{"text":"oi"},"ServerName":"dots","ToolName":"dots_echo"},"output":"ECHO: oi"}}}"#;
        assert!(matches!(a.parse_line(l, &mut st)[0], ExecutionEvent::ToolResult { ref name, ok: true, .. } if name == "dots/dots_echo"));
    }
}
