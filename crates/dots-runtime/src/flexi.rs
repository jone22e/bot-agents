//! Servico do conector Flexi: conexoes, sessao, reconexao e execucao das ferramentas `flexi_*`.

use std::collections::HashMap;
use std::sync::Arc;

use dots_core::{AppConnection, ConnectionStatus, DotsError, DotsResult, FlexiConnectInput, RiskLevel};
use dots_flexi::{BrowserLoginPoll, FlexiClient, FlexiError, LoginOutcome, SecretStore, SessionInfo, ToolRisk, find_tool};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::policy::SharedContext;
use crate::service::RuntimeService;
use crate::tools::ToolOutput;

pub const KIND: &str = "flexi";
pub const DEFAULT_BASE_URL: &str = "https://flexi.c2s-business.com";
pub(crate) const ACCOUNT_CONNECTION_SETTING: &str = "account.flexi_connection_id";
pub(crate) const AGENTS_CONNECTION_SETTING: &str = "agents.flexi_connection_id";
const MAX_RESULT_CHARS: usize = 60_000;

/// Clientes vivos por conexao + desafios 2FA pendentes + logins pelo navegador em andamento (por token).
pub struct FlexiRegistry {
    pub clients: Mutex<HashMap<String, FlexiClient>>,
    pub mfa: Mutex<HashMap<String, (FlexiClient, String)>>,
    pub browser: Mutex<HashMap<String, FlexiClient>>,
    pub secrets: Arc<dyn SecretStore>,
    /// Ultimas especialidades lidas do Flexi (cargo dos contatos e reserva quando o Flexi nao responde).
    pub templates: std::sync::Mutex<Option<Vec<crate::flexi_team::FlexiRoleTemplate>>>,
}

impl FlexiRegistry {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { clients: Mutex::new(HashMap::new()), mfa: Mutex::new(HashMap::new()), browser: Mutex::new(HashMap::new()), secrets, templates: std::sync::Mutex::new(None) }
    }
    fn cookie_key(id: &str) -> String {
        format!("flexi:{id}:cookie")
    }
    fn password_key(id: &str) -> String {
        format!("flexi:{id}:password")
    }
}

/// O S3 pode responder `binary/octet-stream`; descobre o tipo pela assinatura do arquivo.
fn sniff_image_mime(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'G', b'I', b'F', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "image/jpeg",
    }
}

pub fn risk_of(tool: &dots_flexi::FlexiTool) -> RiskLevel {
    match tool.risk {
        ToolRisk::Read => RiskLevel::Read,
        ToolRisk::Write => RiskLevel::Write,
        ToolRisk::Critical => RiskLevel::Critical,
    }
}

fn truncate(v: &Value) -> String {
    let s = serde_json::to_string_pretty(v).unwrap_or_default();
    if s.len() > MAX_RESULT_CHARS {
        let mut t: String = s.chars().take(MAX_RESULT_CHARS).collect();
        t.push_str("\n…[resposta truncada; filtre por ids/periodo para reduzir]");
        t
    } else {
        s
    }
}

impl RuntimeService {
    pub(crate) async fn flexi_client(&self, conn: &AppConnection) -> DotsResult<FlexiClient> {
        if let Some(c) = self.flexi.clients.lock().await.get(&conn.id) {
            return Ok(c.clone());
        }
        let c = FlexiClient::new(&conn.base_url).map_err(|e| DotsError::Validation(e.to_string()))?;
        if let Some(cookie) = self.flexi.secrets.get(&FlexiRegistry::cookie_key(&conn.id)) {
            c.restore_session_cookie(&cookie);
        }
        self.flexi.clients.lock().await.insert(conn.id.clone(), c.clone());
        Ok(c)
    }

    async fn flexi_store_session(&self, conn_id: &str, client: &FlexiClient, info: &SessionInfo) -> DotsResult<AppConnection> {
        if let Some(cookie) = client.session_cookie() {
            if let Err(e) = self.flexi.secrets.set(&FlexiRegistry::cookie_key(conn_id), &cookie) {
                tracing::warn!(error = %e, "nao foi possivel guardar o cookie no Keychain");
            }
        }
        let (id, nome, uid, empresa, base) = (conn_id.to_string(), info.nome.clone(), info.user_id, info.empresa_razao.clone(), info.base.clone());
        self.storage.call(move |s| { s.apps().set_session(&id, nome.as_deref(), uid, empresa.as_deref(), base.as_deref())?; s.apps().get(&id) }).await
    }

