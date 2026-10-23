use std::process::{Command, Stdio};
use std::time::Duration;

use dots_flexi::{BrowserLoginPoll, FlexiClient, FlexiError, LoginOutcome};
use dots_flexi::catalog::{build_request, find_tool};
use serde_json::json;

struct Server {
    child: std::process::Child,
    port: u16,
}

impl Server {
    async fn start(mfa: bool) -> Self {
        Self::start_at(mfa, if mfa { 500 } else { 0 }).await
    }
    /// Testes rodam em paralelo: cada um usa um deslocamento de porta proprio.
    async fn start_at(mfa: bool, offset: u16) -> Self {
        let port = 18000 + (std::process::id() % 1000) as u16 + offset;
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
    let price_tool = find_tool("flexi_precos_calculados").unwrap();
    let (price_path, price_query, price_body) = build_request(price_tool, &json!({ "anuncio_id": 101 })).unwrap();
    let v = c.call(price_tool.method, &price_path, price_query.as_ref(), price_body.as_ref()).await.unwrap();
    assert_eq!(v["status"], 1);
    assert_eq!(v["dados"]["id"], 101);
    let history_tool = find_tool("flexi_historico_preco").unwrap();
    let (history_path, history_query, history_body) = build_request(history_tool, &json!({ "anuncio_id": 101 })).unwrap();
    let v = c.call(history_tool.method, &history_path, history_query.as_ref(), history_body.as_ref()).await.unwrap();
    assert_eq!(v["status"], 1);
    assert!(v["dados"].is_array());
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

#[tokio::test]
async fn browser_login_and_profile_photo() {
    let s = Server::start_at(false, 250).await;
    let c = FlexiClient::new(&s.url()).unwrap();
    assert!(matches!(c.poll_browser_login("tok-1").await, Ok(BrowserLoginPoll::Closed(ref st)) if st == "not_found"));
    assert!(c.start_browser_login("outro").await.is_err());
    let ch = c.start_browser_login("dots").await.unwrap();
    assert_eq!(ch.token, "tok-1");
    assert_eq!(ch.code, "ABC-123");
    assert_eq!(ch.authorize_url, format!("{}/login-external-app?token=tok-1", s.url()));
    assert_eq!(c.poll_browser_login(&ch.token).await.unwrap(), BrowserLoginPoll::Pending);
    assert!(c.session_cookie().is_none());
    reqwest::get(format!("{}/__approve", s.url())).await.unwrap();
    assert_eq!(c.poll_browser_login(&ch.token).await.unwrap(), BrowserLoginPoll::Authenticated);
    assert_eq!(c.session_cookie().as_deref(), Some("sess-browser"));
    assert_eq!(c.me().await.unwrap().email.as_deref(), Some("agente@teste.com"));
    assert_eq!(c.poll_browser_login(&ch.token).await.unwrap(), BrowserLoginPoll::Closed("consumed".into()));
    assert!(c.poll_browser_login("../api/me").await.is_err(), "token com caracteres invalidos");
    let (_, bytes) = c.profile_photo().await.unwrap().expect("foto");
    assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
    // recusado no navegador
    let c2 = FlexiClient::new(&s.url()).unwrap();
    c2.start_browser_login("dots").await.unwrap();
    reqwest::get(format!("{}/__deny", s.url())).await.unwrap();
    assert_eq!(c2.poll_browser_login("tok-1").await.unwrap(), BrowserLoginPoll::Closed("denied".into()));
    assert!(c2.session_cookie().is_none());
}
