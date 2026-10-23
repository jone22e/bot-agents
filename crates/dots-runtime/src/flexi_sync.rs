//! Sincronizacao entre o `dotsd` e o Flexi web por REST (ADR 0016; substitui o canal WebSocket do ADR 0015).
//!
//! O runtime so faz chamadas de saida, com a sessao da conta do usuario (`account.flexi_connection_id`):
//! registra os DOTs compartilhados, busca a cada poucos segundos as mensagens e decisoes pendentes, responde
//! pela conversa principal de cada DOT (a mesma que o app mostra; no Flexi e de grupo), espelha essa conversa
//! inteira no Flexi e devolve respostas, aprovacoes, agendamentos e metas; aplica os comandos que o dono deu
//! no Flexi web (modelo, pausa, agendamentos). Nada entra na maquina: funciona num computador Ubuntu ligado
//! 24 h sem porta aberta.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use dots_core::{AppConnection, Contact, Dot, DotStatus, DotsError, DotsResult, EngineConfiguration, Message, MessageRole, Schedule, TaskStatus, Team, Timestamp};
use dots_flexi::{FlexiClient, FlexiError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::flexi::ACCOUNT_CONNECTION_SETTING;
use crate::service::RuntimeService;

/// Prefixo das rotas que o Flexi expoe ao app (autenticadas pelo cookie da conta do usuario).
pub const API_PREFIX: &str = "/api/agentes-dots/app";
const POLL_INTERVAL: Duration = Duration::from_secs(4);
const MIN_CYCLE_GAP: Duration = Duration::from_millis(700);
const IDLE_RETRY: Duration = Duration::from_secs(30);
const BACKOFF_MIN: u64 = 5;
const BACKOFF_MAX: u64 = 60;
const APPROVALS_LIMIT: u32 = 100;
const CONVERSATION_SCAN: u32 = 200;
/// Quantas mensagens recentes da conversa principal vao ao Flexi na primeira vez que um DOT e espelhado.
const MIRROR_BACKFILL: usize = 50;
/// Cursor do espelho por DOT (`created_at` da ultima mensagem examinada), guardado para nao reler tudo a cada reinicio.
const MIRROR_CURSORS_SETTING: &str = "flexi_sync.mirror_cursors";
/// Versao do formato do espelho. Quando muda, os cursores zeram uma vez e as `MIRROR_BACKFILL` mensagens mais
/// recentes de cada DOT sao reenviadas (o Flexi faz upsert e corrige papel, autor, origem e data).
/// 2 (2026-10-23): papel, autor, origem, detalhe e data original das mensagens; estado da resposta corrigido.
const MIRROR_VERSION: u32 = 2;
const MIRROR_VERSION_SETTING: &str = "flexi_sync.mirror_version";
/// Agendamentos (`dots_schedule_*` e perfil do DOT) e metas (`dots_goal_*`): vao ao Flexi quando mudam.
const AUTOMATION_EVENT: &str = "automation.changed";
const GOAL_EVENT: &str = crate::goals::GOAL_EVENT;
/// Evento interno (IPC `apps.flexi.link_reconnect`): sincroniza na hora.
pub const RECONNECT_EVENT: &str = "flexi_link.reconnect";
const REGISTRY_EVENTS: [&str; 2] = ["dots.changed", "teams.changed"];
/// Ordem das equipes escolhida pelo usuario (mesma chave da lista de contatos do app): muda o agrupamento no Flexi.
const TEAMS_ORDER_SETTING: &str = "ui.teams_order";
const APPROVAL_EVENTS: [&str; 2] = ["approval.requested", "approval.resolved"];
/// Avisos `dots_notify_user` ("X precisa de voce"): vao ao Flexi quando mudam.
const NOTIFICATION_EVENT: &str = "notification.changed";
const NOTIFICATIONS_LIMIT: u32 = 50;

/// Estado da sincronizacao, em memoria, para a interface do app (`apps.flexi.link_status`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LinkStatus {
    pub connected: bool,
    pub since: Option<Timestamp>,
    pub last_sync: Option<Timestamp>,
    pub last_error: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Default)]
pub struct LinkState {
    inner: std::sync::Mutex<LinkStatus>,
}

impl LinkState {
    pub fn status(&self) -> LinkStatus {
        self.inner.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// Mensagem que um usuario do Flexi deixou para um DOT compartilhado.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct InboxMessage {
    pub id: i64,
    pub agente_id: String,
    #[serde(default)]
    pub usuario_id: Option<i64>,
    #[serde(default)]
    pub autor_nome: String,
    #[serde(default)]
    pub conteudo: String,
    #[serde(default)]
    pub imagem: Option<InboxImage>,
    #[serde(default)]
    pub estado: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct InboxImage {
    pub nome: String,
    pub mime: String,
    pub dados: String,
}

/// Decisao de aprovacao tomada no Flexi web, ainda nao aplicada aqui.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Decision {
    pub id: String,
    pub aprovado: bool,
    #[serde(default)]
    pub motivo: Option<String>,
}

/// Comando que o dono deu no Flexi web (menu do agente), ainda nao aplicado aqui: `definir_modelo`
/// (`dados.motor`, `dados.modelo`), `pausar`, `retomar`, `agendamento_ativar` (`dados.id`, `dados.ativo`) e
/// `agendamento_cancelar` (`dados.id`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Command {
    pub id: i64,
    pub agente_id: String,
    pub tipo: String,
    #[serde(default)]
    pub dados: Value,
}

#[derive(Debug, Default, Deserialize)]
struct SyncResponse {
    #[serde(default)]
    mensagens: Vec<InboxMessage>,
    #[serde(default)]
    decisoes: Vec<Decision>,
    /// Avisos que o dono dispensou no Flexi web e o app ainda nao marcou como lidos.
    #[serde(default)]
    dispensadas: Vec<String>,
    /// O Flexi nao tem o registro desta versao (ex.: backend atualizado depois do nosso registro): manda tudo de novo.
    #[serde(default)]
    reenviar_registro: bool,
    /// Comandos do dono dados no Flexi web, ainda nao aplicados.
    #[serde(default)]
    comandos: Vec<Command>,
}

/// Mensagem em andamento: a tarefa que responde a mensagem `id` do Flexi.
#[derive(Debug, Clone)]
struct InFlight {
    task_id: String,
    conversation_id: String,
    agente_id: String,
    /// Ultimo (conteudo, estado, situacao) enviado ao Flexi, para nao repetir.
    last_pushed: Option<(String, String, String)>,
}

/// Espelho da conversa principal de um DOT no Flexi (a mesma conversa que o app mostra).
#[derive(Debug, Clone, Default)]
struct Mirror {
    /// `created_at` da ultima mensagem examinada; as anteriores nao voltam a ser lidas.
    cursor: Option<Timestamp>,
    /// Mensagens enviadas ainda em andamento (resposta do DOT em streaming): ultimo (conteudo, estado, situacao).
    open: HashMap<String, (String, String, String)>,
}

