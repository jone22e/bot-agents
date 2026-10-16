//! Grupo geral: o usuario escreve para todos os gerentes de uma vez. Cada mensagem vira um pedido na conversa
//! principal de cada gerente (origem `group:<id>`); as respostas sao lidas de la e aparecem no grupo.
//! Com gerentes marcados (@Nome), so eles respondem; os outros recebem para ciencia com a origem
//! `group-cc:<id>`, que nao entra no grupo.

use dots_core::{Dot, DotRole, DotsError, DotsResult, MessageRole, TaskCreate, TaskKind};
use dots_storage::repos::GROUP_ORIGIN_PREFIX;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::service::RuntimeService;

#[derive(Deserialize, Default)]
struct MessagesParams {
    #[serde(default)]
    limit: u32,
}
#[derive(Deserialize)]
struct SendParams {
    content: String,
    /// Ids dos gerentes marcados com @ na mensagem; vazio = todos respondem.
    #[serde(default)]
    mentions: Vec<String>,
}

/// Origem das tarefas de quem recebeu a mensagem so para ciencia (nao comeca com `group:`, entao a resposta fica fora do grupo).
const GROUP_CC_ORIGIN_PREFIX: &str = "group-cc:";

impl RuntimeService {
    /// Membros do grupo: quem gerencia uma equipe ou e supervisor (DOTs nao arquivados).
    pub async fn group_members(&self) -> DotsResult<Vec<Dot>> {
        self.storage
            .call(|s| {
                let managers: std::collections::HashSet<String> = s.teams().list()?.into_iter().filter_map(|t| t.manager_dot_id).collect();
                Ok(s.dots().list(false)?.into_iter().filter(|d| d.role == DotRole::Supervisor || managers.contains(&d.id)).collect())
            })
            .await
    }

    pub async fn handle_group(&self, method: &str, params: Value) -> DotsResult<Option<Value>> {
        use crate::service::parse_params as p;
        Ok(Some(match method {
            "group.members" => json!(self.group_members().await?),
            "group.messages" => {
                let MessagesParams { limit } = p(params).unwrap_or_default();
                json!(self.storage.call(move |s| s.group().messages(limit)).await?)
            }
            "group.send" => {
                let SendParams { content, mentions } = p(params)?;
                let content = content.trim().to_string();
                if content.is_empty() {
                    return Err(DotsError::Validation("a mensagem esta vazia".into()));
                }
                let members = self.group_members().await?;
                if members.is_empty() {
                    return Err(DotsError::Validation("o grupo ainda nao tem gerentes: crie uma equipe com gerente em Equipes".into()));
                }
                let names = members.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", ");
                let mentioned: Vec<&Dot> = members.iter().filter(|d| mentions.contains(&d.id)).collect();
                let mentioned_names = mentioned.iter().map(|d| format!("@{}", d.name)).collect::<Vec<_>>().join(", ");
                let meta = json!({ "members": members.len(), "mentions": mentioned.iter().map(|d| json!({ "id": d.id, "name": d.name })).collect::<Vec<_>>() });
                let msg = self.storage.call({ let c = content.clone(); move |s| s.group().add_user_message(&c, meta) }).await?;
                let mut tasks = vec![];
                for m in &members {
                    let addressed = mentioned.is_empty() || mentioned.iter().any(|d| d.id == m.id);
                    let origin = format!("{}{}", if addressed { GROUP_ORIGIN_PREFIX } else { GROUP_CC_ORIGIN_PREFIX }, msg.id);
                    let prompt = if mentioned.is_empty() {
                        format!("Mensagem do usuario no grupo geral, que todos os gerentes recebem ({names}). Sua resposta aparece no grupo para o usuario e os outros gerentes.\n\n{content}")
                    } else if addressed {
                        format!("Mensagem do usuario no grupo geral, que todos os gerentes recebem ({names}). O usuario marcou {mentioned_names}: a mensagem e dirigida a voce, responda e cuide do pedido. Sua resposta aparece no grupo para o usuario e os outros gerentes.\n\n{content}")
                    } else {
                        format!("Mensagem do usuario no grupo geral ({names}), dirigida a {mentioned_names}, nao a voce. Apenas tome conhecimento: nao execute acoes, nao delegue e nao responda ao pedido. Responda somente \"Ciente.\" (esta resposta nao aparece no grupo).\n\n{content}")
                    };
                    let title = if addressed { format!("Grupo geral: {}", content.chars().take(60).collect::<String>()) } else { format!("Grupo geral (para {mentioned_names}, so ciencia): {}", content.chars().take(60).collect::<String>()) };
                    // A mensagem tambem fica na conversa do gerente, para ele (e o usuario) verem o contexto da resposta.
                    let conv = self.storage.call({ let d = m.id.clone(); move |s| s.conversations().main(&d) }).await?;
                    let (cid, c, o) = (conv.id.clone(), content.clone(), origin.clone());
                    self.storage.call(move |s| s.conversations().add_message(&cid, MessageRole::User, &c, None, None, json!({ "origin": o }))).await?;
                    let task = self
                        .create_task(TaskCreate {
                            dot_id: m.id.clone(),
                            prompt,
                            title: Some(title),
                            kind: Some(TaskKind::Chat),
                            conversation_id: Some(conv.id.clone()),
                            origin: Some(origin),
                            ..Default::default()
                        })
                        .await?;
                    self.events.emit("message.changed", json!({ "conversation_id": conv.id, "dot_id": m.id }));
                    tasks.push(task);
                }
                self.audit("group.send", "group", Some(&msg.id), json!({ "members": members.len(), "mentions": mentioned.len() })).await?;
                self.events.emit("group.changed", json!({ "id": msg.id }));
                json!({ "message": msg, "tasks": tasks })
            }
            _ => return Ok(None),
        }))
    }
}