    /// Conecta (ou reconecta) uma conexao Flexi. Retorna a conexao e se ha 2FA pendente.
    pub async fn flexi_connect(&self, input: FlexiConnectInput, existing_id: Option<String>) -> DotsResult<AppConnection> {
        let email = input.email.trim().to_lowercase();
        if email.is_empty() || input.senha.is_empty() {
            return Err(DotsError::Validation("informe e-mail e senha".into()));
        }
        let base_url = input.base_url.clone().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| DEFAULT_BASE_URL.into());
        let conn = match existing_id {
            Some(id) => self.storage.call(move |s| s.apps().get(&id)).await?,
            None => {
                let label = input.label.clone().filter(|l| !l.trim().is_empty()).unwrap_or_else(|| format!("Flexi ({email})"));
                let (bu, server, em, remember) = (base_url.clone(), input.server.clone(), email.clone(), input.remember_password);
                self.storage.call(move |s| s.apps().create(KIND, &label, &bu, server.as_deref(), &em, ConnectionStatus::NeedsLogin, remember)).await?
            }
        };
        let client = FlexiClient::new(&conn.base_url).map_err(|e| DotsError::Validation(e.to_string()))?;
        let outcome = client.login(&email, &input.senha, input.server.as_deref().or(conn.server.as_deref())).await;
        if input.remember_password {
            let _ = self.flexi.secrets.set(&FlexiRegistry::password_key(&conn.id), &input.senha);
            let id = conn.id.clone();
            let _ = self.storage.call(move |s| s.apps().set_remember(&id, true)).await;
        } else {
            self.flexi.secrets.delete(&FlexiRegistry::password_key(&conn.id));
            let id = conn.id.clone();
            let _ = self.storage.call(move |s| s.apps().set_remember(&id, false)).await;
        }
        match outcome {
            Ok(LoginOutcome::LoggedIn(info)) => {
                self.flexi.clients.lock().await.insert(conn.id.clone(), client.clone());
                let c = self.flexi_store_session(&conn.id, &client, &info).await?;
                self.audit("app.flexi.connect", "app_connection", Some(&c.id), json!({ "email": email, "user": info.nome })).await?;
                self.events.emit("apps.changed", json!({ "id": c.id, "status": "connected" }));
                Ok(c)
            }
            Ok(LoginOutcome::MfaRequired { challenge_token, expires_at }) => {
                self.flexi.mfa.lock().await.insert(conn.id.clone(), (client, challenge_token));
                let (id, detail) = (conn.id.clone(), format!("codigo 2FA necessario{}", expires_at.map(|e| format!(" (ate {e})")).unwrap_or_default()));
                self.storage.call(move |s| s.apps().set_status(&id, ConnectionStatus::MfaPending, Some(&detail))).await?;
                let c = self.storage.call({ let id = conn.id.clone(); move |s| s.apps().get(&id) }).await?;
                self.events.emit("apps.changed", json!({ "id": c.id, "status": "mfa_pending" }));
                Ok(c)
            }
            Err(e) => {
                let (id, msg, st) = (conn.id.clone(), e.to_string(), match e { FlexiError::Http(_) | FlexiError::Protocol(_) => ConnectionStatus::Error, _ => ConnectionStatus::NeedsLogin });
                self.storage.call(move |s| s.apps().set_status(&id, st, Some(&msg))).await?;
                self.events.emit("apps.changed", json!({ "id": conn.id, "status": st.as_str() }));
                Err(match e {
                    FlexiError::Unauthenticated(m) => DotsError::Unauthorized(m),
                    other => DotsError::Engine(other.to_string()),
                })
            }
        }
    }

    pub async fn flexi_verify_mfa(&self, id: &str, code: &str) -> DotsResult<AppConnection> {
        let pending = self.flexi.mfa.lock().await.get(id).cloned();
        let Some((client, challenge)) = pending else {
            return Err(DotsError::Validation("nenhum desafio 2FA pendente; conecte de novo".into()));
        };
        match client.verify_mfa(&challenge, code).await {
            Ok(info) => {
                self.flexi.mfa.lock().await.remove(id);
                self.flexi.clients.lock().await.insert(id.to_string(), client.clone());
                let c = self.flexi_store_session(id, &client, &info).await?;
                self.audit("app.flexi.connect", "app_connection", Some(id), json!({ "mfa": true, "user": info.nome })).await?;
                self.events.emit("apps.changed", json!({ "id": id, "status": "connected" }));
                Ok(c)
            }
            Err(e) => Err(DotsError::Unauthorized(e.to_string())),
        }
    }

    /// Verifica a sessao (`/api/me`); tenta relogar com a senha guardada quando expirada.
    pub async fn flexi_check(&self, id: &str) -> DotsResult<AppConnection> {
        let conn = self.storage.call({ let id = id.to_string(); move |s| s.apps().get(&id) }).await?;
        let client = self.flexi_client(&conn).await?;
        match client.me().await {
            Ok(info) => self.flexi_store_session(&conn.id, &client, &info).await,
            Err(FlexiError::Unauthenticated(_)) => self.flexi_relogin(&conn, client).await,
            Err(e) => {
                let (cid, msg) = (conn.id.clone(), e.to_string());
                self.storage.call(move |s| s.apps().set_status(&cid, ConnectionStatus::Error, Some(&msg))).await?;
                self.storage.call({ let id = conn.id.clone(); move |s| s.apps().get(&id) }).await
            }
        }
    }

    pub(crate) async fn flexi_relogin(&self, conn: &AppConnection, client: FlexiClient) -> DotsResult<AppConnection> {
        if conn.remember_password {
            if let Some(pw) = self.flexi.secrets.get(&FlexiRegistry::password_key(&conn.id)) {
                match client.login(&conn.email, &pw, conn.server.as_deref()).await {
                    Ok(LoginOutcome::LoggedIn(info)) => {
                        tracing::info!(connection = %conn.id, "sessao Flexi renovada automaticamente");
                        let c = self.flexi_store_session(&conn.id, &client, &info).await?;
                        self.events.emit("apps.changed", json!({ "id": c.id, "status": "connected" }));
                        return Ok(c);
                    }
                    Ok(LoginOutcome::MfaRequired { challenge_token, .. }) => {
                        self.flexi.mfa.lock().await.insert(conn.id.clone(), (client, challenge_token));
                        let cid = conn.id.clone();
                        self.storage.call(move |s| s.apps().set_status(&cid, ConnectionStatus::MfaPending, Some("sessao expirou; informe o codigo 2FA"))).await?;
                        self.events.emit("apps.changed", json!({ "id": conn.id, "status": "mfa_pending" }));
                        return self.storage.call({ let id = conn.id.clone(); move |s| s.apps().get(&id) }).await;
                    }
                    Err(e) => tracing::warn!(connection = %conn.id, error = %e, "relogin automatico falhou"),
                }
            }
        }
        let cid = conn.id.clone();
        self.storage.call(move |s| s.apps().set_status(&cid, ConnectionStatus::NeedsLogin, Some("sessao expirou; reconecte"))).await?;
        self.events.emit("apps.changed", json!({ "id": conn.id, "status": "needs_login" }));
        self.storage.call({ let id = conn.id.clone(); move |s| s.apps().get(&id) }).await
    }

    /// "Logar com Flexi": cria o pedido no Flexi; a interface abre `authorize_url` no navegador.
    pub async fn flexi_browser_login_start(&self, base_url: Option<String>) -> DotsResult<Value> {
        let base_url = base_url.filter(|u| !u.trim().is_empty()).unwrap_or_else(|| DEFAULT_BASE_URL.into());
        let client = FlexiClient::new(&base_url).map_err(|e| DotsError::Validation(e.to_string()))?;
        let challenge = client.start_browser_login("dots").await.map_err(|e| DotsError::Engine(e.to_string()))?;
        self.flexi.browser.lock().await.insert(challenge.token.clone(), client);
        Ok(json!(challenge))
    }

    /// Acompanha o pedido. Autorizado: cria (ou atualiza, pelo e-mail) a conexao com a sessao recebida.
    pub async fn flexi_browser_login_poll(&self, token: &str) -> DotsResult<Value> {
        let Some(client) = self.flexi.browser.lock().await.get(token).cloned() else {
            return Ok(json!({ "status": "not_found" }));
        };
        match client.poll_browser_login(token).await.map_err(|e| DotsError::Engine(e.to_string()))? {
            BrowserLoginPoll::Pending => Ok(json!({ "status": "pending" })),
            BrowserLoginPoll::Closed(status) => {
                self.flexi.browser.lock().await.remove(token);
                Ok(json!({ "status": status }))
            }
            BrowserLoginPoll::Authenticated => {
                self.flexi.browser.lock().await.remove(token);
                let info = client.me().await.map_err(|e| DotsError::Engine(e.to_string()))?;
                let email = info.email.clone().unwrap_or_default().trim().to_lowercase();
                if email.is_empty() {
                    return Err(DotsError::Engine("o Flexi nao informou o e-mail do usuario".into()));
                }
                let base_url = client.base_url().as_str().trim_end_matches('/').to_string();
                let (bu, em) = (base_url.clone(), email.clone());
                let conn = self
                    .storage
                    .call(move |s| {
                        let existing = s.apps().list(Some(KIND))?.into_iter().find(|c| c.base_url == bu && c.email.eq_ignore_ascii_case(&em));
                        match existing {
                            Some(c) => Ok(c),
                            None => s.apps().create(KIND, &format!("Flexi ({em})"), &bu, None, &em, ConnectionStatus::NeedsLogin, false),
                        }
                    })
                    .await?;
                self.flexi.mfa.lock().await.remove(&conn.id);
                self.flexi.clients.lock().await.insert(conn.id.clone(), client.clone());
                let c = self.flexi_store_session(&conn.id, &client, &info).await?;
                self.audit("app.flexi.connect", "app_connection", Some(&c.id), json!({ "email": email, "user": info.nome, "browser": true })).await?;
                self.events.emit("apps.changed", json!({ "id": c.id, "status": "connected" }));
                Ok(json!({ "status": "authenticated", "connection": c }))
            }
        }
    }

    /// Para de acompanhar um pedido (o usuario cancelou no app).
    pub async fn flexi_browser_login_cancel(&self, token: &str) {
        self.flexi.browser.lock().await.remove(token);
    }

    /// Foto de perfil do usuario da conexao, como data URL (`null` se ele nao tiver foto).
    pub async fn flexi_profile_photo(&self, id: &str) -> DotsResult<Value> {
        use base64::Engine as _;
        let conn = self.storage.call({ let id = id.to_string(); move |s| s.apps().get(&id) }).await?;
        let client = self.flexi_client(&conn).await?;
        match client.profile_photo().await {
            Ok(Some((mime, bytes))) => {
                let mime = if mime.starts_with("image/") { mime } else { sniff_image_mime(&bytes).to_string() };
                Ok(json!({ "data_url": format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes)) }))
            }
            Ok(None) => Ok(json!({ "data_url": null })),
            Err(e) => Err(DotsError::Engine(e.to_string())),
        }
    }

    pub async fn flexi_remove(&self, id: &str) -> DotsResult<()> {
        if let Some(c) = self.flexi.clients.lock().await.remove(id) {
            c.logout().await;
        }
        self.flexi.mfa.lock().await.remove(id);
        self.flexi.secrets.delete(&FlexiRegistry::cookie_key(id));
        self.flexi.secrets.delete(&FlexiRegistry::password_key(id));
        let i = id.to_string();
        self.storage.call(move |s| {
            s.apps().delete(&i)?;
            if s.settings().get(AGENTS_CONNECTION_SETTING)?.and_then(|setting| setting.value.as_str().map(String::from)).as_deref() == Some(i.as_str()) {
                s.settings().delete(AGENTS_CONNECTION_SETTING)?;
            }
            Ok(())
        }).await?;
        self.audit("app.flexi.remove", "app_connection", Some(id), json!({})).await?;
        self.events.emit("apps.changed", json!({ "id": id, "deleted": true }));
        Ok(())
    }

    /// Conexao Flexi usada por um DOT. A conta escolhida para os agentes prevalece sobre vinculos antigos.
    /// Se `url` e do Flexi deste DOT, coloca o cookie de sessao no navegador dele: a pagina abre logada.
    pub async fn browser_login_for_app(&self, dot_id: &str, browser: &crate::browser::Browser, url: &str) -> DotsResult<bool> {
        let Some(conn) = self.flexi_connection_for_dot(dot_id).await? else { return Ok(false) };
        let host = crate::policy::url_host(&conn.base_url);
        let target = crate::policy::url_host(url);
        if host.is_empty() || !(target == host || target.ends_with(&format!(".{host}"))) {
            return Ok(false);
        }
        let Some(cookie) = self.flexi.secrets.get(&FlexiRegistry::cookie_key(&conn.id)) else { return Ok(false) };
        browser.set_cookie("userdata", &cookie, &conn.base_url).await?;
        Ok(true)
    }

    pub async fn flexi_connection_for_dot(&self, dot_id: &str) -> DotsResult<Option<AppConnection>> {
        let d = dot_id.to_string();
        self.storage
            .call(move |s| {
                if let Some(setting) = s.settings().get(AGENTS_CONNECTION_SETTING)? {
                    let id = setting.value.as_str();
                    return Ok(s.apps().list(Some(KIND))?.into_iter().find(|c| Some(c.id.as_str()) == id));
                }
                // Depois que ha uma conta pessoal, a escolha da conta dos agentes e obrigatoria.
                if s.settings().get(ACCOUNT_CONNECTION_SETTING)?.is_some() {
                    return Ok(None);
                }
                if let Some(i) = s.apps().integration(&d, KIND)? {
                    return Ok(Some(s.apps().get(&i.connection_id)?));
                }
                let all = s.apps().list(Some(KIND))?;
                Ok(if all.len() == 1 { Some(all[0].clone()) } else { None })
            })
            .await
    }

    /// Executa uma ferramenta `flexi_*` no contexto de uma execucao (ja autorizada pela politica).
    pub async fn flexi_execute(&self, ctx: &SharedContext, tool_name: &str, args: &Value) -> DotsResult<ToolOutput> {
        let Some(tool) = find_tool(tool_name) else {
            return Ok(ToolOutput::err(format!("ferramenta Flexi desconhecida: {tool_name}")));
        };
        let Some(conn) = self.flexi_connection_for_dot(&ctx.dot_id).await? else {
            return Ok(ToolOutput::err("este DOT nao esta ligado a nenhuma conexao Flexi (configure em Conectar aplicativos / perfil do DOT)".into()));
        };
        let (path, query, body) = match dots_flexi::catalog::build_request(tool, args) {
            Ok(x) => x,
            Err(e) => return Ok(ToolOutput::err(format!("parametros invalidos: {e}"))),
        };
        let client = self.flexi_client(&conn).await?;
        let mut result = client.call(tool.method, &path, query.as_ref(), body.as_ref()).await;
        if matches!(result, Err(FlexiError::Unauthenticated(_))) {
            // Sessao caiu: tenta renovar uma vez e repete.
            let renewed = self.flexi_relogin(&conn, client.clone()).await?;
            if renewed.status == ConnectionStatus::Connected {
                result = client.call(tool.method, &path, query.as_ref(), body.as_ref()).await;
            }
        }
        match result {
            Ok(v) => {
                let flexi_status = v.get("status").and_then(|s| s.as_i64());
                let mut out = if flexi_status == Some(0) {
                    ToolOutput::err(format!("Flexi recusou: {}", v.get("motivo").or(v.get("error")).and_then(|m| m.as_str()).unwrap_or("sem motivo")))
                } else {
                    ToolOutput::ok(truncate(&v))
                };
                if tool.risk != ToolRisk::Read {
                    out.changes.push((format!("flexi:{}", path), "flexi".into(), format!("{} {}", tool.name, args.to_string().chars().take(200).collect::<String>())));
                }
                Ok(out)
            }
            Err(FlexiError::Unauthenticated(m)) => Ok(ToolOutput::err(format!("sessao do Flexi expirou e nao foi possivel renovar ({m}). Peca ao usuario para reconectar em Conectar aplicativos."))),
            Err(e) => Ok(ToolOutput::err(format!("erro do Flexi: {e}"))),
        }
    }

    /// Definicoes MCP das ferramentas Flexi para um DOT (vazio se nao houver conexao).
    pub async fn flexi_tools_for_dot(&self, dot_id: &str) -> Vec<Value> {
        match self.flexi_connection_for_dot(dot_id).await {
            Ok(Some(_)) => dots_flexi::tool_definitions(),
            _ => vec![],
        }
    }
}

