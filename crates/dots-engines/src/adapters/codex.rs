//! Codex CLI (`codex exec --json`). Formato verificado em codex-cli 0.162.1.

use dots_core::{DotsResult, EngineKind, ExecutionEvent, FailureKind};
use serde_json::Value;

use super::{AgentEngine, ParserState, add_dirs, base_env, classify_failure};
use crate::request::{ExecutionRequest, PreparedCommand};

pub struct CodexAdapter;

fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Servidores MCP que o Codex do usuario carrega sozinho (computer use, navegador do Codex, app tools), vistos em
/// `codex mcp list` do codex-cli 0.162.1. Eles vem de plugins e nao aparecem no config.toml.
const CODEX_BUNDLED_MCP: &[&str] = &["cua_repl", "node_repl", "computer-use", "codex_app", "code-review"];

/// Servidores MCP do proprio usuario (`$CODEX_HOME/config.toml` + os embutidos), exceto `keep`, com a indicacao
/// de que tem transporte proprio no config.toml (`command` ou `url`). Nas execucoes do DOT eles ficam desligados:
/// o DOT so usa as ferramentas autorizadas pelo runtime (gateway `dots`), inclusive o navegador proprio; o computer
/// use do Codex nega permissao sozinho em modo nao interativo.
pub fn foreign_mcp_servers(home: &std::path::Path, keep: &str) -> Vec<(String, bool)> {
    let dir = std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()).map(std::path::PathBuf::from).unwrap_or_else(|| home.join(".codex"));
    let mut names: Vec<(String, bool)> = CODEX_BUNDLED_MCP.iter().map(|s| (s.to_string(), false)).collect();
    if let Ok(cfg) = std::fs::read_to_string(dir.join("config.toml")) {
        for line in cfg.lines() {
            let Some(rest) = line.trim().strip_prefix("[mcp_servers.") else { continue };
            let name = rest.trim_end_matches(']').split('.').next().unwrap_or("").trim_matches('"').trim();
            if name.is_empty() {
                continue;
            }
            match names.iter_mut().find(|(n, _)| n == name) {
                Some(found) => found.1 = true,
                None => names.push((name.to_string(), true)),
            }
        }
    }
    names.retain(|(n, _)| n != keep && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    names
}

impl AgentEngine for CodexAdapter {
    fn kind(&self) -> EngineKind {
        EngineKind::Codex
    }