#[derive(Default)]
struct SyncState {
    started: bool,
    registry_dirty: bool,
    approvals_dirty: bool,
    notifications_dirty: bool,
    /// Agendamentos mudaram (`automation.changed`): a lista inteira vai na proxima rodada.
    schedules_dirty: bool,
    /// Metas mudaram (`goal.changed`): a lista inteira vai na proxima rodada.
    goals_dirty: bool,
    /// Ultimo JSON de `estados` aceito pelo Flexi: so manda de novo quando algo mudou.
    last_states: Option<String>,
    inflight: HashMap<i64, InFlight>,
    /// Espelho por DOT; `mirror_all` reexamina todos (inicio, registro, conversa nova), `mirror_dirty` so os avisados por evento.
    mirror: HashMap<String, Mirror>,
    mirror_all: bool,
    mirror_dirty: HashSet<String>,
    /// Conversa principal -> DOT, para `message.changed` acordar o espelho certo.
    main_conversations: HashMap<String, String>,
}

enum SyncError {
    Unauthorized(String),
    Failed(String),
}

impl From<FlexiError> for SyncError {
    fn from(e: FlexiError) -> Self {
        match e {
            FlexiError::Unauthenticated(m) => SyncError::Unauthorized(m),
            other => SyncError::Failed(other.to_string()),
        }
    }
}

impl From<DotsError> for SyncError {
    fn from(e: DotsError) -> Self {
        SyncError::Failed(e.to_string())
    }
}

/// Texto que o DOT recebe: quem falou e o que disse, para a conversa de grupo fazer sentido.
pub fn prompt_for(msg: &InboxMessage) -> String {
    let autor = msg.autor_nome.trim();
    let autor = if autor.is_empty() { "Usuario do Flexi" } else { autor };
    let conteudo = msg.conteudo.trim();
    if conteudo.is_empty() && msg.imagem.is_some() {
        format!("{autor} (pelo Flexi): imagem anexada")
    } else {
        format!("{autor} (pelo Flexi): {conteudo}")
    }
}

/// Usuarios do Flexi que veem o DOT: os dele, com "Compartilhar no Flexi" ligado; senao, os do gerente da equipe
/// dele (compartilhar o gerente compartilha os funcionarios), subindo pelas equipes ate achar um gerente compartilhado.
pub fn shared_users(dot: &Dot, dots: &[Dot], teams: &[Team]) -> Vec<i64> {
    let mut current = dot;
    for _ in 0..8 {
        if current.flexi_share.enabled {
            return current.flexi_share.users.iter().map(|u| u.id).collect();
        }
        let manager = teams
            .iter()
            .find(|t| t.member_ids.iter().any(|id| id == &current.id))
            .and_then(|t| t.manager_dot_id.as_deref())
            .and_then(|m| dots.iter().find(|d| d.id == m));
        match manager {
            Some(m) if m.id != current.id => current = m,
            _ => return vec![],
        }
    }
    vec![]
}

/// O que o Flexi guarda de cada DOT: o dono ve todos os dele; `usuarios` sao os outros que tambem veem.
/// `motor` e `modelo` sao o assistente e o modelo escolhidos para o DOT (vazio = padrao do motor).
pub fn agent_payload(dot: &Dot, usuarios: Vec<i64>) -> Value {
    json!({
        "id": dot.id,
        "nome": dot.name,
        "descricao": dot.description,
        "status": dot.status.as_str(),
        "usuarios": usuarios,
        "motor": dot.primary_engine.map(|e| e.as_str()),
        "modelo": dot.model,
    })
}

/// Motores configurados no app com o catalogo de modelos detectado, para o Flexi web oferecer "Definir modelo da IA".
pub fn engines_payload(engines: &[EngineConfiguration]) -> Vec<Value> {
    engines
        .iter()
        .map(|e| {
            json!({
                "id": e.kind.as_str(),
                "nome": e.kind.display_name(),
                "ativo": e.enabled && e.last_detection.as_ref().is_some_and(|d| d.installed),
                "modelo_padrao": e.model,
                "modelos": e.last_detection.as_ref().map(|d| d.models.iter().map(|m| json!({ "id": m.id, "nome": m.label })).collect::<Vec<_>>()).unwrap_or_default(),
            })
        })
        .collect()
}

/// Agendamento como o Flexi web o lista: texto legivel (`describe`), pedido, ativo e proximo/ultimo disparo.
pub fn schedule_payload(s: &Schedule) -> Value {
    json!({
        "id": s.id,
        "agente_id": s.dot_id,
        "tipo": s.kind.as_str(),
        "spec": s.spec,
        "texto": crate::schedules::describe(s),
        "pedido": s.prompt,
        "ativo": s.enabled,
        "proximo_em": s.next_run_at,
        "ultimo_em": s.last_run_at,
        "criado_em": s.created_at,
    })
}

/// Estado de um DOT compartilhado como a lista de contatos do app o mostra (`contacts.list`), para o Flexi
/// desenhar a mesma linha: situacao (idle | busy | attention), texto, cargo, batimento, concluido e pedidos pendentes.
pub fn state_payload(c: &Contact) -> Value {
    json!({
        "id": c.dot.id,
        "status": c.dot.status.as_str(),
        "situacao": c.state,
        "texto": c.state_text,
        "cargo": c.title,
        "pulso": c.pulse,
        "concluido": c.done,
        "pendentes": c.pending_requests,
        "ultima_atividade_em": c.last_at,
    })
}

/// Equipes na ordem escolhida pelo usuario (`ui.teams_order`), como a lista de contatos do app agrupa os DOTs.
/// `pai` e a equipe da qual o gerente desta e funcionario (subtitulo "em X" no app).
pub fn teams_payload(teams: &[Team], order: &[String]) -> Vec<Value> {
    let rank: HashMap<&str, usize> = order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    let mut sorted: Vec<(usize, &Team)> = teams.iter().enumerate().map(|(i, t)| (rank.get(t.id.as_str()).copied().unwrap_or(order.len() + i), t)).collect();
    sorted.sort_by_key(|(k, _)| *k);
    sorted
        .into_iter()
        .map(|(_, t)| {
            let parent = t.manager_dot_id.as_deref().and_then(|m| teams.iter().find(|p| p.member_ids.iter().any(|id| id == m)));
            json!({
                "id": t.id,
                "nome": t.name,
                "gerente_id": t.manager_dot_id,
                "membros": t.member_ids,
                "pai": parent.map(|p| p.name.clone()),
            })
        })
        .collect()
}

/// Estado da resposta do DOT a partir dos metadados da mensagem dele: `(estado, terminou)`.
pub fn reply_state(metadata: &Value) -> (&'static str, bool) {
    if metadata.get("streaming").and_then(Value::as_bool).unwrap_or(false) {
        return ("processando", false);
    }
    // O executor grava `ExecutionStatus::as_str()` ("COMPLETED", "FAILED"...): compara sem diferenciar maiusculas.
    match metadata.get("status").and_then(Value::as_str) {
        Some(s) if s.eq_ignore_ascii_case("completed") => ("concluida", true),
        Some(_) => ("erro", true),
        // Mensagem sem metadados de execucao (motor ainda nao comecou): continua em andamento.
        None => ("processando", false),
    }
}

