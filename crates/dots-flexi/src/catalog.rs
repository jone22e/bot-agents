//! Catalogo de ferramentas `flexi_*`: nome, rota verificada, parametros, risco.
//! Rotas conferidas em `flexi2/backend/src` em 2026-10-15 (ver docs/PHASE-FLEXI-PLAN.md).

use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk {
    Read,
    Write,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamsIn {
    Query,
    Body,
}

#[derive(Debug, Clone)]
pub struct FlexiTool {
    pub name: &'static str,
    pub description: &'static str,
    pub method: &'static str,
    /// Rota; `{id}` e substituido pelo parametro `id`.
    pub path: &'static str,
    pub params_in: ParamsIn,
    /// (nome, tipo JSON Schema, descricao, obrigatorio)
    pub params: &'static [(&'static str, &'static str, &'static str, bool)],
    pub risk: ToolRisk,
    /// Grupo: contas | anuncios | perguntas | analytics | googleads | produtos
    pub group: &'static str,
}

const R: ToolRisk = ToolRisk::Read;
const W: ToolRisk = ToolRisk::Write;
const C: ToolRisk = ToolRisk::Critical;

pub fn catalog() -> &'static [FlexiTool] {
    &[
        // ---- contas de marketplace
        FlexiTool { name: "flexi_contas_mercadolivre", description: "Lista as contas Mercado Livre conectadas (nome, ambiente, validade do token, reputacao, modo ferias).", method: "GET", path: "/api/marketplaces/mercadolivre/contas", params_in: ParamsIn::Query, params: &[], risk: R, group: "contas" },
        FlexiTool { name: "flexi_contas_shopee", description: "Lista as contas Shopee conectadas.", method: "GET", path: "/api/marketplaces/shopee/contas", params_in: ParamsIn::Query, params: &[], risk: R, group: "contas" },
        FlexiTool { name: "flexi_contas_marketplaces", description: "Lista todas as contas de marketplace (codigo: 0 Mercado Livre, 1 Shopee, 4 Amazon, 5 ML Global).", method: "GET", path: "/api/marketplaces/contas/list", params_in: ParamsIn::Query, params: &[], risk: R, group: "contas" },
        FlexiTool { name: "flexi_shopee_saude_conta", description: "Saude da conta Shopee (metricas de performance).", method: "GET", path: "/api/marketplaces/shopee/contas/{id}/health", params_in: ParamsIn::Query, params: &[("id", "integer", "id da conta Shopee no Flexi", true)], risk: R, group: "contas" },
        FlexiTool { name: "flexi_mercadolivre_modo_ferias", description: "Liga/desliga o modo ferias de uma conta Mercado Livre.", method: "POST", path: "/api/marketplaces/mercadolivre/contas/{id}/modo-ferias", params_in: ParamsIn::Body, params: &[("id", "integer", "id da conta", true), ("ativo", "boolean", "true para ativar", true)], risk: C, group: "contas" },
        // ---- anuncios: consulta
        FlexiTool { name: "flexi_anuncios", description: "Consulta anuncios do Flexi. Filtre por marketplace (0 ML, 1 Shopee, 2 Google Shopping), marketplace_conta_id, produto_id, produto_familia_marketplace_id ou ids (lista de ids de anuncio). Retorna titulo, preco, estoque, status, vendas, visitas, ids externos.", method: "POST", path: "/api/gestao-marketplaces/anuncios", params_in: ParamsIn::Body, params: &[("marketplace", "integer", "0 Mercado Livre, 1 Shopee, 2 Google Shopping", false), ("marketplace_conta_id", "integer", "", false), ("produto_id", "integer", "", false), ("produto_familia_marketplace_id", "integer", "", false), ("ids", "array", "ids de anuncio (produto_marketplace.id)", false), ("incluir_preco_ultima_venda", "integer", "1 para incluir", false)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_anuncios_geral", description: "Visao geral dos anuncios (totais e status por marketplace/conta).", method: "POST", path: "/api/gestao-marketplaces/anuncios/geral", params_in: ParamsIn::Body, params: &[("marketplace", "integer", "", false), ("marketplace_conta_id", "integer", "", false)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_anuncios_vendas", description: "Vendas por anuncio.", method: "POST", path: "/api/gestao-marketplaces/anuncios/vendas", params_in: ParamsIn::Body, params: &[("ids", "array", "ids de anuncio", false), ("marketplace", "integer", "", false), ("marketplace_conta_id", "integer", "", false)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_anuncios_falha", description: "Anuncios com falha de publicacao/sincronizacao.", method: "POST", path: "/api/gestao-marketplaces/anuncios/falha", params_in: ParamsIn::Body, params: &[("marketplace", "integer", "", false), ("marketplace_conta_id", "integer", "", false)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_precos_calculados", description: "Precos calculados pelo Flexi para anuncios (preco sugerido, preco minimo, taxas, frete). Consulte antes de alterar qualquer preco; nunca va abaixo do preco minimo.", method: "GET", path: "/api/gestao-marketplaces/anuncios/precos-calculados", params_in: ParamsIn::Query, params: &[("ids", "string", "ids de anuncio separados por virgula", false), ("marketplace_conta_id", "integer", "", false)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_historico_preco", description: "Historico de precos de anuncios.", method: "POST", path: "/api/gestao-marketplaces/anuncios/historico-preco", params_in: ParamsIn::Body, params: &[("ids", "array", "ids de anuncio", true)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_moderacao_itens", description: "Itens em moderacao/infracao nos marketplaces.", method: "POST", path: "/api/gestao-marketplaces/anuncios/moderacao/itens", params_in: ParamsIn::Body, params: &[("marketplace", "integer", "", false), ("marketplace_conta_id", "integer", "", false)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_estoque_inconsistencias", description: "Anuncios com estoque divergente entre Flexi e marketplace.", method: "GET", path: "/api/gestao-marketplaces/anuncios/estoque-inconsistencias", params_in: ParamsIn::Query, params: &[("marketplace_conta_id", "integer", "", false)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_jobs_status", description: "Status de operacoes assincronas (batch_id/operation_id devolvidos por acoes de anuncio).", method: "GET", path: "/api/gestao-marketplaces/jobs/status", params_in: ParamsIn::Query, params: &[("batch_id", "string", "", false), ("operation_id", "string", "", false)], risk: R, group: "anuncios" },
        // ---- anuncios: escrita
        FlexiTool { name: "flexi_atualizar_preco", description: "Altera o preco de um anuncio. Requer consulta previa de flexi_precos_calculados; preco nao pode ficar abaixo de preco_minimo.", method: "POST", path: "/api/gestao-marketplaces/anuncios/atualizar-preco", params_in: ParamsIn::Body, params: &[("id", "integer", "id do anuncio", true), ("preco", "number", "novo preco", true), ("preco_minimo", "number", "", false), ("expected_version", "integer", "versao esperada (controle de concorrencia)", false)], risk: W, group: "anuncios" },
        FlexiTool { name: "flexi_atualizar_conteudo", description: "Atualiza titulo/atributos de um anuncio.", method: "POST", path: "/api/gestao-marketplaces/anuncios/atualizar-conteudo", params_in: ParamsIn::Body, params: &[("id", "integer", "id do anuncio", true), ("nome", "string", "novo titulo", false), ("atributos", "object", "", false)], risk: W, group: "anuncios" },
        FlexiTool { name: "flexi_atualizar_descricao", description: "Atualiza a descricao de um anuncio.", method: "POST", path: "/api/gestao-marketplaces/anuncios/atualizar-descricao", params_in: ParamsIn::Body, params: &[("id", "integer", "id do anuncio", true), ("descricao", "string", "", true)], risk: W, group: "anuncios" },
        FlexiTool { name: "flexi_atualizar_estoque", description: "Envia o estoque do Flexi para o marketplace (nao aceita quantidade manual).", method: "POST", path: "/api/gestao-marketplaces/anuncios/atualizar-estoque", params_in: ParamsIn::Body, params: &[("id", "integer", "id do anuncio", true)], risk: W, group: "anuncios" },
        FlexiTool { name: "flexi_pausar_anuncios", description: "Pausa anuncios (lista de ids). Devolve batch_id para acompanhar.", method: "POST", path: "/api/gestao-marketplaces/anuncios/pausar", params_in: ParamsIn::Body, params: &[("ids", "array", "ids de anuncio", true)], risk: W, group: "anuncios" },
        FlexiTool { name: "flexi_reativar_anuncios", description: "Reativa anuncios pausados.", method: "POST", path: "/api/gestao-marketplaces/anuncios/reativar", params_in: ParamsIn::Body, params: &[("ids", "array", "ids de anuncio", true)], risk: W, group: "anuncios" },
        FlexiTool { name: "flexi_finalizar_anuncios", description: "FINALIZA anuncios (irreversivel no marketplace).", method: "POST", path: "/api/gestao-marketplaces/anuncios/finalizar", params_in: ParamsIn::Body, params: &[("ids", "array", "ids de anuncio", true)], risk: C, group: "anuncios" },
        FlexiTool { name: "flexi_promocoes_anuncio", description: "Lista promocoes de um anuncio.", method: "GET", path: "/api/gestao-marketplaces/anuncios/{id}/promocoes", params_in: ParamsIn::Query, params: &[("id", "integer", "id do anuncio", true)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_preparar_criacao", description: "Prepara a criacao de anuncios no Mercado Livre ou Shopee a partir de produtos (validacoes, categoria, atributos). Passo 1 de 2. O Google Shopping (marketplace 2) nao usa este passo: va direto a flexi_criar_anuncio.", method: "POST", path: "/api/gestao-marketplaces/anuncios/preparar-criacao", params_in: ParamsIn::Body, params: &[("produto_id", "integer", "", false), ("produto_familia_marketplace_id", "integer", "", false), ("marketplace", "integer", "0 ML, 1 Shopee", true), ("marketplace_conta_id", "integer", "", true)], risk: R, group: "anuncios" },
        FlexiTool { name: "flexi_criar_anuncio", description: "Cria o anuncio no Flexi (passo 2 de 2; no ML e Shopee use os dados de flexi_preparar_criacao). Google Shopping: dados {produto_id, marketplace: 2, marketplace_conta_id: '<id da conta de flexi_google_merchant_contas, so digitos>', quantidade_anuncios: 1}. Nao publica no marketplace.", method: "POST", path: "/api/gestao-marketplaces/anuncios/criar", params_in: ParamsIn::Body, params: &[("dados", "object", "payload: produto_id, marketplace (0 ML, 1 Shopee, 2 Google Shopping), marketplace_conta_id (Shopee/Google) ou user_id (ML), quantidade_anuncios, listing_type_id (ML)", true)], risk: C, group: "anuncios" },
        FlexiTool { name: "flexi_publicar_anuncio", description: "PUBLICA um anuncio no marketplace (ML, Shopee ou Google Shopping: envia o item ao Merchant Center). Acao irreversivel; sempre pede permissao.", method: "POST", path: "/api/gestao-marketplaces/anuncios/publicar", params_in: ParamsIn::Body, params: &[("id", "integer", "id do anuncio", true), ("validar_antes_publicar", "integer", "1 para validar", false)], risk: C, group: "anuncios" },
        FlexiTool { name: "flexi_copiar_para_shopee", description: "Copia um anuncio do Mercado Livre para a Shopee.", method: "POST", path: "/api/gestao-marketplaces/anuncios/copiar-para-shopee", params_in: ParamsIn::Body, params: &[("ids", "array", "ids de anuncio ML", true), ("marketplace_conta_id", "integer", "conta Shopee destino", true)], risk: C, group: "anuncios" },
        // ---- perguntas e mensagens
        FlexiTool { name: "flexi_perguntas", description: "Lista conversas/perguntas de compradores (Mercado Livre e Shopee) pendentes ou recentes.", method: "GET", path: "/api/perguntas-new/threads", params_in: ParamsIn::Query, params: &[("status", "string", "ex.: pending", false), ("page", "integer", "", false), ("page_size", "integer", "", false), ("marketplace_conta_id", "integer", "", false)], risk: R, group: "perguntas" },
        FlexiTool { name: "flexi_pergunta_detalhe", description: "Detalhe de uma conversa com o comprador.", method: "GET", path: "/api/perguntas-new/threads/{id}", params_in: ParamsIn::Query, params: &[("id", "integer", "id da conversa", true)], risk: R, group: "perguntas" },
        FlexiTool { name: "flexi_pergunta_contexto", description: "Contexto (anuncio, produto, historico) de uma conversa.", method: "GET", path: "/api/perguntas-new/threads/{id}/context", params_in: ParamsIn::Query, params: &[("id", "integer", "id da conversa", true)], risk: R, group: "perguntas" },
        FlexiTool { name: "flexi_responder_pergunta", description: "Responde ao comprador (publica a resposta no marketplace).", method: "POST", path: "/api/perguntas-new/threads/{id}/answer", params_in: ParamsIn::Body, params: &[("id", "integer", "id da conversa", true), ("resposta", "string", "texto da resposta", true)], risk: W, group: "perguntas" },
        FlexiTool { name: "flexi_perguntas_metricas", description: "Metricas de atendimento (tempo de resposta, pendentes).", method: "GET", path: "/api/perguntas-new/metrics", params_in: ParamsIn::Query, params: &[], risk: R, group: "perguntas" },
        // ---- analytics
        FlexiTool { name: "flexi_dashboard_marketplace", description: "Resumo do dashboard de marketplaces (vendas, pedidos, ticket).", method: "GET", path: "/api/analytics/marketplace/dashboard/resumo", params_in: ParamsIn::Query, params: &[("ini", "string", "data inicial YYYY-MM-DD", false), ("fin", "string", "data final", false), ("marketplace_conta_id", "integer", "", false)], risk: R, group: "analytics" },
        FlexiTool { name: "flexi_lucro_analise", description: "Analise de lucro por anuncio/periodo.", method: "GET", path: "/api/analytics/marketplace/lucro/analise", params_in: ParamsIn::Query, params: &[("ini", "string", "", false), ("fin", "string", "", false), ("marketplace_conta_id", "integer", "", false)], risk: R, group: "analytics" },
        FlexiTool { name: "flexi_conversao_analise", description: "Analise de conversao (visitas x vendas).", method: "GET", path: "/api/analytics/marketplace/conversao/analise", params_in: ParamsIn::Query, params: &[("ini", "string", "", false), ("fin", "string", "", false), ("marketplace_conta_id", "integer", "", false)], risk: R, group: "analytics" },
        FlexiTool { name: "flexi_curva_abc", description: "Relatorio curva ABC de anuncios.", method: "GET", path: "/api/gestao-marketplaces/relatorios/curva-abc", params_in: ParamsIn::Query, params: &[("ini", "string", "", false), ("fin", "string", "", false)], risk: R, group: "analytics" },
        FlexiTool { name: "flexi_shopee_buybox", description: "Resumo de buybox Shopee.", method: "GET", path: "/api/marketplaces/shopee/buybox/resumo", params_in: ParamsIn::Query, params: &[], risk: R, group: "analytics" },
        // ---- google ads
        FlexiTool { name: "flexi_ads_status", description: "Status da conexao com o Google Ads.", method: "GET", path: "/api/google/ads/auth/status", params_in: ParamsIn::Query, params: &[], risk: R, group: "googleads" },
        FlexiTool { name: "flexi_ads_contas", description: "Contas Google Ads configuradas (contaId, customer_id).", method: "GET", path: "/api/google/ads/contas", params_in: ParamsIn::Query, params: &[], risk: R, group: "googleads" },
        FlexiTool { name: "flexi_google_merchant_contas", description: "Contas do Google Merchant Center conhecidas pelo Flexi (id, name, customer_id, source). Para criar anuncio no Google Shopping (marketplace 2) use como marketplace_conta_id o id da conta cujo id e o proprio customer_id.", method: "GET", path: "/api/google/merchant/accounts", params_in: ParamsIn::Query, params: &[], risk: R, group: "googleads" },
        FlexiTool { name: "flexi_ads_dashboard", description: "Dashboard geral de campanhas (custo, conversoes, ROAS) por periodo.", method: "GET", path: "/api/google/ads/dashboard/geral", params_in: ParamsIn::Query, params: &[("contaId", "integer", "", true), ("ini", "string", "YYYY-MM-DD", true), ("fin", "string", "YYYY-MM-DD", true), ("campanha", "string", "filtro por campanha", false)], risk: R, group: "googleads" },
        FlexiTool { name: "flexi_ads_series", description: "Series temporais das campanhas.", method: "GET", path: "/api/google/ads/dashboard/series", params_in: ParamsIn::Query, params: &[("contaId", "integer", "", true), ("ini", "string", "", true), ("fin", "string", "", true)], risk: R, group: "googleads" },
        FlexiTool { name: "flexi_ads_palavras_chave", description: "Desempenho por palavra-chave.", method: "GET", path: "/api/google/ads/dashboard/palavras-chave", params_in: ParamsIn::Query, params: &[("contaId", "integer", "", true), ("ini", "string", "", true), ("fin", "string", "", true)], risk: R, group: "googleads" },
        FlexiTool { name: "flexi_ads_produtos", description: "Desempenho por produto nas campanhas.", method: "GET", path: "/api/google/ads/dashboard/produtos", params_in: ParamsIn::Query, params: &[("contaId", "integer", "", true), ("ini", "string", "", true), ("fin", "string", "", true)], risk: R, group: "googleads" },
        FlexiTool { name: "flexi_ads_pausar_campanha", description: "Pausa uma campanha do Google Ads.", method: "POST", path: "/api/google/ads/dashboard/campanhas/{id}/pausar", params_in: ParamsIn::Body, params: &[("id", "string", "campaignId", true), ("contaId", "integer", "", true), ("campaignName", "string", "", true)], risk: W, group: "googleads" },
        FlexiTool { name: "flexi_ads_status_campanha", description: "Altera o status de uma campanha (ENABLED/PAUSED).", method: "POST", path: "/api/google/ads/dashboard/campanhas/{id}/status", params_in: ParamsIn::Body, params: &[("id", "string", "campaignId", true), ("contaId", "integer", "", true), ("campaignName", "string", "", true), ("status", "string", "ENABLED ou PAUSED", true)], risk: W, group: "googleads" },
        FlexiTool { name: "flexi_ads_orcamento_campanha", description: "Altera o orcamento diario de uma campanha.", method: "POST", path: "/api/google/ads/dashboard/campanhas/{id}/orcamento", params_in: ParamsIn::Body, params: &[("id", "string", "campaignId", true), ("contaId", "integer", "", true), ("campaignName", "string", "", true), ("valor", "number", "orcamento diario", true)], risk: W, group: "googleads" },
        FlexiTool { name: "flexi_ads_target_roas", description: "Altera o ROAS alvo de uma campanha.", method: "POST", path: "/api/google/ads/dashboard/campanhas/{id}/target-roas", params_in: ParamsIn::Body, params: &[("id", "string", "campaignId", true), ("contaId", "integer", "", true), ("campaignName", "string", "", true), ("valor", "number", "ROAS alvo", true)], risk: W, group: "googleads" },
        FlexiTool { name: "flexi_ads_publicar_produtos", description: "Cria uma CAMPANHA nova no Google Ads (SHOPPING ou PERFORMANCE_MAX) com o Merchant Center inteiro vinculado a conta; nao escolhe produtos por id. Para colocar um produto no Google, antes crie e publique o anuncio dele no Google Shopping (flexi_criar_anuncio marketplace 2 + flexi_publicar_anuncio). Acao irreversivel; sempre pede permissao.", method: "POST", path: "/api/google/ads/campanhas/publicar-produtos", params_in: ParamsIn::Body, params: &[("contaId", "integer", "", true), ("name", "string", "nome da campanha", true), ("dailyBudget", "number", "orcamento diario, maior que zero", true), ("type", "string", "SHOPPING ou PERFORMANCE_MAX (padrao)", false), ("targetRoas", "number", "ROAS alvo", false), ("initialStatus", "string", "ENABLED ou PAUSED (padrao)", false), ("finalUrl", "string", "URL final", false), ("businessName", "string", "nome da empresa", false), ("merchantCenterId", "string", "se a conta tiver mais de um Merchant Center", false)], risk: C, group: "googleads" },
        FlexiTool { name: "flexi_ads_negative_keywords", description: "Adiciona palavras-chave negativas a uma campanha.", method: "POST", path: "/api/google/ads/dashboard/campanhas/{id}/negative-keywords", params_in: ParamsIn::Body, params: &[("id", "string", "campaignId", true), ("contaId", "integer", "", true), ("campaignName", "string", "", true), ("keywords", "array", "lista de termos", true)], risk: W, group: "googleads" },
        // ---- produtos
        FlexiTool { name: "flexi_api", description: "Chamada generica de LEITURA (GET) a uma rota do Flexi sob /api/ nao coberta pelas outras ferramentas. Use so quando necessario.", method: "GET", path: "{path}", params_in: ParamsIn::Query, params: &[("path", "string", "rota comecando com /api/", true), ("query", "object", "parametros de consulta", false)], risk: R, group: "geral" },
    ]
}

pub fn find_tool(name: &str) -> Option<&'static FlexiTool> {
    catalog().iter().find(|t| t.name == name)
}

/// Definicoes MCP (`tools/list`) das ferramentas Flexi.
pub fn tool_definitions() -> Vec<Value> {
    catalog()
        .iter()
        .map(|t| {
            let mut props = serde_json::Map::new();
            let mut required = vec![];
            for (n, ty, desc, req) in t.params {
                let mut p = json!({ "type": ty });
                if *ty == "array" {
                    p["items"] = json!({});
                }
                if !desc.is_empty() {
                    p["description"] = Value::String(desc.to_string());
                }
                props.insert(n.to_string(), p);
                if *req {
                    required.push(Value::String(n.to_string()));
                }
            }
            let risk = match t.risk { ToolRisk::Read => "leitura", ToolRisk::Write => "altera dados (pode pedir permissao)", ToolRisk::Critical => "acao critica (sempre pede permissao)" };
            json!({ "name": t.name, "description": format!("[Flexi · {}] {}", risk, t.description), "inputSchema": { "type": "object", "properties": props, "required": required } })
        })
        .collect()
}

/// Separa os argumentos em (caminho resolvido, query, body) conforme a ferramenta.
pub fn build_request(tool: &FlexiTool, args: &Value) -> Result<(String, Option<Value>, Option<Value>), String> {
    let obj = args.as_object().cloned().unwrap_or_default();
    for (n, _, _, req) in tool.params {
        if *req && !obj.contains_key(*n) {
            return Err(format!("parametro obrigatorio ausente: {n}"));
        }
    }
    let mut path = tool.path.to_string();
    let mut rest = serde_json::Map::new();
    for (k, v) in obj {
        if k == "id" && path.contains("{id}") {
            path = path.replace("{id}", &v.as_str().map(String::from).unwrap_or_else(|| v.to_string()));
        } else if k == "path" && path == "{path}" {
            let p = v.as_str().unwrap_or("");
            if !p.starts_with("/api/") || p.contains("..") || p.contains("?") {
                return Err("path deve comecar com /api/ e nao conter '..' ou '?'".into());
            }
            path = p.to_string();
        } else if k == "query" && tool.name == "flexi_api" {
            if let Value::Object(q) = v {
                rest.extend(q);
            }
        } else if k == "dados" && tool.name == "flexi_criar_anuncio" {
            if let Value::Object(d) = v {
                rest.extend(d);
            }
        } else {
            rest.insert(k, v);
        }
    }
    if path.contains('{') {
        return Err("parametro de rota ausente".into());
    }
    // Google Ads exige confirm=true explicito nas escritas.
    if tool.group == "googleads" && tool.method == "POST" {
        rest.insert("confirm".into(), Value::Bool(true));
    }
    let rest = Value::Object(rest);
    Ok(match tool.params_in {
        ParamsIn::Query => (path, Some(rest), None),
        ParamsIn::Body => (path, None, Some(rest)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_unique_and_definitions_valid() {
        let mut seen = std::collections::HashSet::new();
        for t in catalog() {
            assert!(seen.insert(t.name), "duplicado {}", t.name);
            assert!(t.name.starts_with("flexi_"));
        }
        assert!(tool_definitions().len() > 30);
    }

    #[test]
    fn builds_requests() {
        let t = find_tool("flexi_pergunta_detalhe").unwrap();
        let (p, q, b) = build_request(t, &json!({ "id": 42 })).unwrap();
        assert_eq!(p, "/api/perguntas-new/threads/42");
        assert!(q.is_some() && b.is_none());
        let t = find_tool("flexi_ads_pausar_campanha").unwrap();
        let (p, _, b) = build_request(t, &json!({ "id": "123", "contaId": 1, "campaignName": "X" })).unwrap();
        assert_eq!(p, "/api/google/ads/dashboard/campanhas/123/pausar");
        assert_eq!(b.unwrap()["confirm"], true);
        assert!(build_request(find_tool("flexi_atualizar_preco").unwrap(), &json!({ "id": 1 })).is_err(), "preco obrigatorio");
        assert!(build_request(find_tool("flexi_api").unwrap(), &json!({ "path": "/etc/passwd" })).is_err());
        let (p, q, _) = build_request(find_tool("flexi_api").unwrap(), &json!({ "path": "/api/produtos", "query": { "busca": "x" } })).unwrap();
        assert_eq!(p, "/api/produtos");
        assert_eq!(q.unwrap()["busca"], "x");
    }
}
