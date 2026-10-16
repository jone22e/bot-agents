use std::sync::Arc;
use std::time::Duration;

use reqwest::cookie::{CookieStore, Jar};
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, thiserror::Error)]
pub enum FlexiError {
    /// Sessao ausente/expirada ou credenciais invalidas: e preciso logar de novo.
    #[error("nao autenticado no Flexi: {0}")]
    Unauthenticated(String),
    /// 2FA pendente: guardar `challenge_token` e pedir o codigo ao usuario.
    #[error("codigo de verificacao (2FA) necessario")]
    MfaRequired { challenge_token: String, expires_at: Option<String> },
    #[error("senha temporaria: troque a senha no Flexi antes de conectar")]
    TemporaryPassword,
    #[error("Flexi recusou: {0}")]
    Rejected(String),
    #[error("falha de rede/HTTP: {0}")]
    Http(String),
    #[error("resposta inesperada do Flexi: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, FlexiError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionInfo {
    pub user_id: Option<i64>,
    pub nome: Option<String>,
    pub email: Option<String>,
    pub base: Option<String>,
    pub administrador: bool,
    pub empresa_razao: Option<String>,
    pub empresa_id: Option<i64>,
    #[serde(default)]
    pub raw_user: Value,
}

fn session_from(user: &Value, empresa: Option<&Value>) -> SessionInfo {
    SessionInfo {
        user_id: user.get("usuario_id").and_then(|v| v.as_i64()).or_else(|| user.get("id").and_then(|v| v.as_i64())),
        nome: user.get("nome").and_then(|v| v.as_str()).map(String::from),
        email: user.get("email").and_then(|v| v.as_str()).map(String::from).or_else(|| user.get("login_email").and_then(|v| v.as_str()).map(String::from)),
        base: user.get("connection").and_then(|c| c.get("base")).and_then(|v| v.as_str()).map(String::from),
        administrador: user.get("administrador").map(|v| v.as_bool().unwrap_or(v.as_i64() == Some(1))).unwrap_or(false),
        empresa_razao: empresa.and_then(|e| e.get("razao")).and_then(|v| v.as_str()).map(String::from),
        empresa_id: empresa.and_then(|e| e.get("id")).and_then(|v| v.as_i64()),
        raw_user: {
            let mut u = user.clone();
            if let Some(o) = u.as_object_mut() {
                // Nunca guardar segredos do usuario.
                for k in ["token", "access_token", "senha", "password"] {
                    o.remove(k);
                }
            }
            u
        },
    }
}

#[derive(Debug, Clone)]
pub enum LoginOutcome {
    LoggedIn(SessionInfo),
    MfaRequired { challenge_token: String, expires_at: Option<String> },
}

/// Pedido de "Logar com Flexi": o usuario autoriza no navegador, ja logado no Flexi.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrowserLoginChallenge {
    pub token: String,
    /// Codigo exibido no app e na pagina do Flexi, para o usuario conferir.
    pub code: String,
    pub authorize_url: String,
    pub expires_at: Option<String>,
    pub poll_interval_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BrowserLoginPoll {
    Pending,
    /// Sessao recebida (cookie `userdata` ja esta no jar deste cliente).
    Authenticated,
    /// Encerrado sem sessao: `expired`, `denied`, `consumed` ou desconhecido.
    Closed(String),
}

/// Cliente HTTP com cookie jar (sessao `userdata`).
#[derive(Clone)]
pub struct FlexiClient {
    base_url: Url,
    jar: Arc<Jar>,
    http: reqwest::Client,
}

