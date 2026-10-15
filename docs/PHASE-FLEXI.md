# Fase Flexi: integracao do DOTS com o Flexi (entre a Fase 3 e a Fase 4)

Data: 2026-10-15. Plano aprovado em `docs/PHASE-FLEXI-PLAN.md`. Nenhuma alteracao no repositorio do Flexi.

## O que foi entregue

- **Crate `dots-flexi`**: cliente HTTP do Flexi (`login` com `{email, senha, server}`, 2FA por `challenge_token`, `GET /api/me`,
  `logout`, chamada generica com cookie `userdata`), armazenamento de segredos (`KeychainStore` via `keyring`, servico
  `com.c2s.dots`; `MemoryStore` para testes com `DOTS_SECRETS=memory`) e **catalogo de 49 ferramentas `flexi_*`** com metodo,
  rota, parametros, classe de risco e grupo. Testes com um Flexi falso (`crates/dots-flexi/tests/fixtures/fake_flexi.py`).
- **Conexoes nomeadas** (`app_connections`, migracao `0005_apps.sql`) com o usuario do Flexi que voce escolher, nao
  necessariamente o seu. Cookie e senha ficam **somente no Keychain** (chaves `flexi:<id>:cookie|password`); o SQLite guarda
  e-mail, usuario, empresa, base e estado (`connected`, `needs_login`, `mfa_pending`, `error`). Guardar a senha e uma opcao
  marcavel na tela de conexao; serve para reconectar sozinho quando a sessao expirar. O codigo 2FA e sempre pedido a voce.
- **Vinculo DOT -> conexao** (`dot_integrations`): aba "Aplicativos" no perfil do DOT. Com uma unica conexao, o vinculo e
  automatico. DOT sem conexao nao ve as ferramentas `flexi_*`.
- **Ferramentas pelo gateway MCP existente**: `tools/list` passa a ser dinamico (`gateway.tools`), e `execute_tool`
  despacha `flexi_*` para `flexi_execute`: monta a requisicao pelo catalogo, chama o Flexi, repete uma vez apos relogin
  quando a sessao expirou (401), trata `status: 0` do Flexi como erro de ferramenta e registra escritas em `workspace_changes`.
  Risco: leitura passa pela politica do DOT; escrita exige `auto_approve_level` >= `write` ou aprovacao; critica
  (`flexi_mercadolivre_modo_ferias`, `flexi_finalizar_anuncios`, `flexi_criar_anuncio`, `flexi_publicar_anuncio`,
  `flexi_copiar_para_shopee`, `flexi_ads_publicar_produtos`) sempre pede permissao. Rotas POST do Google Ads recebem `confirm: true` automaticamente.
- **Google Shopping** (verificado no backend do Flexi em 2026-10-15): marketplace `2` = Google Merchant Center
  (`GOOGLE_MERCHANT_MARKETPLACE_ID`); `anuncios/criar` aceita `google-shopping` (payload `produto_id`, `marketplace: 2`,
  `marketplace_conta_id` = id da conta de `/api/google/merchant/accounts` cujo id e o proprio `customer_id`,
  `quantidade_anuncios`), `preparar-criacao` so ML/Shopee, `publicar` envia o item ao Merchant Center
  (`publicarGoogleMerchantProdutoMarketplace`). Ferramenta `flexi_google_merchant_contas` lista as contas. Todo DOT ligado
  ao Flexi recebe `FLEXI_CONTEXT` nas instrucoes (`crates/dots-runtime/src/flexi.rs`): numeros dos marketplaces e a
  distincao entre anuncio no Google Shopping (catalogo) e campanha no Google Ads (midia); os papeis do Google
  (`ads_manager`, `ads_agent`, `publisher`) foram reescritos com esse entendimento.
