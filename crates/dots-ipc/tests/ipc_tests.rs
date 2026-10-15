use std::sync::Arc;

use dots_core::DotsError;
use dots_ipc::{HandlerFuture, IpcClient, IpcServer, ServerConfig, SocketPaths};

fn echo_handler(method: String, params: serde_json::Value) -> HandlerFuture {
    Box::pin(async move {
        match method.as_str() {
            "echo" => Ok(params),
            "slow" => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                Ok(serde_json::json!("slow-done"))
            }
            "fail" => Err(DotsError::NotFound("coisa".into())),
            _ => Err(DotsError::Protocol(format!("metodo desconhecido: {method}"))),
        }
    })
}

async fn start() -> (tempfile::TempDir, SocketPaths, String, dots_ipc::server::EventSink, tokio::sync::oneshot::Sender<()>) {
    let dir = tempfile::tempdir().unwrap();
    let paths = SocketPaths::resolve(dir.path());
    let server = IpcServer::bind(paths.clone(), ServerConfig::default()).unwrap();
    let token = server.token().to_string();
    let sink = server.event_sink();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(server.serve(Arc::new(echo_handler), async move {
        let _ = stop_rx.await;
    }));
    (dir, paths, token, sink, stop_tx)
}

#[tokio::test]
async fn roundtrip_and_multiplexing() {
    let (_dir, paths, token, _sink, _stop) = start().await;
    let (client, _events) = IpcClient::connect(&paths.socket_path, &token, "test").await.unwrap();
    assert_eq!(client.hello().pid, std::process::id());

    let slow = client.call("slow", serde_json::json!({}));
    let fast = client.call("echo", serde_json::json!({"a": 1}));
    let (s, f) = tokio::join!(slow, fast);
    assert_eq!(s.unwrap(), serde_json::json!("slow-done"));
    assert_eq!(f.unwrap(), serde_json::json!({"a": 1}));

    let err = client.call("fail", serde_json::json!({})).await.unwrap_err();
    assert_eq!(err.code(), "NOT_FOUND");
}

#[tokio::test]
async fn wrong_token_is_rejected() {
    let (_dir, paths, _token, _sink, _stop) = start().await;
    let err = IpcClient::connect(&paths.socket_path, "0000", "test").await.err().unwrap();
    assert_eq!(err.code(), "UNAUTHORIZED");
}

#[tokio::test]
async fn request_before_hello_is_rejected() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_util::codec::{Framed, LinesCodec};
    let (_dir, paths, _token, _sink, _stop) = start().await;
    let stream = tokio::net::UnixStream::connect(&paths.socket_path).await.unwrap();
    let mut framed = Framed::new(stream, LinesCodec::new());
    framed.send(r#"{"id":1,"method":"echo","params":{}}"#.to_string()).await.unwrap();
    let line = framed.next().await.unwrap().unwrap();
    assert!(line.contains("UNAUTHORIZED"), "{line}");
    assert!(framed.next().await.is_none(), "servidor deve fechar a conexao");
}

#[tokio::test]
async fn events_are_delivered_with_sequence() {
    let (_dir, paths, token, sink, _stop) = start().await;
    let (_client, mut events) = IpcClient::connect(&paths.socket_path, &token, "test").await.unwrap();
    sink.emit("dots.changed", serde_json::json!({"id": "x"}));
    sink.emit("dots.changed", serde_json::json!({"id": "y"}));
    let a = events.recv().await.unwrap();
    let b = events.recv().await.unwrap();
    assert_eq!(a.event, "dots.changed");
    assert_eq!(b.seq, a.seq + 1);
}

#[tokio::test]
async fn shutdown_removes_socket_and_fails_pending() {
    let (_dir, paths, token, _sink, stop) = start().await;
    let (client, _events) = IpcClient::connect(&paths.socket_path, &token, "test").await.unwrap();
    assert!(paths.socket_path.exists());
    stop.send(()).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!paths.socket_path.exists());
    assert!(!paths.token_path.exists());
    // Conexoes ja aceitas continuam ate o servidor sair; novas conexoes falham.
    let err = IpcClient::connect(&paths.socket_path, &token, "test2").await.err().unwrap();
    assert_eq!(err.code(), "RUNTIME_UNAVAILABLE");
    let _ = client;
}

#[tokio::test]
async fn socket_permissions_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, paths, _token, _sink, _stop) = start().await;
    let mode = std::fs::metadata(&paths.socket_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}