/// Nomes das ferramentas usadas, guardados pelo executor em `metadata.tools`.
pub fn reply_actions(metadata: &Value) -> Vec<String> {
    metadata.get("tools").and_then(Value::as_array).map(|t| t.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Id da mensagem do Flexi guardado nos metadados da mensagem do usuario na conversa do DOT.
pub fn flexi_message_id(metadata: &Value) -> Option<i64> {
    metadata.get("flexi").and_then(|f| f.get("mensagem_id")).and_then(Value::as_i64)
}

/// Texto da resposta do DOT: a mensagem, ou o erro/aviso quando terminou vazia.
pub fn reply_content(m: &Message, done: bool) -> String {
    if m.content.is_empty() && done {
        m.metadata.get("error").and_then(Value::as_str).unwrap_or("A tarefa terminou sem resposta.").to_string()
    } else {
        m.content.clone()
    }
}

/// Como uma mensagem da conversa principal vai ao Flexi.
#[derive(Debug, Clone, PartialEq)]
pub struct MirrorPayload {
    pub papel: &'static str,
    pub conteudo: String,
    pub estado: &'static str,
    pub done: bool,
    pub acoes: Vec<String>,
    /// Origem de uma nota de sistema (`metadata.origin` sem o id): `schedule`, `event`, `delegation`, `followup`,
    /// `autonomy`, `goal`...; vazio nas demais.
    pub origem: String,
    /// Complemento da nota (`metadata.prompt`: o pedido agendado, a meta...); vazio nas demais.
    pub detalhe: String,
}

/// Origem de uma nota de sistema sem o id (`schedule:abc` -> `schedule`).
pub fn note_origin(metadata: &Value) -> String {
    metadata.get("origin").and_then(Value::as_str).map(|o| o.split(':').next().unwrap_or("").trim().chars().take(40).collect()).unwrap_or_default()
}

/// `None` para o que nao e espelhado: chamadas de ferramenta e mensagens que ja nasceram no Flexi.
pub fn mirror_payload(m: &Message) -> Option<MirrorPayload> {
    let plain = |papel: &'static str| MirrorPayload { papel, conteudo: m.content.clone(), estado: "concluida", done: true, acoes: vec![], origem: String::new(), detalhe: String::new() };
    match m.role {
        MessageRole::Tool => None,
        MessageRole::User if flexi_message_id(&m.metadata).is_some() => None,
        MessageRole::User => Some(plain("usuario")),
        MessageRole::System => Some(MirrorPayload {
            origem: note_origin(&m.metadata),
            detalhe: m.metadata.get("prompt").and_then(Value::as_str).map(|p| p.chars().take(4000).collect()).unwrap_or_default(),
            ..plain("sistema")
        }),
        MessageRole::Dot => {
            let (estado, done) = reply_state(&m.metadata);
            Some(MirrorPayload { papel: "agente", conteudo: reply_content(m, done), estado, done, acoes: reply_actions(&m.metadata), origem: String::new(), detalhe: String::new() })
        }
    }
}

fn task_is_terminal(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
}

fn path(suffix: &str) -> String {
    format!("{API_PREFIX}{suffix}")
}

impl RuntimeService {
    fn set_link(&self, connected: bool, base_url: Option<String>, error: Option<String>) {
        let now = dots_core::now();
        let changed = {
            let mut cur = match self.flexi_link.inner.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let changed = cur.connected != connected || cur.last_error != error;
            if changed {
                cur.connected = connected;
                cur.since = if connected { Some(now) } else { None };
                cur.last_error = error;
            }
            cur.base_url = base_url.or(cur.base_url.take());
            if connected {
                cur.last_sync = Some(now);
            }
            changed
        };
        if changed {
            self.events.emit("flexi_link.changed", json!(self.flexi_link.status()));
        }
    }

    /// Contadores que o Flexi mostra no icone: aprovacoes pendentes (permissoes + perguntas) e DOTs ativos.
    async fn flexi_sync_counts(&self) -> DotsResult<(u32, u32)> {
        self.storage
            .call(|s| {
                let pending = s.approvals().pending_count()? + s.approvals().pending_questions_count()?;
                let dots = s.dots().list(false)?.len() as u32;
                Ok((pending, dots))
            })
            .await
    }

    /// Conta do usuario (ligada pelo "Logar com Flexi") com sessao guardada.
    async fn flexi_sync_account(&self) -> DotsResult<Option<(AppConnection, FlexiClient)>> {
        let conn = self
            .storage
            .call(|s| {
                let Some(setting) = s.settings().get(ACCOUNT_CONNECTION_SETTING)? else { return Ok(None) };
                let Some(id) = setting.value.as_str().map(String::from) else { return Ok(None) };
                Ok(s.apps().get(&id).ok())
            })
            .await?;
        let Some(conn) = conn else { return Ok(None) };
        let client = self.flexi_client(&conn).await?;
        Ok(if client.session_cookie().is_some() { Some((conn, client)) } else { None })
    }

    /// DOTs que o Flexi conhece: todos os nao arquivados (o dono ve todos; os outros usuarios, so os compartilhados).
    async fn flexi_shared_dots(&self) -> DotsResult<Vec<Dot>> {
        Ok(self.storage.call(|s| s.dots().list(false)).await?.into_iter().filter(|d| d.status != DotStatus::Archived).collect())
    }

    /// Estado atual dos DOTs compartilhados, na mesma forma da lista de contatos do app.
    async fn flexi_shared_states(&self) -> DotsResult<Vec<Value>> {
        let contacts: Vec<Contact> = serde_json::from_value(self.dispatch("contacts.list", json!({})).await?).map_err(|e| DotsError::Internal(format!("contatos invalidos: {e}")))?;
        Ok(contacts.iter().filter(|c| c.dot.status != DotStatus::Archived).map(state_payload).collect())
    }

    /// Equipes do app, na ordem escolhida pelo usuario, para o Flexi agrupar os DOTs como a lista de contatos.
    async fn flexi_teams(&self) -> DotsResult<Vec<Value>> {
        self.storage
            .call(|s| {
                let teams = s.teams().list()?;
                let order: Vec<String> = s
                    .settings()
                    .get(TEAMS_ORDER_SETTING)?
                    .and_then(|st| st.value.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()))
                    .unwrap_or_default();
                Ok(teams_payload(&teams, &order))
            })
            .await
    }

    /// Conversa do DOT com o Flexi: a principal, a mesma que o app mostra (cria se o DOT ainda nao tem nenhuma).
    async fn flexi_conversation_for(&self, dot_id: &str) -> DotsResult<String> {
        let d = dot_id.to_string();
        self.storage.call(move |s| s.conversations().main(&d).map(|c| c.id)).await
    }

    /// Cursores do espelho da conversa guardados entre reinicios. Se o formato do espelho mudou
    /// (`MIRROR_VERSION`), zera os cursores uma vez: as mensagens recentes vao de novo e o Flexi as corrige.
    async fn mirror_cursors(&self) -> HashMap<String, Timestamp> {
        let (cursors, version) = self
            .storage
            .call(|s| {
                let cursors: HashMap<String, Timestamp> = s.settings().get(MIRROR_CURSORS_SETTING)?.and_then(|st| serde_json::from_value(st.value).ok()).unwrap_or_default();
                let version = s.settings().get(MIRROR_VERSION_SETTING)?.and_then(|st| st.value.as_u64()).unwrap_or(1) as u32;
                Ok((cursors, version))
            })
            .await
            .unwrap_or_default();
        if version == MIRROR_VERSION {
            return cursors;
        }
        tracing::info!(de = version, para = MIRROR_VERSION, "formato do espelho da conversa mudou; reenviando as mensagens recentes de cada DOT ao Flexi");
        self.save_mirror_cursors(HashMap::new()).await;
        HashMap::new()
    }

    async fn save_mirror_cursors(&self, cursors: HashMap<String, Timestamp>) {
        let r = self
            .storage
            .call(move |s| {
                s.settings().set(MIRROR_CURSORS_SETTING, &json!(cursors))?;
                s.settings().set(MIRROR_VERSION_SETTING, &json!(MIRROR_VERSION)).map(|_| ())
            })
            .await;
        if let Err(e) = r {
            tracing::warn!(error = %e, "cursores do espelho da conversa nao foram guardados");
        }
    }

    /// Usuarios da base da conta do usuario, para escolher com quem compartilhar um DOT (IPC `apps.flexi.users`).
    pub async fn flexi_users(&self) -> DotsResult<Vec<Value>> {
        let Some((_, client)) = self.flexi_sync_account().await? else {
            return Err(DotsError::Validation("entre com sua conta do Flexi (Logar com Flexi) para escolher usuarios".into()));
        };
        let v = client
            .call("GET", "/api/usuarios", Some(&json!({ "page": 1, "pageSize": 500, "sortBy": "nome", "sortDir": "asc" })), None)
            .await
            .map_err(|e| DotsError::Validation(e.to_string()))?;
        Ok(v.get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|u| {
                        let id = u.get("id").and_then(Value::as_i64)?;
                        Some(json!({ "id": id, "nome": u.get("nome").and_then(Value::as_str).unwrap_or(""), "email": u.get("email").and_then(Value::as_str).unwrap_or("") }))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Aplica uma decisao vinda do Flexi web com a mesma politica e auditoria das decisoes no app.
    async fn apply_flexi_decision(&self, d: &Decision) -> DotsResult<dots_core::Approval> {
        let reason = d.motivo.clone().filter(|m| !m.trim().is_empty());
        let approval = self.decide_approval_by(&d.id, d.aprovado, reason, "user", "o usuario (pelo Flexi)").await?;
        let (aid, kind, approved) = (d.id.clone(), approval.kind.as_str().to_string(), d.aprovado);
        let _ = self.storage.call(move |s| s.audit().record("user", "approval.decide_via_flexi_web", "approval", Some(&aid), json!({ "approved": approved, "kind": kind })).map(|_| ())).await;
        Ok(approval)
    }

    /// Aplica um comando do Flexi web pelos mesmos IPCs do app (`dots.update`, `dots.pause`, `schedules.*`),
    /// com auditoria. Devolve o que mudou, para o Flexi registrar.
    async fn apply_flexi_command(&self, c: &Command) -> DotsResult<Value> {
        let dot = self.storage.call({ let id = c.agente_id.clone(); move |s| s.dots().get(&id) }).await?;
        if dot.status == DotStatus::Archived {
            return Err(DotsError::Validation("este agente foi arquivado no app".into()));
        }
        let text = |k: &str| c.dados.get(k).and_then(Value::as_str).map(|v| v.trim().to_string()).unwrap_or_default();
        let result = match c.tipo.as_str() {
            "definir_modelo" => {
                let mut patch = serde_json::Map::new();
                let motor = text("motor");
                if !motor.is_empty() {
                    patch.insert("primary_engine".into(), json!(dots_core::EngineKind::parse(&motor)?.as_str()));
                }
                let modelo = text("modelo");
                patch.insert("model".into(), if modelo.is_empty() { Value::Null } else { json!(modelo) });
                let updated: Dot = serde_json::from_value(self.dispatch("dots.update", json!({ "id": c.agente_id, "patch": Value::Object(patch) })).await?).map_err(|e| DotsError::Internal(e.to_string()))?;
                json!({ "motor": updated.primary_engine.map(|e| e.as_str()), "modelo": updated.model })
            }
            "pausar" | "retomar" => {
                let method = if c.tipo == "pausar" { "dots.pause" } else { "dots.resume" };
                let updated: Dot = serde_json::from_value(self.dispatch(method, json!({ "id": c.agente_id })).await?).map_err(|e| DotsError::Internal(e.to_string()))?;
                json!({ "status": updated.status.as_str() })
            }
            "agendamento_ativar" | "agendamento_cancelar" => {
                let sid = text("id");
                let sch = self.storage.call({ let sid = sid.clone(); move |s| s.schedules().get(&sid) }).await?;
                if sch.dot_id != c.agente_id {
                    return Err(DotsError::Validation("este agendamento nao e deste agente".into()));
                }
                if c.tipo == "agendamento_cancelar" {
                    self.dispatch("schedules.delete", json!({ "id": sid })).await?;
                    json!({ "agendamento": sid, "removido": true })
                } else {
                    let ativo = c.dados.get("ativo").and_then(Value::as_bool).unwrap_or(true);
                    self.dispatch("schedules.update", json!({ "id": sid, "patch": { "enabled": ativo } })).await?;
                    json!({ "agendamento": sid, "ativo": ativo })
                }
            }
            other => return Err(DotsError::Validation(format!("comando desconhecido: {other}"))),
        };
        let (aid, tipo, dados, res) = (c.agente_id.clone(), c.tipo.clone(), c.dados.clone(), result.clone());
        let _ = self.storage.call(move |s| s.audit().record("user", "flexi_web.command", "dot", Some(&aid), json!({ "comando": tipo, "dados": dados, "resultado": res })).map(|_| ())).await;
        Ok(result)
    }
}

async fn register_agents(svc: &RuntimeService, client: &FlexiClient) -> Result<(), SyncError> {
    let dots = svc.flexi_shared_dots().await?;
    let teams = svc.storage.call(|s| s.teams().list()).await?;
    let equipes = svc.flexi_teams().await?;
    let engines = svc.storage.call(|s| s.engines().list()).await?;
    let agentes = dots.iter().map(|d| agent_payload(d, shared_users(d, &dots, &teams))).collect::<Vec<_>>();
    let body = json!({ "versao": crate::VERSION, "agentes": agentes, "equipes": equipes, "motores": engines_payload(&engines) });
    client.call("PUT", &path("/agentes"), None, Some(&body)).await?;
    tracing::debug!(agentes = dots.len(), equipes = equipes.len(), "DOTs compartilhados registrados no Flexi");
    Ok(())
}

async fn start_message(svc: &RuntimeService, client: &FlexiClient, st: &mut SyncState, msg: &InboxMessage) -> Result<(), SyncError> {
    let dot = svc.storage.call({ let id = msg.agente_id.clone(); move |s| s.dots().get(&id) }).await.ok();
    let Some(dot) = dot.filter(|d| d.status != DotStatus::Archived) else {
        client.call("POST", &path(&format!("/mensagens/{}/estado", msg.id)), None, Some(&json!({ "estado": "erro", "detalhe": "este agente nao existe mais no app" }))).await?;
        return Ok(());
    };
    let cid = svc.flexi_conversation_for(&dot.id).await?;
    let meta = json!({ "flexi": { "mensagem_id": msg.id, "usuario_id": msg.usuario_id, "autor_nome": msg.autor_nome } });
    let attachments = if let Some(imagem) = &msg.imagem {
        let saved = svc.dispatch("conversations.attach", json!({ "conversation_id": cid, "name": imagem.nome, "data": imagem.dados })).await?;
        vec![saved.get("path").and_then(Value::as_str).ok_or_else(|| SyncError::Failed("anexo sem caminho".into()))?.to_string()]
    } else {
        vec![]
    };
    let (_, task) = svc.send_conversation_message(&cid, prompt_for(msg), attachments, meta).await?;
    client.call("POST", &path(&format!("/mensagens/{}/estado", msg.id)), None, Some(&json!({ "estado": "processando" }))).await?;
    st.inflight.insert(msg.id, InFlight { task_id: task.id, conversation_id: cid, agente_id: dot.id, last_pushed: None });
    Ok(())
}

/// Depois de um reinicio, reencontra a tarefa de uma mensagem que ja estava "processando".
async fn recover_message(svc: &RuntimeService, st: &mut SyncState, msg: &InboxMessage) -> DotsResult<bool> {
    let cid = svc.flexi_conversation_for(&msg.agente_id).await?;
    let msgs = svc.storage.call({ let c = cid.clone(); move |s| s.conversations().messages(&c, CONVERSATION_SCAN) }).await?;
    let Some(task_id) = msgs.iter().rev().find(|m| m.role == MessageRole::User && flexi_message_id(&m.metadata) == Some(msg.id)).and_then(|m| m.metadata.get("task_id").and_then(Value::as_str).map(String::from)) else {
        return Ok(false);
    };
    st.inflight.insert(msg.id, InFlight { task_id, conversation_id: cid, agente_id: msg.agente_id.clone(), last_pushed: None });
    Ok(true)
}

async fn push_progress(svc: &RuntimeService, client: &FlexiClient, st: &mut SyncState) -> Result<(), SyncError> {
    let ids: Vec<i64> = st.inflight.keys().copied().collect();
    for id in ids {
        let Some(mut f) = st.inflight.get(&id).cloned() else { continue };
        let (task, msgs) = svc
            .storage
            .call({ let (t, c) = (f.task_id.clone(), f.conversation_id.clone()); move |s| Ok((s.tasks().get(&t).ok(), s.conversations().messages(&c, CONVERSATION_SCAN)?)) })
            .await?;
        let reply = msgs.iter().rev().find(|m| m.role == MessageRole::Dot && m.task_id.as_deref() == Some(&f.task_id));
        let situacao = task.as_ref().map(|t| t.status.as_str().to_string()).unwrap_or_default();
        let (dots_message_id, content, estado, done, acoes) = match (reply, task.as_ref()) {
            (Some(m), _) => {
                let (estado, done) = reply_state(&m.metadata);
                (m.id.clone(), reply_content(m, done), estado, done, reply_actions(&m.metadata))
            }
            (None, Some(t)) if task_is_terminal(t.status) => {
                let content = t.result_text.clone().filter(|r| !r.is_empty()).or_else(|| t.error.clone()).unwrap_or_else(|| "A tarefa terminou sem resposta.".into());
                (format!("task:{}", t.id), content, if t.status == TaskStatus::Completed { "concluida" } else { "erro" }, true, vec![])
            }
            (None, Some(_)) => continue,
            (None, None) => (format!("task:{}", f.task_id), "A tarefa nao existe mais.".to_string(), "erro", true, vec![]),
        };
        let key = (content.clone(), estado.to_string(), situacao.clone());
        if f.last_pushed.as_ref() != Some(&key) {
            let body = json!({
                "agente_id": f.agente_id,
                "dots_message_id": dots_message_id,
                "conteudo": content,
                "estado": estado,
                "situacao": situacao,
                "acoes": acoes,
                "em_resposta_a": id,
            });
            client.call("POST", &path("/mensagens"), None, Some(&body)).await?;
            f.last_pushed = Some(key);
            st.inflight.insert(id, f.clone());
        }
        if done {
            client.call("POST", &path(&format!("/mensagens/{id}/estado")), None, Some(&json!({ "estado": if estado == "erro" { "erro" } else { "concluida" } }))).await?;
            st.inflight.remove(&id);
        }
    }
    Ok(())
}

/// Espelha no Flexi a conversa principal dos DOTs (a mesma do app): mensagens do usuario, do DOT e de sistema,
/// com upsert por `dots_message_id`. O que nasceu no Flexi ja esta la, e as respostas a mensagens do Flexi em
/// andamento ficam com `push_progress`. Na primeira vez de cada DOT vao so as `MIRROR_BACKFILL` mais recentes.
async fn push_mirror(svc: &RuntimeService, client: &FlexiClient, st: &mut SyncState) -> Result<(), SyncError> {
    let dots = svc.flexi_shared_dots().await?;
    let targets: Vec<String> = if st.mirror_all {
        dots.iter().map(|d| d.id.clone()).collect()
    } else {
        let mut t: Vec<String> = st.mirror_dirty.iter().cloned().collect();
        t.extend(st.mirror.iter().filter(|(_, m)| !m.open.is_empty()).map(|(id, _)| id.clone()));
        t.sort();
        t.dedup();
        t.retain(|id| dots.iter().any(|d| &d.id == id));
        t
    };
    let inflight_tasks: HashSet<String> = st.inflight.values().map(|f| f.task_id.clone()).collect();
    let mut cursors_changed = false;
    for dot_id in targets {
        let conv = svc.storage.call({ let d = dot_id.clone(); move |s| Ok(s.conversations().list(Some(&d))?.into_iter().min_by_key(|c| c.created_at)) }).await?;
        let Some(conv) = conv else { continue };
        st.main_conversations.insert(conv.id.clone(), dot_id.clone());
        let msgs = svc.storage.call({ let c = conv.id.clone(); move |s| s.conversations().messages(&c, CONVERSATION_SCAN) }).await?;
        let mirror = st.mirror.entry(dot_id.clone()).or_default();
        let candidates: Vec<Message> = match mirror.cursor {
            None => msgs.iter().skip(msgs.len().saturating_sub(MIRROR_BACKFILL)).cloned().collect(),
            Some(cursor) => msgs.iter().filter(|m| m.created_at > cursor || mirror.open.contains_key(&m.id)).cloned().collect(),
        };
        for m in candidates {
            let inflight = m.role == MessageRole::Dot && m.task_id.as_deref().is_some_and(|t| inflight_tasks.contains(t));
            if let Some(p) = mirror_payload(&m).filter(|_| !inflight) {
                let situacao = match (&m.task_id, p.done) {
                    (Some(t), false) => svc.storage.call({ let t = t.clone(); move |s| s.tasks().get(&t) }).await.map(|t| t.status.as_str().to_string()).unwrap_or_default(),
                    _ => String::new(),
                };
                let key = (p.conteudo.clone(), p.estado.to_string(), situacao.clone());
                if mirror.open.get(&m.id) != Some(&key) {
                    let body = json!({
                        "agente_id": dot_id,
                        "dots_message_id": m.id,
                        "papel": p.papel,
                        "conteudo": p.conteudo,
                        "estado": p.estado,
                        "situacao": situacao,
                        "acoes": p.acoes,
                        "origem": p.origem,
                        "detalhe": p.detalhe,
                        "criada_em": m.created_at,
                    });
                    client.call("POST", &path("/mensagens"), None, Some(&body)).await?;
                    if p.done {
                        mirror.open.remove(&m.id);
                    } else {
                        mirror.open.insert(m.id.clone(), key);
                    }
                }
            }
            if mirror.cursor.map_or(true, |c| m.created_at > c) {
                mirror.cursor = Some(m.created_at);
                cursors_changed = true;
            }
        }
    }
    st.mirror_all = false;
    st.mirror_dirty.clear();
    if cursors_changed {
        svc.save_mirror_cursors(st.mirror.iter().filter_map(|(id, m)| m.cursor.map(|c| (id.clone(), c))).collect()).await;
    }
    Ok(())
}

async fn cycle(svc: &RuntimeService, client: &FlexiClient, st: &mut SyncState) -> Result<(), SyncError> {
    if !st.started || st.registry_dirty {
        register_agents(svc, client).await?;
        st.registry_dirty = false;
        st.mirror_all = true;
        st.schedules_dirty = true;
        st.goals_dirty = true;
    }

    let (pending, dots) = svc.flexi_sync_counts().await?;
    let mut body = json!({ "versao": crate::VERSION, "pendentes": pending, "dots": dots });
    // Estado de cada DOT compartilhado (a linha da lista de contatos): so quando mudou desde o ultimo aceite.
    let states = svc.flexi_shared_states().await?;
    let states_json = serde_json::to_string(&states).unwrap_or_default();
    let states_due = !st.started || st.last_states.as_deref() != Some(states_json.as_str());
    if states_due {
        body["estados"] = Value::Array(states);
    }
    if !st.started || st.approvals_dirty {
        let approvals = svc.storage.call(|s| s.approvals().list(false, APPROVALS_LIMIT)).await?;
        body["aprovacoes"] = json!(approvals);
    }
    // Avisos pendentes, a lista inteira: o Flexi marca como lido o que saiu dela.
    if !st.started || st.notifications_dirty {
        let notifications = svc.storage.call(|s| s.notifications().list(true, NOTIFICATIONS_LIMIT)).await?;
        body["notificacoes"] = json!(notifications);
    }
    // Agendamentos de todos os DOTs, a lista inteira: o Flexi substitui a de cada agente.
    if !st.started || st.schedules_dirty {
        let schedules = svc.storage.call(|s| s.schedules().list(None)).await?;
        body["agendamentos"] = Value::Array(schedules.iter().map(schedule_payload).collect());
    }
    // Metas com o historico recente, a lista inteira.
    if !st.started || st.goals_dirty {
        body["metas"] = Value::Array(svc.goals_snapshot().await?);
    }
    let resp = client.call("POST", &path("/sincronizar"), None, Some(&body)).await?;
    // So depois do aceite: um erro de rede deixa os dados para a proxima rodada.
    st.started = true;
    st.approvals_dirty = false;
    st.notifications_dirty = false;
    st.schedules_dirty = false;
    st.goals_dirty = false;
    if states_due {
        st.last_states = Some(states_json);
    }
    let resp: SyncResponse = serde_json::from_value(resp).map_err(|e| SyncError::Failed(format!("resposta inesperada do Flexi: {e}")))?;
    if resp.reenviar_registro && !st.registry_dirty {
        tracing::info!("Flexi pediu o registro dos DOTs de novo; reenviando agentes, equipes e estados na proxima rodada");
        st.registry_dirty = true;
        st.last_states = None;
        st.notifications_dirty = true;
        st.schedules_dirty = true;
        st.goals_dirty = true;
    }

    // Comandos do dono dados no Flexi web (menu do agente): aplica e confirma; o estado novo vai na proxima rodada.
    for c in &resp.comandos {
        let body = match svc.apply_flexi_command(c).await {
            Ok(resultado) => json!({ "ok": true, "resultado": resultado }),
            Err(e) => {
                tracing::warn!(comando = c.id, tipo = %c.tipo, error = %e, "comando do Flexi web nao pode ser aplicado");
                json!({ "ok": false, "erro": e.to_string() })
            }
        };
        client.call("POST", &path(&format!("/comandos/{}/aplicada", c.id)), None, Some(&body)).await?;
        st.registry_dirty = true;
        st.schedules_dirty = true;
    }

    // Avisos dispensados no Flexi web: o mesmo efeito de clicar no balao do app.
    for id in &resp.dispensadas {
        let cleared = svc.storage.call({ let id = id.clone(); move |s| s.notifications().clear(Some(&id)) }).await?;
        if cleared > 0 {
            svc.events.emit(NOTIFICATION_EVENT, json!({ "cleared": cleared, "id": id, "via": "flexi" }));
        }
        client.call("POST", &path(&format!("/notificacoes/{id}/aplicada")), None, None).await?;
    }

    for d in &resp.decisoes {
        let body = match svc.apply_flexi_decision(d).await {
            Ok(a) => json!({ "aprovacao": a }),
            Err(e) => {
                tracing::warn!(approval = %d.id, error = %e, "decisao do Flexi web nao pode ser aplicada");
                let current = svc.storage.call({ let id = d.id.clone(); move |s| s.approvals().get(&id) }).await.ok();
                json!({ "erro": e.to_string(), "aprovacao": current })
            }
        };
        client.call("POST", &path(&format!("/aprovacoes/{}/aplicada", d.id)), None, Some(&body)).await?;
        st.approvals_dirty = true;
    }

    for m in &resp.mensagens {
        if st.inflight.contains_key(&m.id) {
            continue;
        }
        if m.estado == "processando" && recover_message(svc, st, m).await? {
            continue;
        }
        if let Err(e) = start_message(svc, client, st, m).await {
            match e {
                SyncError::Unauthorized(m) => return Err(SyncError::Unauthorized(m)),
                SyncError::Failed(err) => {
                    tracing::warn!(mensagem = m.id, error = %err, "mensagem do Flexi nao pode ser entregue ao DOT");
                    client.call("POST", &path(&format!("/mensagens/{}/estado", m.id)), None, Some(&json!({ "estado": "erro", "detalhe": err }))).await?;
                }
            }
        }
    }

    push_progress(svc, client, st).await?;
    push_mirror(svc, client, st).await
}

/// Espera o proximo ciclo: `POLL_INTERVAL` ou um evento que mude o que o Flexi precisa ver, o que vier primeiro.
async fn wait_tick(svc: &RuntimeService, rx: &mut broadcast::Receiver<dots_ipc::Event>, st: &mut SyncState, timeout: Duration) {
    let mut shutdown = Box::pin(svc.shutdown.clone().wait());
    let sleep = tokio::time::sleep(timeout);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut shutdown => return,
            _ = &mut sleep => return,
            ev = rx.recv() => match ev {
                Ok(ev) if ev.event == RECONNECT_EVENT || ev.event == "apps.changed" => return,
                Ok(ev) if REGISTRY_EVENTS.contains(&ev.event.as_str()) => { st.registry_dirty = true; return }
                Ok(ev) if ev.event == "settings.changed" && ev.payload.get("key").and_then(Value::as_str) == Some(TEAMS_ORDER_SETTING) => { st.registry_dirty = true; return }
                Ok(ev) if APPROVAL_EVENTS.contains(&ev.event.as_str()) => { st.approvals_dirty = true; return }
                Ok(ev) if ev.event == NOTIFICATION_EVENT => { st.notifications_dirty = true; return }
                Ok(ev) if ev.event == AUTOMATION_EVENT => { st.schedules_dirty = true; return }
                Ok(ev) if ev.event == GOAL_EVENT => { st.goals_dirty = true; return }
                Ok(ev) if ev.event == "task.changed" => {
                    let id = ev.payload.get("id").and_then(Value::as_str).unwrap_or("");
                    if st.inflight.values().any(|f| f.task_id == id) { return }
                }
                Ok(ev) if ev.event == "message.changed" => {
                    let cid = ev.payload.get("conversation_id").and_then(Value::as_str).unwrap_or("");
                    if let Some(dot_id) = st.main_conversations.get(cid) {
                        st.mirror_dirty.insert(dot_id.clone());
                        return;
                    }
                    if st.inflight.values().any(|f| f.conversation_id == cid) { return }
                }
                // Conversa criada ou apagada: a principal de algum DOT pode ter mudado.
                Ok(ev) if ev.event == "conversation.changed" => { st.mirror_all = true; return }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
        }
    }
}

/// Tarefa de fundo: sincroniza com o Flexi enquanto houver conta do usuario com sessao.
pub async fn run(svc: Arc<RuntimeService>) {
    let mut rx = svc.events.subscribe();
    let mut st = SyncState::default();
    st.mirror = svc.mirror_cursors().await.into_iter().map(|(id, c)| (id, Mirror { cursor: Some(c), open: HashMap::new() })).collect();
    let mut backoff = BACKOFF_MIN;
    let mut last_cycle: Option<tokio::time::Instant> = None;
    loop {
        if svc.shutdown.is_triggered() {
            svc.set_link(false, None, None);
            return;
        }
        let account = match svc.flexi_sync_account().await {
            Ok(Some(a)) => a,
            Ok(None) => {
                svc.set_link(false, None, None);
                st.started = false;
                wait_tick(&svc, &mut rx, &mut st, IDLE_RETRY).await;
                continue;
            }
            Err(e) => {
                svc.set_link(false, None, Some(e.to_string()));
                wait_tick(&svc, &mut rx, &mut st, IDLE_RETRY).await;
                continue;
            }
        };
        let (conn, client) = account;
        if let Some(t) = last_cycle {
            if t.elapsed() < MIN_CYCLE_GAP {
                tokio::time::sleep(MIN_CYCLE_GAP - t.elapsed()).await;
            }
        }
        last_cycle = Some(tokio::time::Instant::now());
        match cycle(&svc, &client, &mut st).await {
            Ok(()) => {
                svc.set_link(true, Some(conn.base_url.clone()), None);
                backoff = BACKOFF_MIN;
                wait_tick(&svc, &mut rx, &mut st, POLL_INTERVAL).await;
            }
            Err(SyncError::Unauthorized(reason)) => {
                tracing::warn!(reason = %reason, "sessao do Flexi recusada na sincronizacao; tentando renovar");
                svc.set_link(false, Some(conn.base_url.clone()), Some(reason));
                st.started = false;
                if let Err(e) = svc.flexi_check(&conn.id).await {
                    tracing::warn!(error = %e, "renovacao da sessao do Flexi falhou");
                }
                wait_tick(&svc, &mut rx, &mut st, Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
            Err(SyncError::Failed(reason)) => {
                tracing::warn!(reason = %reason, backoff, "sincronizacao com o Flexi falhou");
                svc.set_link(false, Some(conn.base_url.clone()), Some(reason));
                wait_tick(&svc, &mut rx, &mut st, Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(autor: &str, texto: &str) -> InboxMessage {
        InboxMessage { id: 7, agente_id: "dot-1".into(), usuario_id: Some(3), autor_nome: autor.into(), conteudo: texto.into(), imagem: None, estado: "enviada".into() }
    }

    #[test]
    fn prompt_identifica_quem_falou() {
        assert_eq!(prompt_for(&msg("Maria Silva", "  qual o estoque do MLB1?  ")), "Maria Silva (pelo Flexi): qual o estoque do MLB1?");
        assert_eq!(prompt_for(&msg("  ", "oi")), "Usuario do Flexi (pelo Flexi): oi");
        let mut com_imagem = msg("Maria Silva", "");
        com_imagem.imagem = Some(InboxImage { nome: "foto.png".into(), mime: "image/png".into(), dados: "YWJj".into() });
        assert_eq!(prompt_for(&com_imagem), "Maria Silva (pelo Flexi): imagem anexada");
    }

    #[test]
    fn estado_da_resposta_segue_os_metadados_do_executor() {
        assert_eq!(reply_state(&json!({ "streaming": true })), ("processando", false));
        // O executor grava ExecutionStatus::as_str() em maiusculas.
        assert_eq!(reply_state(&json!({ "streaming": false, "status": "COMPLETED", "tools": ["flexi_anuncios"] })), ("concluida", true));
        assert_eq!(reply_state(&json!({ "streaming": false, "status": "completed" })), ("concluida", true));
        assert_eq!(reply_state(&json!({ "streaming": false, "status": "FAILED", "error": "x" })), ("erro", true));
        assert_eq!(reply_state(&json!({ "streaming": false, "status": "CANCELLED" })), ("erro", true));
        assert_eq!(reply_state(&json!({})), ("processando", false));
        assert_eq!(reply_actions(&json!({ "tools": ["a", "b", 3] })), vec!["a".to_string(), "b".to_string()]);
        assert!(reply_actions(&json!({})).is_empty());
    }

    fn conv_msg(role: MessageRole, content: &str, metadata: Value) -> Message {
        Message { id: "m1".into(), conversation_id: "c1".into(), role, content: content.into(), task_id: Some("t1".into()), execution_id: None, metadata, created_at: dots_core::now() }
    }

    #[test]
    fn espelho_leva_usuario_dot_e_sistema_e_pula_ferramentas_e_o_que_veio_do_flexi() {
        let u = mirror_payload(&conv_msg(MessageRole::User, "oi", json!({}))).unwrap();
        assert_eq!((u.papel, u.conteudo.as_str(), u.estado, u.done, u.origem.as_str()), ("usuario", "oi", "concluida", true, ""));
        let n = mirror_payload(&conv_msg(MessageRole::System, "Mariana agendou: cron 0 8 * * *", json!({ "origin": "schedule:abc", "prompt": "rodada" }))).unwrap();
        assert_eq!((n.papel, n.estado, n.done, n.origem.as_str(), n.detalhe.as_str()), ("sistema", "concluida", true, "schedule", "rodada"));
        assert_eq!(mirror_payload(&conv_msg(MessageRole::System, "Gatilho: pedido", json!({ "origin": "event" }))).unwrap().origem, "event");
        assert_eq!(mirror_payload(&conv_msg(MessageRole::Tool, "x", json!({}))), None);
        assert_eq!(mirror_payload(&conv_msg(MessageRole::User, "Ana (pelo Flexi): oi", json!({ "flexi": { "mensagem_id": 3 } }))), None);
        let streaming = mirror_payload(&conv_msg(MessageRole::Dot, "", json!({ "streaming": true }))).unwrap();
        assert_eq!((streaming.papel, streaming.estado, streaming.done), ("agente", "processando", false));
        let failed = mirror_payload(&conv_msg(MessageRole::Dot, "", json!({ "streaming": false, "status": "FAILED", "error": "deu ruim", "tools": ["a"] }))).unwrap();
        assert_eq!((failed.conteudo.as_str(), failed.estado, failed.done, failed.acoes.clone()), ("deu ruim", "erro", true, vec!["a".to_string()]));
        let ok = mirror_payload(&conv_msg(MessageRole::Dot, "pronto", json!({ "streaming": false, "status": "COMPLETED" }))).unwrap();
        assert_eq!((ok.conteudo.as_str(), ok.estado, ok.done), ("pronto", "concluida", true));
    }

    #[test]
    fn le_id_da_mensagem_do_flexi_nos_metadados() {
        assert_eq!(flexi_message_id(&json!({ "flexi": { "mensagem_id": 42 }, "task_id": "t" })), Some(42));
        assert_eq!(flexi_message_id(&json!({ "attachments": [] })), None);
    }

    #[test]
    fn interpreta_resposta_da_sincronizacao() {
        let r: SyncResponse = serde_json::from_value(json!({
            "mensagens": [{ "id": 1, "agente_id": "d", "usuario_id": 9, "autor_nome": "Ana", "conteudo": "oi", "imagem": { "nome": "foto.png", "mime": "image/png", "dados": "YWJj" }, "estado": "enviada" }],
            "decisoes": [{ "id": "ap-1", "aprovado": false, "motivo": "nao agora" }],
            "dispensadas": ["n-1"],
            "comandos": [{ "id": 5, "agente_id": "d", "tipo": "definir_modelo", "dados": { "modelo": "gpt-5.6-terra" } }, { "id": 6, "agente_id": "d", "tipo": "pausar" }]
        }))
        .unwrap();
        assert_eq!(r.mensagens.len(), 1);
        assert_eq!(r.mensagens[0].autor_nome, "Ana");
        assert_eq!(r.mensagens[0].imagem.as_ref().map(|i| (i.nome.as_str(), i.mime.as_str(), i.dados.as_str())), Some(("foto.png", "image/png", "YWJj")));
        assert_eq!(r.decisoes[0], Decision { id: "ap-1".into(), aprovado: false, motivo: Some("nao agora".into()) });
        assert_eq!(r.dispensadas, vec!["n-1".to_string()]);
        assert_eq!(r.comandos[0], Command { id: 5, agente_id: "d".into(), tipo: "definir_modelo".into(), dados: json!({ "modelo": "gpt-5.6-terra" }) });
        assert_eq!(r.comandos[1].dados, Value::Null);
        let vazio: SyncResponse = serde_json::from_value(json!({ "status": 1 })).unwrap();
        assert!(vazio.mensagens.is_empty() && vazio.decisoes.is_empty() && !vazio.reenviar_registro);
        let pede: SyncResponse = serde_json::from_value(json!({ "status": 1, "reenviar_registro": true })).unwrap();
        assert!(pede.reenviar_registro);
    }

    #[test]
    fn equipes_seguem_a_ordem_do_usuario_e_apontam_a_equipe_pai() {
        let t = |id: &str, name: &str, manager: Option<&str>, members: &[&str]| Team {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            manager_dot_id: manager.map(String::from),
            member_ids: members.iter().map(|m| m.to_string()).collect(),
            created_at: dots_core::now(),
            updated_at: dots_core::now(),
        };
        let teams = vec![t("eq-a", "Marketing", Some("ger-a"), &["f-1"]), t("eq-b", "Ads", Some("ger-b"), &["ger-a"]), t("eq-c", "Sem dono", None, &["f-2"])];
        let out = teams_payload(&teams, &["eq-c".to_string(), "eq-a".to_string()]);
        assert_eq!(out.iter().map(|v| v["id"].as_str().unwrap()).collect::<Vec<_>>(), vec!["eq-c", "eq-a", "eq-b"]);
        let a = out.iter().find(|v| v["id"] == "eq-a").unwrap();
        assert_eq!(a["pai"], json!("Ads"));
        assert_eq!(a["gerente_id"], json!("ger-a"));
        assert_eq!(a["membros"], json!(["f-1"]));
        assert_eq!(out[0]["pai"], Value::Null);
        assert_eq!(out[0]["gerente_id"], Value::Null);
    }

    #[test]
    fn compartilhar_o_gerente_compartilha_os_funcionarios() {
        let dot = |id: &str, enabled: bool, users: &[i64]| Dot {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            objective: String::new(),
            instructions: String::new(),
            primary_engine: None,
            model: None,
            workspace_path: None,
            limits: Default::default(),
            status: DotStatus::Active,
            role: dots_core::DotRole::Worker,
            flexi_share: dots_core::FlexiShare { enabled, users: users.iter().map(|u| dots_core::FlexiShareUser { id: *u, name: String::new() }).collect() },
            created_at: dots_core::now(),
            updated_at: dots_core::now(),
        };
        let team = |id: &str, manager: &str, members: &[&str]| Team {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            manager_dot_id: Some(manager.into()),
            member_ids: members.iter().map(|m| m.to_string()).collect(),
            created_at: dots_core::now(),
            updated_at: dots_core::now(),
        };
        let dots = vec![dot("ger", true, &[7, 9]), dot("func", false, &[]), dot("sub-ger", false, &[]), dot("sub-func", false, &[]), dot("proprio", true, &[3]), dot("solto", false, &[])];
        let teams = vec![team("eq", "ger", &["func", "sub-ger"]), team("sub", "sub-ger", &["sub-func", "proprio"])];
        assert_eq!(shared_users(&dots[0], &dots, &teams), vec![7, 9]);
        assert_eq!(shared_users(&dots[1], &dots, &teams), vec![7, 9]);
        assert_eq!(shared_users(&dots[3], &dots, &teams), vec![7, 9], "sobe duas equipes ate o gerente compartilhado");
        assert_eq!(shared_users(&dots[4], &dots, &teams), vec![3], "o proprio compartilhamento vale mais");
        assert!(shared_users(&dots[5], &dots, &teams).is_empty());
        assert_eq!(agent_payload(&dots[5], vec![])["usuarios"], json!([]));
    }

    #[test]
    fn monta_os_caminhos_do_app() {
        assert_eq!(path("/sincronizar"), "/api/agentes-dots/app/sincronizar");
        assert_eq!(path("/mensagens/12/estado"), "/api/agentes-dots/app/mensagens/12/estado");
    }
}