- **Modelos de papel para marketplaces** (`apps.flexi.templates`) e **"Montar equipe de marketplaces"** (`apps.flexi.create_team`):
  Gerente de Marketplaces (supervisor) -> SubGerente de Mercado Livre, SubGerente de Shopee, Gerente de Google Ads
  (supervisores) -> Agente de Mercado Livre: Criador de anuncios / Revisor de anuncios / Atendimento de perguntas;
  Agente de Shopee: Anuncios e estoque / Promocoes; Agente de Google Ads: Campanhas / Publicacao (cria campanhas
  Shopping ou Performance Max para publicar produtos via `flexi_ads_publicar_produtos`).
  Todos nascem ligados a conexao, com politica `read`, delegacao configurada e instrucoes com as regras de preco resumidas.
- **Equipes** (ADR 0014, migracao `0008_teams.sql`, metodos `teams.*` e `dots.team`): o usuario cria equipes com um gerente
  e funcionarios; equipes se encaixam (o gerente de "Mercado Livre" e funcionario de "Marketplaces"). O runtime deriva a
  delegacao: o gerente pode pedir trabalho exatamente aos funcionarios da equipe dele e vira supervisor. "Montar equipe de
  marketplaces" cria as quatro equipes ja encaixadas. No Novo DOT, o passo 3 pergunta se e Gerente ou Funcionario e, em
  seguida, "gerente do que" (equipe sem gerente ou nova) ou "funcionario de quem" (equipe, mostrando o gerente). Os papeis
  do Flexi pre-preenchem isso. O passo final mostra, com o Flexi conectado, "Somente consulta" (`read`) ou "Pode editar"
  (`write`); acoes criticas sempre pedem permissao. `apps.flexi.assign_role` so registra papel de origem e area
  (`role_template`, `work_group`, migracoes 0006/0007), sem montar hierarquia.
- **Interface**: cartao Flexi em "Conectar aplicativos" (e-mail e senha do usuario; a empresa e a base vem da conta, como no login do Flexi; "guardar senha no Keychain", avancado com
  endereco do Flexi, codigo 2FA, status, verificar, entrar de novo, remover, montar equipe); aba Aplicativos no perfil;
  no "Novo DOT", com o Flexi conectado os modelos passam a ser os papeis de marketplaces; na barra lateral, aviso ambar
  quando uma conexao precisa de atencao (evento `apps.changed`).

## Metodos IPC novos

`apps.list`, `apps.flexi.connect`, `apps.flexi.reconnect`, `apps.flexi.mfa`, `apps.flexi.check`, `apps.flexi.remove`,
`apps.flexi.templates`, `apps.flexi.create_team`, `apps.flexi.assign_role`, `teams.list|get|create|update|delete|set_manager|add_member|remove_member`, `dots.team`, `dots.integration`, `dots.set_integration`, `gateway.tools` (interno, com token
de execucao).

## Como validar

- Automatizado: `cargo test -p dots-flexi` (cliente, 2FA, expiracao), `cargo test -p dots-storage` (repositorio),
  `cargo test -p dotsd --test flexi_e2e` (conectar com senha errada e certa, ferramentas na lista, leitura de anuncios e precos,
  alteracao de preco com aprovacao, recusa do Flexi abaixo do minimo, publicacao critica, Google Ads com `confirm`,
  sessao expirada com relogin automatico, auditoria, remocao da conexao; 2FA e criacao da equipe com 11 DOTs).
  O CLI falso aceita `FLEXI:<ferramenta>:<json>` e `TOOLS`.
- Manual: em Conectar aplicativos, conectar ao Flexi local (`http://127.0.0.1:2012`, em Avancado) ou producao; montar a
  equipe; pedir ao Revisor de anuncios uma consulta; pedir uma alteracao de preco e aprovar o cartao na conversa.

## Limites conhecidos

- Sessao por cookie expira em um ou dois dias. Sem senha guardada, o DOT para ate voce entrar de novo. Com 2FA ativo, o
  relogin automatico fica em `mfa_pending` ate voce informar o codigo.
- As rotas do Flexi nao sao um contrato publico; mudancas no backend podem quebrar ferramentas. O catalogo concentra isso.
- Escritas assincronas do Flexi devolvem `batch_id`/`operation_id`; o DOT consulta o andamento com `flexi_jobs_status`.
- `flexi_api` (GET generico sob `/api/`) existe para consultas nao cobertas; e sempre leitura.
