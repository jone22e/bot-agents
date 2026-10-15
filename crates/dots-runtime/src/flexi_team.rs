//! Modelos de DOTs para o Flexi e montagem da equipe de marketplaces.

use dots_core::{AutoApproveLevel, DotCreate, DotIntegration, DotRole, DotsError, DotsResult, TeamCreate, ToolPolicyUpdate};
use serde::Serialize;
use serde_json::json;

use crate::flexi::{KIND, PRICE_RULES_SUMMARY};
use crate::service::RuntimeService;

#[derive(Debug, Clone, Serialize)]
pub struct FlexiRoleTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub role: DotRole,
    pub trust: AutoApproveLevel,
    pub purpose: String,
    /// ids dos papeis que este coordena (para a equipe montada).
    pub team: &'static [&'static str],
    /// Area de trabalho (ver [`GROUPS`]).
    pub group: &'static str,
    /// Nome da equipe que este papel gerencia, se gerente.
    pub manages: Option<&'static str>,
    /// Nome da equipe de que este papel e funcionario.
    pub member_of: Option<&'static str>,
}

fn team_names(template_id: &str) -> (Option<&'static str>, Option<&'static str>) {
    match template_id {
        "mkt_manager" => (Some("Marketplaces"), None),
        "ml_manager" => (Some("Mercado Livre"), Some("Marketplaces")),
        "shopee_manager" => (Some("Shopee"), Some("Marketplaces")),
        "ads_manager" => (Some("Google Ads"), Some("Marketplaces")),
        "publisher" => (None, Some("Google Ads")),
        id if id.starts_with("ml_") => (None, Some("Mercado Livre")),
        id if id.starts_with("shopee_") => (None, Some("Shopee")),
        id if id.starts_with("ads_") => (None, Some("Google Ads")),
        _ => (None, Some("Marketplaces")),
    }
}

/// Areas de trabalho: (id, nome). `marketplaces` e a area geral, acima das demais.
pub const GROUPS: &[(&str, &str)] = &[
    ("marketplaces", "Marketplaces (tudo)"),
    ("mercadolivre", "Mercado Livre"),
    ("shopee", "Shopee"),
    ("googleads", "Google Ads"),
    ("publicacao", "Publicacao"),
];

fn group_of(template_id: &str) -> &'static str {
    match template_id {
        "mkt_manager" => "marketplaces",
        id if id.starts_with("ml_") => "mercadolivre",
        id if id.starts_with("shopee_") => "shopee",
        id if id.starts_with("ads_") => "googleads",
        "publisher" => "googleads",
        _ => "marketplaces",
    }
}

fn t(id: &'static str, name: &'static str, role: DotRole, trust: AutoApproveLevel, purpose: &str, team: &'static [&'static str]) -> FlexiRoleTemplate {
    let (manages, member_of) = team_names(id);
    FlexiRoleTemplate { id, name, role, trust, purpose: purpose.to_string(), team, group: group_of(id), manages, member_of }
}

/// Nome do papel (cargo) de um modelo, ex. `ads_manager` -> "Gerente de Google Ads".
pub fn template_name(template_id: &str) -> Option<&'static str> {
    templates().into_iter().find(|t| t.id == template_id).map(|t| t.name)
}

