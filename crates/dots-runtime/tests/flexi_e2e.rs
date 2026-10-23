//! Fase Flexi de ponta a ponta: Flexi falso + dotsd + CLI falso que usa as ferramentas flexi_* pelo gateway.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use dots_core::*;
use dots_ipc::{IpcClient, RuntimeInfo, SocketPaths};
use serde_json::{Value, json};

struct Flexi {
    child: std::process::Child,
    url: String,
}
impl Flexi {
    async fn start(mfa: bool) -> Self {
        Self::start_at(mfa, if mfa { 50 } else { 0 }).await
    }
    /// Testes rodam em paralelo: cada um usa um deslocamento de porta proprio.
    async fn start_at(mfa: bool, offset: u16) -> Self {
        let port = 19000 + (std::process::id() % 900) as u16 + offset;
        let mut args = vec![concat!(env!("CARGO_MANIFEST_DIR"), "/../dots-flexi/tests/fixtures/fake_flexi.py").to_string(), port.to_string()];
        if mfa { args.push("--mfa".into()); }
        let child = Command::new("python3").arg("-I").args(&args).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap();
        for _ in 0..50 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Self { child, url: format!("http://127.0.0.1:{port}") }
    }
}
impl Drop for Flexi {
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

struct Rt { _dir: tempfile::TempDir, child: std::process::Child, client: IpcClient }
async fn start() -> Rt {
    let dir = tempfile::tempdir().unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_dotsd")).args(["--data-dir", dir.path().to_str().unwrap(), "--mode", "detached"]).env("DOTS_SECRETS", "memory").env("DOTS_LOG", "info").stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let start = Instant::now();
    loop {
        if let Ok(Some(info)) = RuntimeInfo::read(dir.path()) { if info.pid == child.id() && std::path::Path::new(&info.socket_path).exists() { tokio::time::sleep(Duration::from_millis(100)).await; break; } }
        assert!(start.elapsed() < Duration::from_secs(15));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let paths = SocketPaths::resolve(dir.path());
    let (client, _ev) = IpcClient::connect(&paths.socket_path, &paths.read_token().unwrap(), "flexi-e2e").await.unwrap();
    let rt = Rt { _dir: dir, child, client };
    let fake = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake_claude.py");
    rt.call("engines.set_override", json!({ "kind": "claude", "path": fake })).await;
    rt
}
impl Rt {
    async fn call(&self, m: &str, p: Value) -> Value { self.client.call(m, p).await.unwrap_or_else(|e| panic!("{m}: {e}")) }
    async fn try_call(&self, m: &str, p: Value) -> Result<Value, DotsError> { self.client.call(m, p).await }
    async fn dot(&self, name: &str, level: &str) -> Dot {
        let d: Dot = serde_json::from_value(self.call("dots.create", json!({ "name": name, "primary_engine": "claude" })).await).unwrap();
        self.call("policies.set", json!({ "dot_id": d.id, "patch": { "auto_approve_level": level } })).await;
        d
    }
    async fn run(&self, dot: &Dot, prompt: &str) -> Task {
        let t: Task = serde_json::from_value(self.call("tasks.create", json!({ "dot_id": dot.id, "prompt": prompt })).await).unwrap();
        let start = Instant::now();
        loop {
            let v = self.call("tasks.get", json!({ "id": t.id })).await;
            let t: Task = serde_json::from_value(v["task"].clone()).unwrap();
            if t.status.is_terminal() || t.status == TaskStatus::WaitingUser { return t; }
            if t.status == TaskStatus::WaitingApproval {
                let pend: Vec<Approval> = serde_json::from_value(self.call("approvals.list", json!({ "pending_only": true })).await).unwrap();
                if let Some(a) = pend.first() { self.call("approvals.decide", json!({ "id": a.id, "approved": true })).await; }
            }
            assert!(start.elapsed() < Duration::from_secs(40), "tarefa nao terminou: {t:?}");
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }
    fn stop(mut self) { unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) }; let _ = self.child.wait(); }
}

#[tokio::test]
async fn agent_account_is_separate_from_login_account() {
    let personal = Flexi::start_at(false, 120).await;
    let agent = Flexi::start_at(false, 125).await;
    let rt = start().await;
    let own: AppConnection = serde_json::from_value(rt.call("apps.flexi.connect", json!({ "base_url": personal.url, "email": "agente@teste.com", "senha": "segredo123" })).await).unwrap();
    let worker: AppConnection = serde_json::from_value(rt.call("apps.flexi.connect", json!({ "base_url": agent.url, "email": "agente@teste.com", "senha": "segredo123" })).await).unwrap();
    let dot = rt.dot("Revisor", "read").await;
    rt.call("dots.set_integration", json!({ "dot_id": dot.id, "kind": "flexi", "connection_id": own.id })).await;
    rt.call("settings.set", json!({ "key": "account.flexi_connection_id", "value": own.id })).await;

    // A conexao pessoal, mesmo vinculada anteriormente ao DOT, nao libera suas ferramentas.
    let tools = rt.run(&dot, "TOOLS").await;
    assert!(!tools.result_text.as_deref().unwrap_or_default().contains("flexi_anuncios"));

    rt.call("settings.set", json!({ "key": "agents.flexi_connection_id", "value": worker.id })).await;
    reqwest::get(format!("{}/__especialidades", agent.url)).await.unwrap();
    let templates: Vec<Value> = serde_json::from_value(rt.call("apps.flexi.templates", json!({})).await).unwrap();
    assert!(templates.iter().any(|t| t["id"] == "gerente_de_compras"), "especialidades devem vir da conta do agente");
    reqwest::get(format!("{}/__expire", personal.url)).await.unwrap();
    // A sessao pessoal foi invalidada: uma consulta bem-sucedida prova que o DOT usou a conta do agente.
    let result = rt.run(&dot, r#"FLEXI:flexi_anuncios:{"ids":[101]}"#).await;
    assert_eq!(result.status, TaskStatus::Completed, "{:?}", result.error);
    assert!(result.result_text.as_deref().unwrap_or_default().contains("Furadeira X"), "{:?}", result.result_text);

    rt.call("apps.flexi.remove", json!({ "id": worker.id })).await;
    assert!(rt.call("settings.get", json!({ "key": "agents.flexi_connection_id" })).await.is_null());
    let tools = rt.run(&dot, "TOOLS").await;
    assert!(!tools.result_text.as_deref().unwrap_or_default().contains("flexi_anuncios"));
    rt.stop();
}

#[tokio::test]
async fn connect_bind_and_use_flexi_tools_with_approval_and_session_renewal() {
    let flexi = Flexi::start(false).await;
    let rt = start().await;
    // senha errada -> UNAUTHORIZED e conexao fica needs_login
    let err = rt.try_call("apps.flexi.connect", json!({ "base_url": flexi.url, "email": "agente@teste.com", "senha": "errada", "server": "teste" })).await.unwrap_err();
    assert_eq!(err.code(), "UNAUTHORIZED");
    let conns: Vec<AppConnection> = serde_json::from_value(rt.call("apps.list", json!({})).await).unwrap();
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0].status, ConnectionStatus::NeedsLogin);
    // reconecta com a senha certa e lembra a senha
    let c: AppConnection = serde_json::from_value(rt.call("apps.flexi.reconnect", json!({ "id": conns[0].id, "senha": "segredo123", "remember_password": true })).await).unwrap();
    assert_eq!(c.status, ConnectionStatus::Connected);
    assert_eq!(c.user_name.as_deref(), Some("Agente Teste"));
    assert_eq!(c.empresa.as_deref(), Some("Empresa Teste"));
    assert!(c.remember_password);

