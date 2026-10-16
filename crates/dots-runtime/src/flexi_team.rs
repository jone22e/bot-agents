//! Especialidades de gerente do Flexi (aplicativo Agentes DOTS) e montagem das equipes.

use std::collections::HashMap;

use dots_core::{AutoApproveLevel, ConnectionStatus, DotCreate, DotIntegration, DotRole, DotsError, DotsResult, TeamCreate, ToolPolicyUpdate};
use dots_flexi::FlexiError;
use serde::Serialize;
use serde_json::{json, Value};

use crate::flexi::{KIND, PRICE_RULES_SUMMARY};
use crate::service::RuntimeService;

/// Rota do Flexi com as especialidades do aplicativo Agentes DOTS (cadastradas em Configuracoes > Aplicativos).
pub const ESPECIALIDADES_PATH: &str = "/api/sub-aplicativos/agentes-dots/especialidades";

/// Especialidade e sempre de gerente: ele contrata os funcionarios (dots_hire_employee) e define o que cada um faz.
#[derive(Debug, Clone, Serialize)]
pub struct FlexiRoleTemplate {
    /// Codigo da especialidade no Flexi (gravado no DOT como `role_template`).
    pub id: String,
    pub name: String,
    pub role: DotRole,
    pub trust: AutoApproveLevel,
    pub purpose: String,
    /// Setor do sistema no Flexi (ex.: `mercadolivre`, `compras`); vira a area de trabalho do DOT.
    pub group: String,
    /// Nome do setor (ex.: "Mercado Livre").
    pub group_name: String,
    /// Nome da equipe que este gerente gerencia: o nome do setor.
    pub manages: Option<String>,
}

fn build(id: &str, name: &str, group: &str, group_name: &str, trust: AutoApproveLevel, purpose: String) -> FlexiRoleTemplate {
    FlexiRoleTemplate {
        id: id.to_string(),
        name: name.to_string(),
        role: DotRole::Supervisor,
        trust,
        purpose,
        group: group.to_string(),
        group_name: group_name.to_string(),
        manages: Some(group_name.to_string()),
    }
}

/// Converte a resposta do Flexi (`{ items: [{ codigo, nome, setor, setor_nome, permissao, instrucoes }] }`) em
/// especialidades. Itens sem codigo, nome ou setor conhecido (`setor_nome` vazio) sao ignorados.
pub fn templates_from_flexi(v: &Value) -> Vec<FlexiRoleTemplate> {
    let items = v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default();
    items
        .iter()
        .filter_map(|it| {
            let s = |k: &str| it.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            let (id, name, group, group_name) = (s("codigo"), s("nome"), s("setor"), s("setor_nome"));
            if id.is_empty() || name.is_empty() || group.is_empty() || group_name.is_empty() {
                return None;
            }
            let trust = if s("permissao") == "write" { AutoApproveLevel::Write } else { AutoApproveLevel::Read };
            Some(build(&id, &name, &group, &group_name, trust, s("instrucoes")))
        })
        .collect()
}

fn t(id: &str, name: &str, group: &str, group_name: &str, purpose: &str) -> FlexiRoleTemplate {
    build(id, name, group, group_name, AutoApproveLevel::Read, purpose.to_string())
}

/// Especialidades embutidas: usadas so quando o Flexi ainda nao tem a rota de especialidades (versao antiga) ou nao
/// responde. No Flexi elas ficam na tabela `agentes_dots_especialidades`.
pub fn builtin_templates() -> Vec<FlexiRoleTemplate> {
    let common = "Voce trabalha dentro do Flexi pelas ferramentas flexi_*. Antes de agir, consulte os dados (contas, anuncios, precos calculados). Explique em portugues, de forma breve, o que fez e o que recomenda. Nunca invente ids: busque-os com as ferramentas.";
    vec![
        t("ml_manager", "SubGerente de Mercado Livre", "mercadolivre", "Mercado Livre",
          &format!("Gerencia a conta Mercado Livre: acompanha anuncios, precos, perguntas e moderacao com flexi_anuncios, flexi_anuncios_geral, flexi_moderacao_itens e flexi_perguntas_metricas. Delega criacao de anuncios ao Criador, revisao ao Revisor e atendimento ao Atendimento. Consolida e reporta. {PRICE_RULES_SUMMARY} {common}")),
        t("shopee_manager", "SubGerente de Shopee", "shopee", "Shopee",
          &format!("Gerencia a conta Shopee: saude da conta (flexi_shopee_saude_conta), anuncios, estoque, promocoes e buybox. Delega aos agentes de Shopee e consolida. {PRICE_RULES_SUMMARY} {common}")),
        t("ads_manager", "Gerente de Google Ads", "googleads", "Google Ads",
          &format!("Gerencia a presenca da empresa no Google, que no Flexi tem duas partes: o catalogo do Google Shopping (anuncios no marketplace 2, enviados ao Merchant Center) e as campanhas do Google Ads (midia). Acompanha a cobertura do catalogo com flexi_anuncios (marketplace 2) e as campanhas com flexi_ads_dashboard, flexi_ads_series, flexi_ads_palavras_chave e flexi_ads_produtos. Quando faltam produtos no Google, delega ao Agente de Publicacao a criacao e publicacao dos anuncios no Google Shopping (produto a produto, por id); quando campanhas tem ROAS baixo ou gasto fora do esperado, delega ajustes ao Agente de Campanhas. Responde as perguntas dos seus agentes (dots_answer_question) e so envolve o usuario quando a decisao for dele. Consolida um resumo com numeros. {common}")),
    ]
}