pub fn templates() -> Vec<FlexiRoleTemplate> {
    let common = "Voce trabalha dentro do Flexi pelas ferramentas flexi_*. Antes de agir, consulte os dados (contas, anuncios, precos calculados). Explique em portugues, de forma breve, o que fez e o que recomenda. Nunca invente ids: busque-os com as ferramentas.";
    vec![
        t("mkt_manager", "Gerente de Marketplaces", DotRole::Supervisor, AutoApproveLevel::Read,
          &format!("Coordena a operacao de marketplaces da empresa. Recebe objetivos do usuario, divide em pedidos claros e delega aos subgerentes de Mercado Livre, Shopee e Google (Google Shopping e Google Ads). Acompanha resultados, identifica bloqueios e consolida um relatorio objetivo. Pergunta ao usuario quando a decisao for dele (dots_ask_user). {common}"),
          &["ml_manager", "shopee_manager", "ads_manager"]),
        t("ml_manager", "SubGerente de Mercado Livre", DotRole::Supervisor, AutoApproveLevel::Read,
          &format!("Gerencia a conta Mercado Livre: acompanha anuncios, precos, perguntas e moderacao com flexi_anuncios, flexi_anuncios_geral, flexi_moderacao_itens e flexi_perguntas_metricas. Delega criacao de anuncios ao Criador, revisao ao Revisor e atendimento ao Atendimento. Consolida e reporta. {PRICE_RULES_SUMMARY} {common}"),
          &["ml_creator", "ml_reviewer", "ml_support"]),
        t("ml_creator", "Agente de Mercado Livre: Criador de anuncios", DotRole::Worker, AutoApproveLevel::Read,
          &format!("Cria anuncios no Mercado Livre a partir de produtos do Flexi: usa flexi_preparar_criacao para validar categoria e atributos, propoe titulo e descricao claros e completos, e so entao flexi_criar_anuncio. Nunca publica no marketplace: flexi_publicar_anuncio so com pedido e permissao do usuario. {PRICE_RULES_SUMMARY} {common}"),
          &[]),
        t("ml_reviewer", "Agente de Mercado Livre: Revisor de anuncios", DotRole::Worker, AutoApproveLevel::Read,
          &format!("Revisa anuncios do Mercado Livre: titulo, descricao, atributos, preco versus flexi_precos_calculados, estoque e moderacao. Aponta problemas com o id do anuncio e propoe correcoes; aplica correcoes de conteudo com flexi_atualizar_conteudo e flexi_atualizar_descricao quando autorizado. {PRICE_RULES_SUMMARY} {common}"),
          &[]),
        t("ml_support", "Agente de Mercado Livre: Atendimento de perguntas", DotRole::Worker, AutoApproveLevel::Read,
          &format!("Atende perguntas de compradores: lista com flexi_perguntas, le o contexto com flexi_pergunta_contexto (anuncio, estoque, prazo) e responde com flexi_responder_pergunta de forma cordial, objetiva e sem prometer o que o anuncio nao garante. Em duvida, pergunta ao usuario antes de responder. {common}"),
          &[]),
        t("shopee_manager", "SubGerente de Shopee", DotRole::Supervisor, AutoApproveLevel::Read,
          &format!("Gerencia a conta Shopee: saude da conta (flexi_shopee_saude_conta), anuncios, estoque, promocoes e buybox. Delega aos agentes de Shopee e consolida. {PRICE_RULES_SUMMARY} {common}"),
          &["shopee_listings", "shopee_promos"]),
        t("shopee_listings", "Agente de Shopee: Anuncios e estoque", DotRole::Worker, AutoApproveLevel::Read,
          &format!("Cuida dos anuncios Shopee: consulta com flexi_anuncios (marketplace 1), corrige inconsistencias de estoque com flexi_estoque_inconsistencias e flexi_atualizar_estoque, pausa/reativa quando necessario. {PRICE_RULES_SUMMARY} {common}"),
          &[]),
        t("shopee_promos", "Agente de Shopee: Promocoes", DotRole::Worker, AutoApproveLevel::Read,
          &format!("Analisa promocoes e buybox da Shopee (flexi_promocoes_anuncio, flexi_shopee_buybox, flexi_lucro_analise) e propoe ajustes de preco respeitando o preco minimo. Aplica somente com autorizacao. {PRICE_RULES_SUMMARY} {common}"),
          &[]),
        t("ads_manager", "Gerente de Google Ads", DotRole::Supervisor, AutoApproveLevel::Read,
          &format!("Gerencia a presenca da empresa no Google, que no Flexi tem duas partes: o catalogo do Google Shopping (anuncios no marketplace 2, enviados ao Merchant Center) e as campanhas do Google Ads (midia). Acompanha a cobertura do catalogo com flexi_anuncios (marketplace 2) e as campanhas com flexi_ads_dashboard, flexi_ads_series, flexi_ads_palavras_chave e flexi_ads_produtos. Quando faltam produtos no Google, delega ao Agente de Publicacao a criacao e publicacao dos anuncios no Google Shopping (produto a produto, por id); quando campanhas tem ROAS baixo ou gasto fora do esperado, delega ajustes ao Agente de Campanhas. Responde as perguntas dos seus agentes (dots_answer_question) e so envolve o usuario quando a decisao for dele. Consolida um resumo com numeros. {common}"),
          &["ads_agent", "publisher"]),
        t("ads_agent", "Agente de Google Ads: Campanhas", DotRole::Worker, AutoApproveLevel::Read,
          &format!("Executa ajustes em campanhas do Google Ads com base em dados: pausar (flexi_ads_pausar_campanha), status, orcamento (flexi_ads_orcamento_campanha), ROAS alvo e palavras negativas. Sempre informa contaId e campaignName corretos obtidos de flexi_ads_contas e flexi_ads_dashboard. Sabe que as campanhas so divulgam itens que ja estao no Merchant Center (anuncios do Google Shopping, marketplace 2 no Flexi). Toda alteracao e justificada com numeros. {common}"),
          &[]),
        t("publisher", "Agente de Google Ads: Publicacao", DotRole::Worker, AutoApproveLevel::Read,
          &format!("Coloca produtos no Google. Primeiro o catalogo: confere com flexi_anuncios (marketplace 2, produto_id) se o produto ja tem anuncio no Google Shopping; se nao tem, obtem a conta em flexi_google_merchant_contas, cria o anuncio com flexi_criar_anuncio (dados: produto_id, marketplace 2, marketplace_conta_id, quantidade_anuncios 1) e envia ao Merchant Center com flexi_publicar_anuncio, que sempre pede permissao. Trabalha por lotes pequenos, produto a produto, e informa os ids criados. Campanha do Google Ads (flexi_ads_publicar_produtos) e um passo separado e opcional: ela usa o Merchant Center inteiro da conta e nao seleciona produtos por id; so cria uma se o gerente ou o usuario pedir, conferindo antes em flexi_ads_dashboard para nao duplicar, com nome, orcamento diario, ROAS alvo e status inicial PAUSED combinados. {common}"),
          &[]),
    ]
}