    fn prepare(&self, req: &ExecutionRequest) -> DotsResult<PreparedCommand> {
        let mut args: Vec<String> = vec!["exec".into(), "--json".into(), "--color".into(), "never".into(), "--skip-git-repo-check".into()];
        args.push("-C".into());
        args.push(req.workspace.to_string_lossy().to_string());
        args.push("-s".into());
        args.push(if req.native_write { "workspace-write".into() } else { "read-only".into() });
        if let Some(m) = &req.model {
            args.push("-m".into());
            args.push(m.clone());
        }
        add_dirs(&mut args, "--add-dir", &req.extra_dirs);
        if let Some(g) = &req.gateway {
            let n = &g.server_name;
            args.push("-c".into());
            args.push(format!("mcp_servers.{n}.command={}", toml_str(&g.command.to_string_lossy())));
            args.push("-c".into());
            args.push(format!("mcp_servers.{n}.args=[{}]", g.args.iter().map(|a| toml_str(a)).collect::<Vec<_>>().join(",")));
            let env_items: Vec<String> = g.env.iter().map(|(k, v)| format!("{k}={}", toml_str(v))).collect();
            args.push("-c".into());
            args.push(format!("mcp_servers.{n}.env={{{}}}", env_items.join(",")));
            args.push("-c".into());
            args.push(format!("mcp_servers.{n}.default_tools_approval_mode=\"approve\""));
            // `enabled=false` sozinho falha para servidores que vem de plugins ("invalid transport"): um comando
            // inofensivo da ao override um transporte valido. Os do config.toml ja tem transporte, e `command` num
            // servidor por `url` falha ("url is not supported for stdio"): esses levam so `enabled=false`
            // (verificado com `codex mcp list`).
            for (other, in_config) in foreign_mcp_servers(&req.home, n) {
                if !in_config {
                    args.push("-c".into());
                    args.push(format!("mcp_servers.{other}.command=\"/usr/bin/true\""));
                }
                args.push("-c".into());
                args.push(format!("mcp_servers.{other}.enabled=false"));
            }
        }
        if let Some(session) = &req.resume_session {
            args.push("resume".into());
            args.push(session.clone());
        }
        // "-" = prompt via stdin
        args.push("-".into());
        Ok(PreparedCommand {
            program: req.executable.clone(),
            args,
            env: base_env(req),
            cwd: req.workspace.clone(),
            stdin: Some(req.prompt_with_instructions()),
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
            "thread.started" => {
                st.session_id = v.get("thread_id").and_then(|x| x.as_str()).map(String::from);
                vec![ExecutionEvent::Started { engine_session_id: st.session_id.clone() }]
            }
            "turn.started" => vec![],
            "item.started" | "item.updated" | "item.completed" => {
                let item = v.get("item").cloned().unwrap_or(Value::Null);
                let id = item.get("id").and_then(|x| x.as_str()).unwrap_or("item").to_string();
                let itype = item.get("type").and_then(|x| x.as_str()).unwrap_or("");
                let completed = t == "item.completed";
                match itype {
                    "agent_message" => {
                        if completed {
                            let text = item.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            st.last_text = text.clone();
                            vec![ExecutionEvent::Text { text }]
                        } else {
                            vec![]
                        }
                    }
                    "reasoning" => {
                        if completed {
                            let text = item.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            if text.is_empty() { vec![] } else { vec![ExecutionEvent::Reasoning { text }] }
                        } else {
                            vec![]
                        }
                    }
                    "command_execution" => {
                        let cmd = item.get("command").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        if t == "item.started" {
                            vec![ExecutionEvent::ToolCall { call_id: id, name: "command_execution".into(), input: serde_json::json!({ "command": cmd }) }]
                        } else if completed {
                            let status = item.get("status").and_then(|x| x.as_str()).unwrap_or("");
                            let output = item.get("aggregated_output").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            let exit = item.get("exit_code").and_then(|x| x.as_i64());
                            vec![ExecutionEvent::ToolResult { call_id: id, name: "command_execution".into(), ok: status == "completed" && exit.unwrap_or(0) == 0, output: if output.is_empty() { format!("status={status} exit={exit:?}") } else { output } }]
                        } else {
                            vec![]
                        }
                    }
                    "mcp_tool_call" => {
                        let name = format!("{}/{}", item.get("server").and_then(|x| x.as_str()).unwrap_or("mcp"), item.get("tool").and_then(|x| x.as_str()).unwrap_or("tool"));
                        if t == "item.started" {
                            vec![ExecutionEvent::ToolCall { call_id: id, name, input: item.get("arguments").cloned().unwrap_or(Value::Null) }]
                        } else if completed {
                            let error = item.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str());
                            let output = match error {
                                Some(e) => e.to_string(),
                                None => item
                                    .get("result")
                                    .and_then(|r| r.get("content"))
                                    .and_then(|c| c.as_array())
                                    .map(|arr| arr.iter().filter_map(|c| c.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("\n"))
                                    .unwrap_or_default(),
                            };
                            vec![ExecutionEvent::ToolResult { call_id: id, name, ok: error.is_none(), output }]
                        } else {
                            vec![]
                        }
                    }
                    "file_change" => {
                        if completed {
                            vec![ExecutionEvent::ToolResult { call_id: id, name: "file_change".into(), ok: item.get("status").and_then(|x| x.as_str()) != Some("failed"), output: serde_json::to_string(&item.get("changes").cloned().unwrap_or(Value::Null)).unwrap_or_default() }]
                        } else {
                            vec![ExecutionEvent::ToolCall { call_id: id, name: "file_change".into(), input: item.get("changes").cloned().unwrap_or(Value::Null) }]
                        }
                    }
                    other => {
                        if completed {
                            vec![ExecutionEvent::Notice { level: "info".into(), text: format!("{other}: {}", serde_json::to_string(&item).unwrap_or_default()) }]
                        } else {
                            vec![]
                        }
                    }
                }
            }
            "turn.completed" => {
                let usage = v.get("usage").cloned().unwrap_or(Value::Null);
                st.usage = usage.clone();
                vec![ExecutionEvent::Usage {
                    input_tokens: usage.get("input_tokens").and_then(|x| x.as_u64()),
                    output_tokens: usage.get("output_tokens").and_then(|x| x.as_u64()),
                    cost_usd: None,
                    context_tokens: None,
                }]
            }
            "turn.failed" | "error" => {
                let msg = v
                    .get("error")
                    .and_then(|e| e.get("message").and_then(|m| m.as_str()).or_else(|| e.as_str()))
                    .or_else(|| v.get("message").and_then(|m| m.as_str()))
                    .unwrap_or("falha no Codex")
                    .to_string();
                st.failed = true;
                vec![ExecutionEvent::Failed { kind: classify_failure(&msg), message: msg }]
            }
            _ => vec![ExecutionEvent::Raw { line: line.to_string() }],
        }
    }

    fn finish(&self, exit_code: Option<i32>, timed_out: bool, cancelled: bool, st: &mut ParserState) -> Vec<ExecutionEvent> {
        if !st.completed && !st.failed && exit_code == Some(0) && st.session_id.is_none() {
            st.failed = true;
            return vec![ExecutionEvent::Failed { kind: FailureKind::Engine, message: "Codex encerrou sem iniciar uma thread".into() }];
        }
        super::default_finish(exit_code, timed_out, cancelled, st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::test_support::{request, run_fixture};

    #[test]
    fn foreign_servers_come_from_config_and_bundled_list() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".codex")).unwrap();
        std::fs::write(dir.path().join(".codex/config.toml"), "[mcp_servers.flexi]\nurl = \"https://x/mcp\"\n[mcp_servers.flexi.env]\nA = \"1\"\n[mcp_servers.\"meu-srv\"]\ncommand = \"y\"\n[mcp_servers.node_repl]\ncommand = \"w\"\n[mcp_servers.dots]\ncommand = \"z\"\n").unwrap();
        let names = foreign_mcp_servers(dir.path(), "dots");
        let has = |n: &str, cfg: bool| names.contains(&(n.to_string(), cfg));
        assert!(has("cua_repl", false), "embutido fora do config.toml");
        assert!(has("node_repl", true), "embutido tambem no config.toml");
        assert!(has("flexi", true));
        assert!(has("meu-srv", true));
        assert!(!names.iter().any(|(n, _)| n == "dots"));
        assert_eq!(names.iter().filter(|(n, _)| n == "flexi").count(), 1, "sub-tabela .env nao duplica");
    }