    // Equipes: gerente delega aos funcionarios; equipes se encaixam; papel e politica ficam em sincronia
    let creator = rt.dot("Criador ML", "read").await;
    rt.call("apps.flexi.assign_role", json!({ "dot_id": creator.id, "connection_id": c.id, "group": "mercadolivre", "role": "worker" })).await;
    let integ: DotIntegration = serde_json::from_value(rt.call("dots.integration", json!({ "dot_id": creator.id, "kind": "flexi" })).await).unwrap();
    assert_eq!((integ.role_template.as_deref(), integ.work_group.as_deref()), (None, Some("mercadolivre")));
    let sub = rt.dot("Sub ML", "read").await;
    let ml: Team = serde_json::from_value(rt.call("teams.create", json!({ "name": "Mercado Livre", "manager_dot_id": sub.id, "member_ids": [creator.id] })).await).unwrap();
    let pol: ToolPolicy = serde_json::from_value(rt.call("policies.get", json!({ "dot_id": sub.id })).await).unwrap();
    assert_eq!(pol.can_delegate_to, vec![creator.id.clone()], "gerente delega aos funcionarios");
    let sub_dot = rt.call("dots.get", json!({ "id": sub.id })).await;
    assert_eq!(sub_dot["role"], "supervisor", "gerente vira supervisor");
    let boss = rt.dot("Chefe", "read").await;
    let mkt: Team = serde_json::from_value(rt.call("teams.create", json!({ "name": "Marketplaces", "manager_dot_id": boss.id })).await).unwrap();
    rt.call("teams.add_member", json!({ "id": mkt.id, "dot_id": sub.id })).await;
    let pol: ToolPolicy = serde_json::from_value(rt.call("policies.get", json!({ "dot_id": boss.id })).await).unwrap();
    assert_eq!(pol.can_delegate_to, vec![sub.id.clone()], "equipes encaixadas: chefe delega ao subgerente");
    let info: DotTeamInfo = serde_json::from_value(rt.call("dots.team", json!({ "dot_id": sub.id })).await).unwrap();
    assert_eq!((info.member_of.map(|t| t.name), info.manages.map(|t| t.name)), (Some("Marketplaces".into()), Some("Mercado Livre".into())));
    // ciclo recusado: o chefe nao pode virar funcionario do subgerente
    assert_eq!(rt.try_call("teams.add_member", json!({ "id": ml.id, "dot_id": boss.id })).await.unwrap_err().code(), "VALIDATION");
    // mover funcionario de equipe atualiza os dois gerentes
    rt.call("teams.add_member", json!({ "id": mkt.id, "dot_id": creator.id })).await;
    let pol: ToolPolicy = serde_json::from_value(rt.call("policies.get", json!({ "dot_id": sub.id })).await).unwrap();
    assert!(pol.can_delegate_to.is_empty());
    let pol: ToolPolicy = serde_json::from_value(rt.call("policies.get", json!({ "dot_id": boss.id })).await).unwrap();
    assert!(pol.can_delegate_to.contains(&creator.id));
    // tirar o gerente: volta a funcionario e nao delega
    rt.call("teams.set_manager", json!({ "id": ml.id, "dot_id": null })).await;
    let pol: ToolPolicy = serde_json::from_value(rt.call("policies.get", json!({ "dot_id": sub.id })).await).unwrap();
    assert!(pol.can_delegate_to.is_empty());
    assert_eq!(rt.call("dots.get", json!({ "id": sub.id })).await["role"], "worker");
    assert!(rt.try_call("apps.flexi.assign_role", json!({ "dot_id": creator.id, "connection_id": c.id, "template_id": "nao_existe" })).await.is_err());
    rt.call("teams.delete", json!({ "id": ml.id })).await;
    let teams: Vec<Team> = serde_json::from_value(rt.call("teams.list", json!({})).await).unwrap();
    assert_eq!(teams.len(), 1);

