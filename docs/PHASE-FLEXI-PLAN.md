# Plano da Fase Flexi: integracao do DOTS com o Flexi (antes da Fase 4)

Data: 2026-10-15. Base: inspecao do codigo em `/Users/jone/Flexi/flexi2`, `/Users/jone/Flexi/microservice-marketplaces` e `/Users/jone/Flexi/flexi-lite`.

## Fatos verificados no Flexi (file:line no backend `flexi2/backend/src`)

- **Login**: `POST /api/auth/login` com `{email, senha, server?}` (`system/authPasswordRoutes.ts:46`). Resposta 200 `{status:1, user, empresa}` + cookie `userdata`
  (HttpOnly, AES, validade ~1-2 dias + sessao web do servidor). 202 com `mfa_required` -> `POST /api/auth/mfa/totp/login/verify {challenge_token, code}`.
  `GET /api/me` confirma a sessao. Nao ha refresh: ao 401, logar de novo. Cookie e Bearer nao podem ir juntos.
- **Tenant**: vem do token/cookie (`connection.base`, ex. `c2s_brasiltec`), devolvido pelo login legado a partir da conta do usuario. A tela de login do Flexi (`frontend/components/Login.tsx`) pede so e-mail e senha; o campo `server` e opcional e o DOTS nao o envia. (Corrigido em 2026-10-15 apos revisao do usuario.)
- **Tokens de integracao** (`fit1.`) so alcancam uma lista fixa de rotas (`server.ts:10151-10515`); rotas de marketplace nao estao nela. Portanto a integracao usa a sessao por cookie.
- **MCP do Flexi**: o servidor completo foi removido em 2026-10-14 (commit `7f54649ec`). Resta `/mcp` com uma unica ferramenta de eventos e somente OAuth. Nao serve para anuncios hoje.
- **Rotas de marketplace** (todas exigem usuario logado; prefixo `/api/gestao-marketplaces/anuncios/...`): consulta (`POST .../anuncios`, `/geral`, `/vendas`, `/historico-preco`, `/precos-calculados`),
  criacao (`/preparar-criacao`, `/criar`, `/criar-kit`, `/criar-familia-mercado-livre`, `/copiar-para-shopee`), publicacao (`/publicar`), conteudo (`/atualizar-conteudo`, `/atualizar-descricao`),
  ciclo de vida (`/pausar`, `/reativar`, `/finalizar`), preco/estoque (`/atualizar-preco`, `/atualizar-estoque`), promocoes (`/:id/promocoes`), moderacao, contas (`/api/marketplaces/mercadolivre/contas`,
  `/api/marketplaces/shopee/contas`, `.../health`), perguntas (`/api/perguntas-new/threads`, `.../answer`), analytics (`/api/analytics/marketplace/*`, relatorios), Google Ads (`/api/google/ads/dashboard/*`,
  `/campanhas/:id/pausar|status|orcamento|target-roas|nome|negative-keywords` com `confirm`). Varias escritas sao assincronas e devolvem `batch_id`/`operation_id` com rota `.../progresso`.
- **Auditoria do Flexi**: acoes ficam atribuidas ao usuario da sessao (`auditoria`, `produto_marketplace_auditoria_evento` com `origem_tipo/origem_nome/correlation_id`).
- **Regras de preco**: `flexi2/docs/regras-preco-anuncios-marketplaces.md` e obrigatorio antes de mudar preco (`AGENTS.md` do Flexi).

## Decisoes

1. **Conector Flexi dentro do `dotsd`** (sem alterar o repositorio do Flexi nesta fase). Cliente HTTP com cookie jar, login por e-mail/senha com 2FA, `GET /api/me` para confirmar usuario e empresa.
2. **Conexoes nomeadas, com o usuario que voce escolher**: tabela `app_connections(id, kind='flexi', label, base_url, server, email, user_json, empresa_json, status, last_login_at)`.
   Cookie e, opcionalmente, senha guardados **so no Keychain do macOS** (`keyring`), nunca no SQLite nem expostos aos CLIs. Guardar a senha e opcional e serve para reconectar sozinho quando a sessao expirar;
   sem ela, o DOTS pede para reconectar. 2FA sempre pede o codigo a voce (cartao na conversa).
