use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

#[test]
fn catalog_fails_explicitly_when_gateway_cannot_reach_runtime() {
    for token in [None, Some("test-token")] {
        let dir = tempfile::tempdir().unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dotsd"));
        cmd.args(["mcp-gateway", "--data-dir", dir.path().to_str().unwrap()])
            .current_dir(dir.path())
            .env_remove("DOTS_EXECUTION_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(token) = token {
            cmd.env("DOTS_EXECUTION_TOKEN", token);
        }
        let mut child = cmd.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, "{}", json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap();
        stdin.flush().unwrap();
        let mut output = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut output).unwrap();
        let reply: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(reply["error"]["code"], -32603);
        assert!(reply.get("result").is_none(), "nao deve anunciar catalogo parcial: {reply}");
        drop(stdin);
        assert!(child.wait().unwrap().success());
    }
}