impl RuntimeService {
    /// Especialidades do aplicativo Agentes DOTS no Flexi. Usa a conexao informada ou a primeira conectada. Se o Flexi
    /// nao responder (ou for uma versao sem a rota), usa a ultima lista lida e, sem ela, os papeis embutidos.
    pub async fn flexi_templates(&self, connection_id: Option<&str>) -> Vec<FlexiRoleTemplate> {
        match self.flexi_fetch_templates(connection_id).await {
            Ok(list) => {
                *self.flexi.templates.lock().unwrap() = Some(list.clone());
                list
            }
            Err(e) => {
                tracing::warn!(error = %e, "especialidades do Flexi indisponiveis; usando a ultima lista conhecida");
                self.flexi.templates.lock().unwrap().clone().unwrap_or_else(builtin_templates)
            }
        }
    }

    async fn flexi_fetch_templates(&self, connection_id: Option<&str>) -> DotsResult<Vec<FlexiRoleTemplate>> {
        let id = connection_id.map(str::to_string);
        let conn = self
            .storage
            .call(move |s| match id {
                Some(id) => s.apps().get(&id).map(Some),
                None => Ok(s.apps().list(Some(KIND))?.into_iter().find(|c| c.status == ConnectionStatus::Connected)),
            })
            .await?
            .ok_or_else(|| DotsError::Validation("nenhuma conexao Flexi".into()))?;
        let client = self.flexi_client(&conn).await?;
        let mut result = client.call("GET", ESPECIALIDADES_PATH, None, None).await;
        if matches!(result, Err(FlexiError::Unauthenticated(_))) && self.flexi_relogin(&conn, client.clone()).await?.status == ConnectionStatus::Connected {
            result = client.call("GET", ESPECIALIDADES_PATH, None, None).await;
        }
        result.map(|v| templates_from_flexi(&v)).map_err(|e| DotsError::Validation(e.to_string()))
    }

    /// Codigo da especialidade -> nome (cargo), pela ultima lista lida do Flexi.
    pub fn flexi_role_names(&self) -> HashMap<String, String> {
        let list = self.flexi.templates.lock().unwrap().clone().unwrap_or_else(builtin_templates);
        list.into_iter().map(|t| (t.id, t.name)).collect()
    }

    /// Cria a equipe completa ligada a conexao, com delegacao configurada.
    pub async fn flexi_create_team(&self, connection_id: &str) -> DotsResult<serde_json::Value> {
        let conn = self.storage.call({ let id = connection_id.to_string(); move |s| s.apps().get(&id) }).await?;
        let engine = self.storage.call(|s| Ok(s.engines().list()?.into_iter().find(|e| e.enabled && e.last_detection.as_ref().map(|d| d.installed).unwrap_or(false)).map(|e| e.kind))).await?;
        let tpls = self.flexi_templates(Some(connection_id)).await;
        let mut created: Vec<(String, String)> = vec![];
        for tpl in tpls.iter().cloned() {
            let input = DotCreate {
                name: tpl.name.to_string(),
                description: String::new(),
                objective: tpl.purpose.split('.').next().unwrap_or("").to_string(),
                instructions: tpl.purpose.clone(),
                primary_engine: engine,
                workspace_path: None,
                limits: None,
                role: Some(tpl.role),
                ..Default::default()
            };
            let dot = self.storage.call(move |s| s.dots().create("user", input, None)).await?;
            let (did, cid, trust, tid, group) = (dot.id.clone(), conn.id.clone(), tpl.trust, tpl.id.clone(), tpl.group.clone());
            self.storage.call(move |s| {
                s.policies().update(&did, ToolPolicyUpdate { auto_approve_level: Some(trust), ..Default::default() })?;
                s.apps().set_integration(&did, KIND, Some(&cid), Some(&tid), Some(&group))?;
                Ok(())
            }).await?;
            let ws = self.layout.workspaces_dir.join(&dot.id);
            std::fs::create_dir_all(&ws)?;
            let (did2, wsp) = (dot.id.clone(), ws.to_string_lossy().to_string());
            let _ = self.storage.call(move |s| s.dots().update("user", &did2, dots_core::DotUpdate { workspace_path: Some(Some(wsp)), ..Default::default() })).await;
            created.push((tpl.id.clone(), dot.id.clone()));
        }
        // Equipes: uma por gerente, com o nome do setor; os funcionarios o gerente contrata depois.
        let dot_of = |tid: &str| created.iter().find(|(t, _)| t == tid).map(|(_, d)| d.clone());
        let mut team_ids: Vec<String> = vec![];
        for tpl in tpls.iter() {
            let Some(name) = tpl.manages.clone() else { continue };
            let input = TeamCreate { name, description: format!("Equipe do Flexi criada em \"Montar equipe de marketplaces\" ({})", conn.label), manager_dot_id: dot_of(&tpl.id), member_ids: vec![] };
            let team = self.storage.call(move |s| s.teams().create(input)).await?;
            self.sync_team_public(&team).await?;
            team_ids.push(team.id);
        }
        self.events.emit("teams.changed", json!({ "ids": team_ids }));
        self.audit("app.flexi.create_team", "app_connection", Some(connection_id), json!({ "dots": created.len() })).await?;
        self.events.emit("dots.changed", json!({ "action": "team" }));
        Ok(json!({ "created": created.iter().map(|(t, d)| json!({ "template": t, "dot_id": d })).collect::<Vec<_>>() }))
    }
}