    // DOT sem vinculo explicito usa a unica conexao; ferramentas aparecem no tools/list
    let d = rt.dot("Revisor ML", "read").await;
    let t = rt.run(&d, "TOOLS").await;
    let names = t.result_text.clone().unwrap();
    assert!(names.contains("flexi_anuncios") && names.contains("dots_read_file"), "{names}");
    let t = rt.run(&d, r#"FLEXI:flexi_anuncios:{"ids":[101]}"#).await;
    assert_eq!(t.status, TaskStatus::Completed, "erro={:?} result={:?}", t.error, t.result_text);
    assert!(t.result_text.as_deref().unwrap().contains("Furadeira X"), "{:?}", t.result_text);
    // leitura de precos calculados (GET com query)
    let t = rt.run(&d, r#"FLEXI:flexi_precos_calculados:{"anuncio_id":101}"#).await;
    assert!(t.result_text.as_deref().unwrap().contains("preco_minimo"));
    let t = rt.run(&d, r#"FLEXI:flexi_historico_preco:{"anuncio_id":101}"#).await;
    assert!(t.result_text.as_deref().unwrap().contains("dados"));
    // escrita com politica "read" exige aprovacao (o helper aprova) e altera o preco
    let t = rt.run(&d, r#"FLEXI:flexi_atualizar_preco:{"id":101,"preco":189.9}"#).await;
    assert!(t.result_text.as_deref().unwrap().starts_with("OK"), "{:?}", t.result_text);
    assert!(t.result_text.as_deref().unwrap().contains("op-1"));
    let apr: Vec<Approval> = serde_json::from_value(rt.call("approvals.list", json!({})).await).unwrap();
    assert!(apr.iter().any(|a| a.tool == "flexi_atualizar_preco" && a.status == ApprovalStatus::Approved && a.risk == RiskLevel::Write));
    // recusa do Flexi (abaixo do minimo) vira erro de ferramenta, nao falha da tarefa
    let t = rt.run(&d, r#"FLEXI:flexi_atualizar_preco:{"id":101,"preco":10}"#).await;
    assert!(t.result_text.as_deref().unwrap().contains("Preco abaixo do minimo"));
    // publicar e critico: aprovacao com risco critical
    rt.run(&d, r#"FLEXI:flexi_publicar_anuncio:{"id":101}"#).await;
    let apr: Vec<Approval> = serde_json::from_value(rt.call("approvals.list", json!({})).await).unwrap();
    assert!(apr.iter().any(|a| a.tool == "flexi_publicar_anuncio" && a.risk == RiskLevel::Critical));
    // Google Ads: confirm=true injetado
    let t = rt.run(&d, r#"FLEXI:flexi_ads_pausar_campanha:{"id":"555","contaId":3,"campaignName":"Promo"}"#).await;
    assert!(t.result_text.as_deref().unwrap().contains("\"paused\": true"), "{:?}", t.result_text);
    // sessao expira no Flexi -> relogin automatico com a senha guardada
    let before: Value = reqwest::get(format!("{}/__logins", flexi.url)).await.unwrap().json().await.unwrap();
    reqwest::get(format!("{}/__expire", flexi.url)).await.unwrap();
    let t = rt.run(&d, r#"FLEXI:flexi_contas_mercadolivre:{}"#).await;
    assert!(t.result_text.as_deref().unwrap().contains("Loja ML"), "{:?}", t.result_text);
    let after: Value = reqwest::get(format!("{}/__logins", flexi.url)).await.unwrap().json().await.unwrap();
    assert_eq!(after["logins"].as_i64().unwrap(), before["logins"].as_i64().unwrap() + 1, "relogou uma vez");
    // auditoria e alteracoes registradas no DOTS
    let audit: Vec<AuditLog> = serde_json::from_value(rt.call("audit.list", json!({ "limit": 100 })).await).unwrap();
    assert!(audit.iter().any(|a| a.action == "tool.flexi_atualizar_preco" && a.details["outcome"] == "ok"));
    assert!(audit.iter().any(|a| a.action == "app.flexi.connect"));
    // remover a conexao: ferramentas somem e chamada e recusada
    rt.call("apps.flexi.remove", json!({ "id": c.id })).await;
    let t = rt.run(&d, r#"FLEXI:flexi_anuncios:{}"#).await;
    assert!(t.result_text.as_deref().unwrap().contains("nenhuma conexao Flexi"));
    rt.stop();
}

#[tokio::test]
async fn mfa_connect_and_team_creation() {
    let flexi = Flexi::start(true).await;
    let rt = start().await;
    let c: AppConnection = serde_json::from_value(rt.call("apps.flexi.connect", json!({ "base_url": flexi.url, "email": "agente@teste.com", "senha": "segredo123" })).await).unwrap();
    assert_eq!(c.status, ConnectionStatus::MfaPending);
    assert!(rt.try_call("apps.flexi.mfa", json!({ "id": c.id, "code": "000000" })).await.is_err());
    let c: AppConnection = serde_json::from_value(rt.call("apps.flexi.mfa", json!({ "id": c.id, "code": "123456" })).await).unwrap();
    assert_eq!(c.status, ConnectionStatus::Connected);
    assert_eq!(c.user_id, Some(7));
    // Flexi sem a rota de especialidades: os 3 gerentes embutidos, cada um com a equipe do seu setor (sem funcionarios:
    // o gerente contrata).
    let tpl: Vec<Value> = serde_json::from_value(rt.call("apps.flexi.templates", json!({})).await).unwrap();
    assert!(tpl.iter().all(|t| t["role"] == "supervisor"));
    let r = rt.call("apps.flexi.create_team", json!({ "id": c.id })).await;
    assert_eq!(r["created"].as_array().unwrap().len(), 3);
    let dots: Vec<Dot> = serde_json::from_value(rt.call("dots.list", json!({})).await).unwrap();
    let manager = dots.iter().find(|d| d.name == "SubGerente de Mercado Livre").unwrap();
    assert_eq!(manager.role, DotRole::Supervisor);
    let teams: Vec<Team> = serde_json::from_value(rt.call("teams.list", json!({})).await).unwrap();
    assert_eq!(teams.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["Google Ads", "Mercado Livre", "Shopee"]);
    let ml = teams.iter().find(|t| t.name == "Mercado Livre").unwrap();
    assert_eq!(ml.manager_dot_id.as_deref(), Some(manager.id.as_str()));
    assert!(ml.member_ids.is_empty());
    let integ: Option<DotIntegration> = serde_json::from_value(rt.call("dots.integration", json!({ "dot_id": manager.id, "kind": "flexi" })).await).unwrap();
    let integ = integ.unwrap();
    assert_eq!(integ.connection_id, c.id);
    assert_eq!(integ.work_group.as_deref(), Some("mercadolivre"));
    rt.stop();
}

#[tokio::test]
async fn templates_come_from_flexi_especialidades() {
    let flexi = Flexi::start_at(false, 75).await;
    let rt = start().await;
    let c: AppConnection = serde_json::from_value(rt.call("apps.flexi.connect", json!({ "base_url": flexi.url, "email": "agente@teste.com", "senha": "segredo123" })).await).unwrap();
    assert_eq!(c.status, ConnectionStatus::Connected);
    // Flexi sem a rota (versao antiga): especialidades embutidas.
    let tpl: Vec<Value> = serde_json::from_value(rt.call("apps.flexi.templates", json!({})).await).unwrap();
    assert_eq!(tpl.len(), 3);
    // Com a rota: a lista do Flexi substitui os embutidos; setor fora da lista e ignorado.
    reqwest::get(format!("{}/__especialidades", flexi.url)).await.unwrap();
    let tpl: Vec<Value> = serde_json::from_value(rt.call("apps.flexi.templates", json!({})).await).unwrap();
    assert_eq!(tpl.iter().map(|t| t["id"].as_str().unwrap()).collect::<Vec<_>>(), vec!["ml_manager", "gerente_de_compras"]);
    assert!(tpl.iter().all(|t| t["role"] == "supervisor"));
    assert_eq!(tpl[1]["group"], "compras");
    assert_eq!(tpl[1]["group_name"], "Compras");
    assert_eq!(tpl[1]["manages"], "Compras");
    assert_eq!(tpl[1]["trust"], "write");
    // Novo gerente com uma especialidade criada no Flexi: setor e cargo vem do Flexi.
    let d = rt.dot("Ana", "read").await;
    rt.call("apps.flexi.assign_role", json!({ "dot_id": d.id, "connection_id": c.id, "template_id": "gerente_de_compras" })).await;
    let integ: Option<DotIntegration> = serde_json::from_value(rt.call("dots.integration", json!({ "dot_id": d.id, "kind": "flexi" })).await).unwrap();
    let integ = integ.unwrap();
    assert_eq!(integ.role_template.as_deref(), Some("gerente_de_compras"));
    assert_eq!(integ.work_group.as_deref(), Some("compras"));
    let dots: Vec<Dot> = serde_json::from_value(rt.call("dots.list", json!({})).await).unwrap();
    assert_eq!(dots.iter().find(|x| x.id == d.id).unwrap().role, DotRole::Supervisor);
    let contacts: Vec<Value> = serde_json::from_value(rt.call("contacts.list", json!({})).await).unwrap();
    let ana = contacts.iter().find(|x| x["dot"]["id"] == json!(d.id)).unwrap();
    assert_eq!(ana["title"], "Gerente de Compras");
    // Funcionario criado pelo usuario: sem especialidade e sem setor.
    let w = rt.dot("Bia", "read").await;
    rt.call("apps.flexi.assign_role", json!({ "dot_id": w.id, "connection_id": c.id, "role": "worker" })).await;
    let integ: Option<DotIntegration> = serde_json::from_value(rt.call("dots.integration", json!({ "dot_id": w.id, "kind": "flexi" })).await).unwrap();
    assert_eq!(integ.unwrap().work_group, None);
    assert!(rt.try_call("apps.flexi.assign_role", json!({ "dot_id": d.id, "connection_id": c.id, "template_id": "mkt_manager" })).await.is_err(), "especialidade com setor fora da lista");
    rt.stop();
}

#[tokio::test]
async fn browser_login_creates_connection_and_reuses_it_on_relogin() {
    let flexi = Flexi::start_at(false, 25).await;
    let rt = start().await;
    let ch = rt.call("apps.flexi.browser_start", json!({ "base_url": flexi.url })).await;
    assert_eq!(ch["code"], "ABC-123");
    assert_eq!(ch["authorize_url"], format!("{}/login-external-app?token=tok-1", flexi.url));
    assert_eq!(rt.call("apps.flexi.browser_poll", json!({ "token": "tok-1" })).await["status"], "pending");
    reqwest::get(format!("{}/__approve", flexi.url)).await.unwrap();
    let done = rt.call("apps.flexi.browser_poll", json!({ "token": "tok-1" })).await;
    assert_eq!(done["status"], "authenticated");
    let c: AppConnection = serde_json::from_value(done["connection"].clone()).unwrap();
    assert_eq!(c.status, ConnectionStatus::Connected);
    assert_eq!(c.email, "agente@teste.com");
    assert_eq!(c.user_name.as_deref(), Some("Agente Teste"));
    assert!(!c.remember_password);
    // token ja usado: o runtime nao acompanha mais
    assert_eq!(rt.call("apps.flexi.browser_poll", json!({ "token": "tok-1" })).await["status"], "not_found");
    // foto de perfil vira data URL com o tipo descoberto pela assinatura (o servidor manda octet-stream)
    let photo = rt.call("apps.flexi.photo", json!({ "id": c.id })).await;
    assert!(photo["data_url"].as_str().unwrap().starts_with("data:image/png;base64,"));
    // sessao expirou: logar de novo pelo navegador atualiza a mesma conexao
    reqwest::get(format!("{}/__expire", flexi.url)).await.unwrap();
    rt.call("apps.flexi.browser_start", json!({ "base_url": flexi.url })).await;
    reqwest::get(format!("{}/__approve", flexi.url)).await.unwrap();
    let again = rt.call("apps.flexi.browser_poll", json!({ "token": "tok-1" })).await;
    assert_eq!(again["connection"]["id"], c.id);
    let conns: Vec<AppConnection> = serde_json::from_value(rt.call("apps.list", json!({})).await).unwrap();
    assert_eq!(conns.len(), 1);
    // recusado no navegador: nada muda
    rt.call("apps.flexi.browser_start", json!({ "base_url": flexi.url })).await;
    reqwest::get(format!("{}/__deny", flexi.url)).await.unwrap();
    assert_eq!(rt.call("apps.flexi.browser_poll", json!({ "token": "tok-1" })).await["status"], "denied");
    rt.stop();
}

#[tokio::test]
async fn link_reconnect_is_accepted_without_an_account() {
    // Sem conta ligada nao ha sincronizacao; o pedido e aceito, fica na auditoria e devolve o estado atual.
    let rt = start().await;
    let before = rt.call("apps.flexi.link_status", json!({})).await;
    assert_eq!(before["connected"], json!(false));
    let after = rt.call("apps.flexi.link_reconnect", json!({})).await;
    assert_eq!(after["connected"], json!(false));
    let audit = rt.call("audit.list", json!({ "limit": 50 })).await;
    assert!(audit.as_array().unwrap().iter().any(|a| a["action"] == "apps.flexi.link_reconnect"), "{audit}");
    rt.stop();
}