impl FlexiClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let base_url = Url::parse(base_url.trim_end_matches('/')).map_err(|e| FlexiError::Protocol(format!("URL invalida: {e}")))?;
        let jar = Arc::new(Jar::default());
        let http = reqwest::Client::builder()
            .cookie_provider(jar.clone())
            .timeout(Duration::from_secs(60))
            .user_agent(format!("DOTS/{} (conector Flexi)", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| FlexiError::Http(e.to_string()))?;
        Ok(Self { base_url, jar, http })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    fn url(&self, path: &str) -> Url {
        let mut u = self.base_url.clone();
        u.set_path(path);
        u.set_query(None);
        u
    }

    /// Cookie `userdata` atual (para persistir no Keychain).
    pub fn session_cookie(&self) -> Option<String> {
        let header = self.jar.cookies(&self.base_url)?;
        let s = header.to_str().ok()?;
        s.split(';').map(|p| p.trim()).find_map(|p| p.strip_prefix("userdata=").map(String::from))
    }

    /// Restaura um cookie guardado.
    pub fn restore_session_cookie(&self, value: &str) {
        self.jar.add_cookie_str(&format!("userdata={value}; Path=/"), &self.base_url);
    }

    pub async fn login(&self, email: &str, senha: &str, server: Option<&str>) -> Result<LoginOutcome> {
        let mut body = json!({ "email": email, "senha": senha });
        if let Some(s) = server.filter(|s| !s.trim().is_empty()) {
            body["server"] = Value::String(s.trim().to_string());
        }
        let resp = self.http.post(self.url("/api/auth/login")).json(&body).send().await.map_err(|e| FlexiError::Http(e.to_string()))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| FlexiError::Http(e.to_string()))?;
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        match status.as_u16() {
            200 if v.get("status").and_then(|s| s.as_i64()) == Some(1) => {
                let user = v.get("user").cloned().unwrap_or(Value::Null);
                if user.is_null() {
                    return Err(FlexiError::Protocol("login sem campo user".into()));
                }
                Ok(LoginOutcome::LoggedIn(session_from(&user, v.get("empresa"))))
            }
            202 => {
                if v.get("mfa_required").and_then(|b| b.as_bool()) == Some(true) {
                    Ok(LoginOutcome::MfaRequired {
                        challenge_token: v.get("challenge_token").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                        expires_at: v.get("expires_at").and_then(|t| t.as_str()).map(String::from),
                    })
                } else if v.get("temporary_password_change_required").and_then(|b| b.as_bool()) == Some(true) {
                    Err(FlexiError::TemporaryPassword)
                } else {
                    Err(FlexiError::Protocol(format!("202 inesperado: {}", text.chars().take(200).collect::<String>())))
                }
            }
            401 => Err(FlexiError::Unauthenticated("e-mail ou senha invalidos".into())),
            400 => Err(FlexiError::Rejected("informe e-mail e senha".into())),
            403 => Err(FlexiError::Rejected(v.get("motivo").or(v.get("error")).and_then(|m| m.as_str()).unwrap_or("login nao permitido para este usuario").to_string())),
            code => Err(FlexiError::Http(format!("HTTP {code}: {}", text.chars().take(200).collect::<String>()))),
        }
    }

    pub async fn verify_mfa(&self, challenge_token: &str, code: &str) -> Result<SessionInfo> {
        let resp = self
            .http
            .post(self.url("/api/auth/mfa/totp/login/verify"))
            .json(&json!({ "challenge_token": challenge_token, "code": code.trim() }))
            .send()
            .await
            .map_err(|e| FlexiError::Http(e.to_string()))?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if status.is_success() && v.get("status").and_then(|s| s.as_i64()) == Some(1) {
            let user = v.get("user").cloned().unwrap_or(Value::Null);
            return Ok(session_from(&user, v.get("empresa")));
        }
        Err(FlexiError::Rejected(v.get("motivo").or(v.get("error")).and_then(|m| m.as_str()).unwrap_or("codigo invalido ou expirado").to_string()))
    }

    /// `GET /api/me`: sessao valida?
    pub async fn me(&self) -> Result<SessionInfo> {
        let resp = self.http.get(self.url("/api/me")).send().await.map_err(|e| FlexiError::Http(e.to_string()))?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if status.as_u16() == 401 || v.get("authenticated").and_then(|a| a.as_bool()) != Some(true) {
            return Err(FlexiError::Unauthenticated("sessao expirada ou invalida".into()));
        }
        Ok(session_from(v.get("user").unwrap_or(&Value::Null), v.get("empresa")))
    }

    /// `POST /api/auth/app-externo/desafios`: cria o pedido e devolve a URL para abrir no navegador.
    pub async fn start_browser_login(&self, app: &str) -> Result<BrowserLoginChallenge> {
        let resp = self
            .http
            .post(self.url("/api/auth/app-externo/desafios"))
            .json(&json!({ "app": app }))
            .send()
            .await
            .map_err(|e| FlexiError::Http(e.to_string()))?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(FlexiError::Rejected(format!("HTTP {}: {}", status.as_u16(), v.get("error").and_then(|m| m.as_str()).unwrap_or("falha ao iniciar o login"))));
        }
        let token = v.get("token").and_then(|t| t.as_str()).filter(|t| !t.is_empty()).ok_or_else(|| FlexiError::Protocol("desafio sem token".into()))?;
        let path = v.get("authorize_path").and_then(|t| t.as_str()).ok_or_else(|| FlexiError::Protocol("desafio sem authorize_path".into()))?;
        let authorize_url = self.base_url.join(path).map_err(|e| FlexiError::Protocol(format!("authorize_path invalido: {e}")))?;
        Ok(BrowserLoginChallenge {
            token: token.to_string(),
            code: v.get("codigo").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            authorize_url: authorize_url.to_string(),
            expires_at: v.get("expires_at").and_then(|t| t.as_str()).map(String::from),
            poll_interval_ms: v.get("poll_interval_ms").and_then(|t| t.as_u64()).unwrap_or(2000),
        })
    }

    /// `GET /api/auth/app-externo/desafios/:token`: depois da autorizacao o Flexi grava o cookie de sessao.
    pub async fn poll_browser_login(&self, token: &str) -> Result<BrowserLoginPoll> {
        if token.is_empty() || !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(FlexiError::Protocol("token de login invalido".into()));
        }
        let resp = self.http.get(self.url(&format!("/api/auth/app-externo/desafios/{token}"))).send().await.map_err(|e| FlexiError::Http(e.to_string()))?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if status.as_u16() == 404 {
            return Ok(BrowserLoginPoll::Closed("not_found".into()));
        }
        if !status.is_success() {
            return Err(FlexiError::Rejected(format!("HTTP {}: {}", status.as_u16(), v.get("error").and_then(|m| m.as_str()).unwrap_or("falha ao acompanhar o login"))));
        }
        Ok(match v.get("status").and_then(|s| s.as_str()).unwrap_or("") {
            "pending" | "approved" => BrowserLoginPoll::Pending,
            "authenticated" if self.session_cookie().is_some() => BrowserLoginPoll::Authenticated,
            "authenticated" => return Err(FlexiError::Protocol("Flexi confirmou o login mas nao enviou a sessao".into())),
            other => BrowserLoginPoll::Closed(other.to_string()),
        })
    }

    /// Foto de perfil do usuario logado (`GET /api/me/profile-photo`). `None` quando ele nao tem foto.
    pub async fn profile_photo(&self) -> Result<Option<(String, Vec<u8>)>> {
        let resp = self
            .http
            .get(self.url("/api/me/profile-photo"))
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| FlexiError::Http(e.to_string()))?;
        match resp.status().as_u16() {
            401 => return Err(FlexiError::Unauthenticated("sessao expirada".into())),
            404 => return Ok(None),
            s if !(200..300).contains(&s) => return Err(FlexiError::Http(format!("HTTP {s} ao buscar a foto de perfil"))),
            _ => {}
        }
        let v: Value = resp.json().await.map_err(|e| FlexiError::Protocol(e.to_string()))?;
        let Some(photo_url) = v.get("url").and_then(|u| u.as_str()).filter(|u| !u.is_empty()) else { return Ok(None) };
        let img = self.http.get(photo_url).send().await.map_err(|e| FlexiError::Http(e.to_string()))?;
        if !img.status().is_success() {
            return Err(FlexiError::Http(format!("HTTP {} ao baixar a foto de perfil", img.status().as_u16())));
        }
        let mime = img.headers().get(reqwest::header::CONTENT_TYPE).and_then(|h| h.to_str().ok()).unwrap_or("image/jpeg").to_string();
        let bytes = img.bytes().await.map_err(|e| FlexiError::Http(e.to_string()))?;
        Ok(Some((mime, bytes.to_vec())))
    }

    pub async fn logout(&self) {
        let _ = self.http.post(self.url("/api/auth/logout")).send().await;
    }

    /// Chamada generica autenticada. `query` vai na URL; `body` como JSON.
    pub async fn call(&self, method: &str, path: &str, query: Option<&Value>, body: Option<&Value>) -> Result<Value> {
        let method = Method::from_bytes(method.as_bytes()).map_err(|_| FlexiError::Protocol(format!("metodo invalido {method}")))?;
        let mut url = self.url(path);
        if let Some(Value::Object(q)) = query {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in q {
                let s = match v {
                    Value::String(s) => s.clone(),
                    Value::Null => continue,
                    other => other.to_string(),
                };
                pairs.append_pair(k, &s);
            }
        }
        let mut req = self.http.request(method, url);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(|e| FlexiError::Http(e.to_string()))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| FlexiError::Http(e.to_string()))?;
        if status.as_u16() == 401 {
            return Err(FlexiError::Unauthenticated("sessao expirada".into()));
        }
        let v: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text.chars().take(4000).collect::<String>() }));
        if !status.is_success() {
            return Err(FlexiError::Rejected(format!("HTTP {}: {}", status.as_u16(), v.get("motivo").or(v.get("error")).and_then(|m| m.as_str()).unwrap_or(&text.chars().take(300).collect::<String>()))));
        }
        Ok(v)
    }
}