/// Resumo das regras de preco do Flexi (fonte: flexi2/docs/regras-preco-anuncios-marketplaces.md). Usado nas instrucoes dos papeis.
/// Mapa do Flexi injetado nas instrucoes de todo DOT ligado a uma conexao Flexi. Fatos verificados no
/// backend do Flexi (2026-10-15): marketplace 2 = Google Shopping (Merchant Center); `criar-anuncio`
/// aceita google-shopping; `preparar-criacao` so ML e Shopee; `publicar` envia o item ao Merchant Center;
/// campanhas do Google Ads (`flexi_ads_publicar_produtos`) usam o Merchant Center vinculado a conta.
pub const FLEXI_CONTEXT: &str = "Mapa do Flexi (ferramentas flexi_*):\n\
- Marketplaces por numero: 0 Mercado Livre, 1 Shopee, 2 Google Shopping. Um 'anuncio' e a publicacao de um produto em um marketplace; filtre e crie sempre informando o numero do marketplace.\n\
- Google tem DUAS coisas distintas no Flexi: (a) o anuncio no Google Shopping (marketplace 2) e o item do catalogo enviado ao Google Merchant Center, visivel na aba Anuncios > Google Shopping do produto; (b) a campanha do Google Ads (flexi_ads_*) e a midia que divulga os itens que JA estao no Merchant Center. Para um produto aparecer no Google, primeiro ele precisa de anuncio publicado no Google Shopping; so depois uma campanha (Shopping ou Performance Max) o divulga. 'Criar anuncio no Google Shopping' nao e 'criar campanha no Google Ads'.\n\
- Anuncio no Google Shopping: liste os existentes com flexi_anuncios (marketplace 2); a conta vem de flexi_google_merchant_contas (use a conta cujo id e o proprio customer_id, que e a conta Google Ads cadastrada no Flexi); crie com flexi_criar_anuncio e dados {produto_id, marketplace: 2, marketplace_conta_id: \"<id da conta, so digitos>\", quantidade_anuncios: 1} (o Google NAO passa por flexi_preparar_criacao); envie ao Merchant Center com flexi_publicar_anuncio (pede permissao). Produtos sem anuncio no Google Shopping sao os que nao aparecem em flexi_anuncios com marketplace 2 para aquele produto_id.\n\
- Campanhas do Google Ads: flexi_ads_dashboard, flexi_ads_produtos e flexi_ads_series mostram desempenho; flexi_ads_publicar_produtos cria uma campanha nova com o Merchant Center inteiro da conta (nao seleciona produtos por id).\n\
- Para ver uma tela do Flexi como o usuario ve (ex.: a aba Anuncios > Google Shopping de um produto, em /produtos/<id>), use dots_browser_open com a URL do Flexi: o navegador do DOT ja abre logado e o usuario acompanha no painel Computador. Acoes que so existem na tela (ex.: Excluir anuncio, no menu de um anuncio) voce faz clicando com dots_browser_click. Nunca use ferramentas de computador ou navegador do proprio assistente.";

pub const PRICE_RULES_SUMMARY: &str = "Regras de preco do Flexi: consulte flexi_precos_calculados antes de qualquer alteracao; nunca defina preco abaixo de preco_minimo; respeite taxas e frete calculados; alteracoes de preco em lote exigem aprovacao do usuario; registre o motivo de cada alteracao na resposta.";