impl RuntimeService {
    /// Liga um DOT a uma conexao Flexi, guardando o papel de origem, a area e o cargo. A hierarquia de delegacao e
    /// feita pelas equipes (`teams.*`), nao aqui.
    pub async fn flexi_assign_role(&self, dot_id: &str, connection_id: &str, template_id: Option<&str>, group: Option<&str>, role: Option<DotRole>) -> DotsResult<DotIntegration> {
        let tpl = match template_id {
            Some(id) => Some(self.flexi_templates(Some(connection_id)).await.into_iter().find(|t| t.id == id).ok_or_else(|| DotsError::Validation(format!("papel desconhecido: {id}")))?),
            None => None,
        };
        // Area de trabalho: a do setor da especialidade; sem especialidade (ex.: funcionario), a informada, se houver.
        let group: Option<String> = tpl.as_ref().map(|t| t.group.clone()).or(group.map(str::to_string)).filter(|g| !g.trim().is_empty());
        let role = role.or(tpl.as_ref().map(|t| t.role)).unwrap_or(DotRole::Worker);
        let integ = self
            .storage
            .call({ let (d, c, t, g) = (dot_id.to_string(), connection_id.to_string(), tpl.as_ref().map(|t| t.id.clone()), group.clone()); move |s| {
                s.dots().update("user", &d, dots_core::DotUpdate { role: Some(role), ..Default::default() })?;
                s.apps().set_integration(&d, KIND, Some(&c), t.as_deref(), g.as_deref())
            } })
            .await?
            .ok_or_else(|| DotsError::Validation("vinculo nao gravado".into()))?;
        self.audit("app.flexi.assign_role", "dot", Some(dot_id), json!({ "template": tpl.as_ref().map(|t| &t.id), "group": group, "role": role, "connection_id": connection_id })).await?;
        self.events.emit("dots.changed", json!({ "id": dot_id, "action": "role" }));
        Ok(integ)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_are_managers_of_their_sector() {
        let list = builtin_templates();
        assert_eq!(list.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["ml_manager", "shopee_manager", "ads_manager"]);
        assert!(list.iter().all(|t| t.role == DotRole::Supervisor));
        assert_eq!(list[2].manages.as_deref(), Some("Google Ads"));
    }

    #[test]
    fn flexi_items_become_managers() {
        let v = json!({ "items": [
            { "codigo": "gerente_compras", "nome": "Gerente de Compras", "setor": "compras", "setor_nome": "Compras", "permissao": "write", "instrucoes": "Coordena." },
            { "codigo": "mkt_manager", "nome": "Gerente de Marketplaces", "setor": "marketplaces", "setor_nome": "", "permissao": "read", "instrucoes": "x" },
            { "codigo": "", "nome": "Sem codigo", "setor": "rh", "setor_nome": "RH", "permissao": "read", "instrucoes": "" }
        ] });
        let list = templates_from_flexi(&v);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].role, DotRole::Supervisor);
        assert_eq!(list[0].trust, AutoApproveLevel::Write);
        assert_eq!((list[0].group.as_str(), list[0].manages.as_deref()), ("compras", Some("Compras")));
        assert!(templates_from_flexi(&json!({ "status": 1 })).is_empty());
    }
}
