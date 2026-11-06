//! Host Native Messaging iniciado pelo Chromium. Stdio usa quadros com tamanho
//! little-endian; o lado do runtime usa o IPC local autenticado do DOTS.

use std::path::{Path, PathBuf};
use std::time::Duration;

use dots_ipc::{IpcClient, RuntimeInfo};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_MESSAGE: usize = 1_000_000;

async fn read_message<R: AsyncRead + Unpin>(input: &mut R) -> Result<Value, String> {
    let len = input.read_u32_le().await.map_err(|e| e.to_string())? as usize;
    if len > MAX_MESSAGE { return Err("mensagem da extensao excedeu 1 MB".into()); }
    let mut data = vec![0; len];
    input.read_exact(&mut data).await.map_err(|e| e.to_string())?;
    serde_json::from_slice(&data).map_err(|e| e.to_string())
}

async fn write_message<W: AsyncWrite + Unpin>(output: &mut W, message: &Value) -> Result<(), String> {
    let data = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    if data.len() > MAX_MESSAGE { return Err("comando para extensao excedeu 1 MB".into()); }
    output.write_u32_le(data.len() as u32).await.map_err(|e| e.to_string())?;
    output.write_all(&data).await.map_err(|e| e.to_string())?;
    output.flush().await.map_err(|e| e.to_string())
}

async fn connect(data_dir: &Path) -> Result<(IpcClient, dots_ipc::EventReceiver), String> {
    let info = RuntimeInfo::read(data_dir).map_err(|e| e.to_string())?.ok_or("runtime DOTS indisponivel")?;
    let token = std::fs::read_to_string(&info.token_path).map_err(|e| e.to_string())?;
    IpcClient::connect(Path::new(&info.socket_path), token.trim(), "chromium-native-host").await.map_err(|e| e.to_string())
}

pub async fn run(data_dir: PathBuf) -> i32 {
    let (client, mut events) = match connect(&data_dir).await {
        Ok(v) => v,
        Err(e) => { eprintln!("DOTS Chromium host: {e}"); return 1; }
    };
    let (tx, mut input_messages) = tokio::sync::mpsc::channel::<Value>(16);
    tokio::spawn(async move {
        let mut input = tokio::io::stdin();
        while let Ok(message) = read_message(&mut input).await {
            if tx.send(message).await.is_err() { break; }
        }
    });
    let mut output = tokio::io::stdout();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if client.call("browser.bridge.heartbeat", json!({})).await.is_err() { break; }
            }
            incoming = input_messages.recv() => {
                match incoming {
                    Some(message) if message["kind"] == "result" => {
                        let id = message["id"].as_str().unwrap_or("");
                        if !id.is_empty() {
                            let _ = client.call("browser.bridge.result", json!({ "id": id, "result": message })).await;
                        }
                    }
                    Some(_) => {}
                    None => break,
                }
            }
            Some(event) = events.recv() => {
                if event.event == "browser.personal.command" {
                    let mut command = event.payload;
                    command["kind"] = json!("command");
                    if let Err(e) = write_message(&mut output, &command).await {
                        eprintln!("DOTS Chromium host: {e}");
                        break;
                    }
                }
            }
            else => break,
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn native_message_round_trip() {
        let value = json!({ "kind": "result", "id": "x", "ok": true });
        let (mut writer, mut reader) = tokio::io::duplex(1024);
        write_message(&mut writer, &value).await.unwrap();
        assert_eq!(read_message(&mut reader).await.unwrap(), value);
    }
}