    #[test]
    fn prepare_builds_verified_flags() {
        let mut req = request(EngineKind::Codex);
        let cmd = CodexAdapter.prepare(&req).unwrap();
        let a = cmd.args.join(" ");
        assert!(a.starts_with("exec --json --color never --skip-git-repo-check -C /tmp/ws -s read-only --add-dir /tmp/extra"));
        assert!(a.contains("mcp_servers.dots.command=\"/opt/dotsd\""));
        assert!(a.contains("mcp_servers.dots.args=[\"mcp-gateway\"]"));
        assert!(a.contains("mcp_servers.dots.env={DOTS_EXECUTION_TOKEN=\"tok\"}"));
        assert!(a.contains("default_tools_approval_mode=\"approve\""));
        assert!(a.contains("mcp_servers.cua_repl.command=\"/usr/bin/true\" -c mcp_servers.cua_repl.enabled=false") && a.contains("mcp_servers.node_repl.enabled=false"), "computer use do Codex desligado: {a}");
        assert!(!a.contains("mcp_servers.dots.enabled=false"));
        assert!(a.ends_with(" -"));
        assert!(cmd.stdin.as_ref().unwrap().contains("<dot_instructions>"));
        assert!(!a.contains("-a "), "exec nao aceita -a");
        assert!(!a.contains(" -m "), "sem modelo definido o CLI usa o proprio padrao");
        req.native_write = true;
        req.resume_session = Some("thread-1".into());
        req.model = Some("gpt-5-codex".into());
        let a2 = CodexAdapter.prepare(&req).unwrap().args.join(" ");
        assert!(a2.contains("-s workspace-write -m gpt-5-codex "));
        assert!(a2.ends_with("resume thread-1 -"));
    }

    #[test]
    fn parses_real_fixture() {
        let (evs, st) = run_fixture(EngineKind::Codex, "codex-exec-ok.jsonl");
        assert!(matches!(evs[0], ExecutionEvent::Started { engine_session_id: Some(ref s) } if s == "01a13fa6-66ab-78b2-b4f5-9aad5386cc52"));
        assert!(evs.iter().any(|e| matches!(e, ExecutionEvent::Text { text } if text == "OK")));
        assert!(evs.iter().any(|e| matches!(e, ExecutionEvent::Usage { input_tokens: Some(14779), .. })));
        assert!(matches!(evs.last(), Some(ExecutionEvent::Completed { result_text }) if result_text == "OK"));
        assert!(st.completed);
    }

    #[test]
    fn parses_mcp_tool_call_items() {
        let a = CodexAdapter;
        let mut st = ParserState::default();
        let started = r#"{"type":"item.started","item":{"id":"item_0","type":"mcp_tool_call","server":"dots","tool":"dots_echo","arguments":{"text":"oi"},"result":null,"error":null,"status":"in_progress"}}"#;
        let done = r#"{"type":"item.completed","item":{"id":"item_0","type":"mcp_tool_call","server":"dots","tool":"dots_echo","arguments":{"text":"oi"},"result":{"content":[{"type":"text","text":"ECHO: oi"}],"structured_content":null},"error":null,"status":"completed"}}"#;
        let failed = r#"{"type":"item.completed","item":{"id":"item_1","type":"mcp_tool_call","server":"dots","tool":"x","arguments":{},"result":null,"error":{"message":"MCP tool call requires approval, but approval policy is never"},"status":"failed"}}"#;
        assert!(matches!(a.parse_line(started, &mut st)[0], ExecutionEvent::ToolCall { ref name, .. } if name == "dots/dots_echo"));
        assert!(matches!(a.parse_line(done, &mut st)[0], ExecutionEvent::ToolResult { ok: true, ref output, .. } if output == "ECHO: oi"));
        assert!(matches!(a.parse_line(failed, &mut st)[0], ExecutionEvent::ToolResult { ok: false, .. }));
        let err = a.parse_line(r#"{"type":"error","message":"Not logged in"}"#, &mut st);
        assert!(matches!(err[0], ExecutionEvent::Failed { kind: FailureKind::Auth, .. }));
    }
}
