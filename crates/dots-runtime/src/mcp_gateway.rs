//! `dotsd mcp-gateway`: servidor MCP (stdio, JSON-RPC 2.0) iniciado pelo CLI do motor.
//! Cada `tools/call` vira `gateway.call` no runtime via IPC, com o token da execucao.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dots_ipc::{IpcClient, RuntimeInfo};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::tools::tool_definitions;

fn resolve_token() -> Option<String> {
    if let Ok(t) = std::env::var("DOTS_EXECUTION_TOKEN") {
        if !t.trim().is_empty() {
            return Some(t.trim().to_string());
        }
    }
    let cwd = std::env::current_dir().ok()?;
    std::fs::read_to_string(cwd.join(".dots/execution.token")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

async fn connect(data_dir: &Path) -> Result<IpcClient, String> {
    let info = RuntimeInfo::read(data_dir).map_err(|e| e.to_string())?.ok_or("runtime nao esta em execucao")?;
    let token = std::fs::read_to_string(&info.token_path).map_err(|e| format!("token do runtime: {e}"))?;
    let (client, mut events) = IpcClient::connect(Path::new(&info.socket_path), token.trim(), "gateway").await.map_err(|e| e.to_string())?;
    tokio::spawn(async move { while events.recv().await.is_some() {} });
    Ok(client)
}

struct Gateway {
    data_dir: PathBuf,
    token: Option<String>,
    client: Mutex<Option<IpcClient>>,
    stdout: Mutex<tokio::io::Stdout>,
}

impl Gateway {
    async fn send(&self, v: Value) {
        let mut out = self.stdout.lock().await;
        let _ = out.write_all(format!("{v}\n").as_bytes()).await;
        let _ = out.flush().await;
    }

    async fn reply(&self, id: Value, result: Value) {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result })).await;
    }

    async fn reply_err(&self, id: Value, code: i64, message: String) {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })).await;
    }

    async fn client(&self) -> Result<IpcClient, String> {
        let mut guard = self.client.lock().await;
        if let Some(c) = guard.as_ref() {
            if c.is_connected() {
                return Ok(c.clone());
            }
        }
        let c = connect(&self.data_dir).await?;
        *guard = Some(c.clone());
        Ok(c)
    }

    async fn handle(self: Arc<Self>, req: Value) {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
        let params = req.get("params").cloned().unwrap_or(json!({}));
        match method.as_str() {
            "initialize" => {
                let pv = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("2025-06-18");
                self.reply(id, json!({ "protocolVersion": pv, "capabilities": { "tools": {} }, "serverInfo": { "name": "dots", "version": crate::VERSION } })).await;
            }
            "notifications/initialized" | "notifications/cancelled" | "notifications/roots/list_changed" => {}
            "ping" => self.reply(id, json!({})).await,
            "tools/list" => {
                // Lista por execucao (inclui ferramentas de aplicativos conectados ao DOT); cai na lista fixa sem token/runtime.
                let dynamic = match (&self.token, self.client().await) {
                    (Some(token), Ok(c)) => c.call("gateway.tools", json!({ "token": token })).await.ok(),
                    _ => None,
                };
                match dynamic {
                    Some(v) => self.reply(id, v).await,
                    None => self.reply(id, json!({ "tools": tool_definitions() })).await,
                }
            }
            "tools/call" => {
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let Some(token) = self.token.clone() else {
                    self.reply(id, json!({ "content": [{ "type": "text", "text": "gateway sem token de execucao (DOTS_EXECUTION_TOKEN ou .dots/execution.token)" }], "isError": true })).await;
                    return;
                };
                match self.client().await {
                    Ok(c) => match c.call("gateway.call", json!({ "token": token, "tool": name, "arguments": args })).await {
                        Ok(v) => self.reply(id, v).await,
                        Err(e) => self.reply(id, json!({ "content": [{ "type": "text", "text": format!("erro do runtime DOTS: {}", e.message()) }], "isError": true })).await,
                    },
                    Err(e) => self.reply(id, json!({ "content": [{ "type": "text", "text": format!("runtime DOTS indisponivel: {e}") }], "isError": true })).await,
                }
            }
            _ => {
                if !id.is_null() {
                    self.reply_err(id, -32601, format!("method not found: {method}")).await;
                }
            }
        }
    }
}

pub async fn run(data_dir: PathBuf) -> i32 {
    let gw = Arc::new(Gateway { data_dir, token: resolve_token(), client: Mutex::new(None), stdout: Mutex::new(tokio::io::stdout()) });
    if gw.token.is_none() {
        eprintln!("dotsd mcp-gateway: sem token de execucao; chamadas de ferramenta serao recusadas");
    }
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(req) => {
                let g = gw.clone();
                tokio::spawn(async move { g.handle(req).await });
            }
            Err(e) => eprintln!("dotsd mcp-gateway: linha invalida: {e}"),
        }
    }
    0
}