3. **Ferramentas `flexi_*` pelo gateway que ja existe**, definidas por um catalogo (nome, metodo, rota, parametros, classe de risco, descricao). Classes: leitura (contas, anuncios, vendas, perguntas, analytics,
   dashboards), escrita (preco, conteudo, pausar/reativar, promocoes, responder pergunta, orcamento/status de campanha, estoque), critica (criar/publicar anuncio, finalizar, copiar para Shopee, confirmar
   desconto Shopee, modo ferias, remover conta). Escritas passam pela politica do DOT e pela central de aprovacoes; criticas sempre pedem permissao. Toda chamada e auditada no DOTS e, no Flexi, fica no nome
   do usuario conectado.
4. **Cada DOT escolhe a conexao Flexi** que usa (`dot_integrations`); se houver uma so, e automatica. Um DOT sem conexao nao ve as ferramentas `flexi_*`.
5. **Modelos de DOT para o Flexi** (aparecem quando ha conexao), organizados como equipe com delegacao ja configurada:
   - Gerente de Marketplaces (supervisor; equipe: subgerentes)
   - SubGerente de Mercado Livre (supervisor; equipe: agentes ML) · Agente de Mercado Livre: Criador de anuncios · Agente de Mercado Livre: Revisor de anuncios · Agente de Mercado Livre: Atendimento de perguntas
   - SubGerente de Shopee (supervisor) · Agente de Shopee: Anuncios e estoque · Agente de Shopee: Promocoes
   - Gerente de Google Ads (supervisor) · Agente de Google Ads: Campanhas
   - Agente de Publicacao (publica anuncios ja revisados; sempre com permissao)
   Acao "Montar equipe de marketplaces" cria todos de uma vez, ligados a conexao, com os papeis e permissoes de delegacao. Nomes editaveis.
   As instrucoes dos papeis de preco exigem usar `precos-calculados` e nunca ficar abaixo do preco minimo; um resumo das regras de preco do Flexi vai no modelo, com a observacao de que o documento
   completo esta no repositorio do Flexi.
6. **"Conectar aplicativos"** ganha o cartao Flexi: conectar (servidor/empresa, e-mail, senha, 2FA), status (usuario, empresa, sessao valida ate), reconectar, remover, "guardar senha para reconectar".
   URL padrao de producao `https://flexi.c2s-business.com`; em "Avancado", URL local (`http://127.0.0.1:2012`).

## Etapas

1. Crate `dots-flexi` (cliente HTTP, login/2FA/me, cookie jar, erros claros) com testes contra um servidor Flexi falso (fixture HTTP em Python que imita login, MFA, me, 401 e as rotas de anuncios).
2. Migracao 0004: `app_connections`, `dot_integrations`; Keychain via `keyring`; metodos IPC `apps.flexi.connect|verify_mfa|status|reconnect|remove`, `dots.set_integration`.
3. Catalogo de ferramentas `flexi_*` (~35 ferramentas na v1) com risco e parametros; execucao generica no gateway; tratamento de `batch_id`/`operation_id` (ferramenta `flexi_progresso`); limite de tamanho de resposta.
4. Instrucoes por papel e modelos de equipe; "Montar equipe de marketplaces".
5. UI: cartao Flexi em Conectar aplicativos (com 2FA), aba Aplicativos no perfil do DOT, modelos Flexi no Novo DOT, cartao de reconexao na conversa.
6. E2E com Flexi falso: conectar, 2FA, DOT lista anuncios, muda preco com aprovacao, publicacao exige permissao, sessao expirada -> reconectar; Supervisor de marketplaces delegando a dois agentes.
7. Validacao real: voce conecta ao Flexi (local em 2012 ou producao) pela tela e pede a um Agente de Mercado Livre uma consulta de anuncios; depois uma alteracao de preco com aprovacao.
8. Docs (`docs/PHASE-FLEXI.md`, ADRs), limitacoes.

## Riscos e limites

- A sessao por cookie expira em 1-2 dias; sem senha guardada, o DOT para ate voce reconectar. Com senha guardada, reconecta sozinho, mas 2FA continua exigindo o codigo.
- As rotas do Flexi nao tem contrato estavel para terceiros; mudancas no backend podem quebrar ferramentas. O catalogo isola isso e os testes com o servidor falso detectam formatos.
- Escritas em marketplaces afetam anuncios reais. Por isso tudo o que escreve passa pela politica do DOT e pela central; criticas sempre pedem permissao.
- O MCP do Flexi foi removido; se um dia voltar com ferramentas de marketplace, o conector pode ser trocado por ele sem mudar os DOTs.