impl RuntimeService {
    /// Cria a equipe completa ligada a conexao, com delegacao configurada.
    pub async fn flexi_create_team(&self, connection_id: &str) -> DotsResult<serde_json::Value> {
        let conn = self.storage.call({ let id = connection_id.to_string(); move |s| s.apps().get(&id) }).await?;
        let engine = self.storage.call(|s| Ok(s.engines().list()?.into_iter().find(|e| e.enabled && e.last_detection.as_ref().map(|d| d.installed).unwrap_or(false)).map(|e| e.kind))).await?;
        let mut created: Vec<(String, String, &'static [&'static str])> = vec![];
        for tpl in templates() {
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
            let (did, cid, trust) = (dot.id.clone(), conn.id.clone(), tpl.trust);
            self.storage.call(move |s| {
                s.policies().update(&did, ToolPolicyUpdate { auto_approve_level: Some(trust), ..Default::default() })?;
                s.apps().set_integration(&did, KIND, Some(&cid), Some(tpl.id), Some(tpl.group))?;
                Ok(())
            }).await?;
            let ws = self.layout.workspaces_dir.join(&dot.id);
            std::fs::create_dir_all(&ws)?;
            let (did2, wsp) = (dot.id.clone(), ws.to_string_lossy().to_string());
            let _ = self.storage.call(move |s| s.dots().update("user", &did2, dots_core::DotUpdate { workspace_path: Some(Some(wsp)), ..Default::default() })).await;
            created.push((tpl.id.to_string(), dot.id.clone(), tpl.team));
        }
        // Equipes: uma por gerente, encaixadas (o subgerente e funcionario de "Marketplaces").
        let tpls = templates();
        let dot_of = |tid: &str| created.iter().find(|(t, _, _)| t == tid).map(|(_, d, _)| d.clone());
        let mut team_ids: Vec<String> = vec![];
        for tpl in tpls.iter().filter(|t| t.manages.is_some()) {
            let name = tpl.manages.unwrap();
            let members: Vec<String> = tpls.iter().filter(|m| m.member_of == Some(name)).filter_map(|m| dot_of(m.id)).collect();
            let input = TeamCreate { name: name.to_string(), description: format!("Equipe do Flexi criada em \"Montar equipe de marketplaces\" ({})", conn.label), manager_dot_id: dot_of(tpl.id), member_ids: members };
            let team = self.storage.call(move |s| s.teams().create(input)).await?;
            self.sync_team_public(&team).await?;
            team_ids.push(team.id);
        }
        self.events.emit("teams.changed", json!({ "ids": team_ids }));
        self.audit("app.flexi.create_team", "app_connection", Some(connection_id), json!({ "dots": created.len() })).await?;
        self.events.emit("dots.changed", json!({ "action": "team" }));
        Ok(json!({ "created": created.iter().map(|(t, d, _)| json!({ "template": t, "dot_id": d })).collect::<Vec<_>>() }))
    }
}

impl RuntimeService {
    /// Liga um DOT a uma conexao Flexi, guardando o papel de origem, a area e o cargo. A hierarquia de delegacao e
    /// feita pelas equipes (`teams.*`), nao aqui.
    pub async fn flexi_assign_role(&self, dot_id: &str, connection_id: &str, template_id: Option<&str>, group: Option<&str>, role: Option<DotRole>) -> DotsResult<DotIntegration> {
        let tpl = match template_id {
            Some(id) => Some(templates().into_iter().find(|t| t.id == id).ok_or_else(|| DotsError::Validation(format!("papel desconhecido: {id}")))?),
            None => None,
        };
        let group = group.or(tpl.as_ref().map(|t| t.group)).ok_or_else(|| DotsError::Validation("informe a area de trabalho".into()))?;
        let group: &'static str = GROUPS.iter().map(|(g, _)| *g).find(|g| *g == group).ok_or_else(|| DotsError::Validation(format!("area desconhecida: {group}")))?;
        let role = role.or(tpl.as_ref().map(|t| t.role)).unwrap_or(DotRole::Worker);
        let integ = self
            .storage
            .call({ let (d, c, t) = (dot_id.to_string(), connection_id.to_string(), tpl.as_ref().map(|t| t.id)); move |s| {
                s.dots().update("user", &d, dots_core::DotUpdate { role: Some(role), ..Default::default() })?;
                s.apps().set_integration(&d, KIND, Some(&c), t, Some(group))
            } })
            .await?
            .ok_or_else(|| DotsError::Validation("vinculo nao gravado".into()))?;
        self.audit("app.flexi.assign_role", "dot", Some(dot_id), json!({ "template": tpl.as_ref().map(|t| t.id), "group": group, "role": role, "connection_id": connection_id })).await?;
        self.events.emit("dots.changed", json!({ "id": dot_id, "action": "role" }));
        Ok(integ)
    }
}
