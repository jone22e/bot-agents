use std::process::{Command, Stdio};
use std::time::Duration;

use dots_flexi::{FlexiClient, FlexiError, LoginOutcome};
use serde_json::json;

struct Server {
    child: std::process::Child,
    port: u16,
}

impl Server {
    async fn start(mfa: bool) -> Self {
        let port = 18000 + (std::process::id() % 1000) as u16 + if mfa { 500 } else { 0 };
        let mut args = vec![concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake_flexi.py").to_string(), port.to_string()];
        if mfa {
            args.push("--mfa".into());
        }
        let child = Command::new("python3").arg("-I").args(&args).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap();
        for _ in 0..50 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Self { child, port }
    }
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn login_me_call_and_expiry() {
    let s = Server::start(false).await;
    let c = FlexiClient::new(&s.url()).unwrap();
    assert!(matches!(c.me().await, Err(FlexiError::Unauthenticated(_))));
    assert!(matches!(c.login("agente@teste.com", "errada", None).await, Err(FlexiError::Unauthenticated(_))));
    let out = c.login("agente@teste.com", "segredo123", Some("teste")).await.unwrap();
    let LoginOutcome::LoggedIn(info) = out else { panic!("esperava login") };
    assert_eq!(info.nome.as_deref(), Some("Agente Teste"));
    assert_eq!(info.base.as_deref(), Some("c2s_teste"));
    assert_eq!(info.empresa_razao.as_deref(), Some("Empresa Teste"));
    assert!(info.raw_user.get("token").is_none(), "segredo removido");
    let cookie = c.session_cookie().expect("cookie userdata");
    assert!(cookie.starts_with("sess-"));
    assert!(c.me().await.is_ok());
    let v = c.call("POST", "/api/gestao-marketplaces/anuncios", None, Some(&json!({ "ids": [101] }))).await.unwrap();
    assert_eq!(v["dados"][0]["nome"], "Furadeira X");
    let v = c.call("GET", "/api/gestao-marketplaces/anuncios/precos-calculados", Some(&json!({ "ids": "101,102" })), None).await.unwrap();
    assert_eq!(v["status"], 1);
    // cookie restaurado em novo cliente funciona
    let c2 = FlexiClient::new(&s.url()).unwrap();
    c2.restore_session_cookie(&cookie);
    assert!(c2.me().await.is_ok());
    // sessao invalidada -> Unauthenticated
    reqwest::get(format!("{}/__expire", s.url())).await.unwrap();
    assert!(matches!(c.me().await, Err(FlexiError::Unauthenticated(_))));
    assert!(matches!(c.call("POST", "/api/gestao-marketplaces/anuncios", None, Some(&json!({}))).await, Err(FlexiError::Unauthenticated(_))));
}

#[tokio::test]
async fn mfa_flow() {
    let s = Server::start(true).await;
    let c = FlexiClient::new(&s.url()).unwrap();
    let out = c.login("agente@teste.com", "segredo123", None).await.unwrap();
    let LoginOutcome::MfaRequired { challenge_token, .. } = out else { panic!("esperava mfa") };
    assert_eq!(challenge_token, "chal-1");
    assert!(matches!(c.verify_mfa(&challenge_token, "000000").await, Err(FlexiError::Rejected(_))));
    let info = c.verify_mfa(&challenge_token, "123456").await.unwrap();
    assert_eq!(info.user_id, Some(7));
    assert!(c.me().await.is_ok());
}
