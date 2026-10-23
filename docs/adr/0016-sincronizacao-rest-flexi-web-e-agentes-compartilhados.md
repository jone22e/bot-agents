# ADR 0016: sincronizacao REST com o Flexi web e agentes compartilhados

Data: 2026-10-22. Estado: aceito. Substitui a ADR 0015.

## Contexto
O canal WebSocket da ADR 0015 nao funcionava de forma confiavel (socket morto em silencio, troca blue/green, rede do
usuario) e so atendia o dono do app. O usuario quer dois tipos de DOT: os exclusivamente dele e os **compartilhados**
com usuarios escolhidos do Flexi, que conversam com o DOT e assistem ao que ele faz. O app vai rodar num computador
Ubuntu ligado 24 h, sem ninguem na frente dele.

## Decisao
O Flexi vira o **correio**. O `dotsd` so faz chamadas de saida por REST, com a sessao da conta do usuario
(`account.flexi_connection_id`), para `/api/agentes-dots/app/*`. Nenhuma porta e aberta na maquina e nao ha socket.

Dados no Flexi (tabelas `agentes_dots_*`, por base; migracao `20261022_agentes_dots_compartilhados.sql`):
`agentes_dots_apps` (presenca e desempenho do dono), `agentes_dots_agentes` + `agentes_dots_agentes_usuarios`
(DOTs compartilhados e quem pode ve-los), `agentes_dots_mensagens` (conversa unica por agente, tipo grupo) e
`agentes_dots_aprovacoes` (espelho das aprovacoes do dono com a decisao tomada no Flexi web).

No DOTS: `dots.flexi_share_json` (`{enabled, users:[{id,name}]}`) e `dots.flexi_conversation_id` (migracao 0014). O
interruptor "Compartilhar no Flexi" e a lista de usuarios ficam no perfil do DOT, aba Aplicativos; os usuarios vem de
`GET /api/usuarios` pelo IPC `apps.flexi.users`.

Tarefa de fundo `flexi_sync::run` (substitui `flexi_link`):
- `PUT /app/agentes` no inicio e em `dots.changed`/`teams.changed` (e quando o usuario reordena as equipes,
  `settings.changed` de `ui.teams_order`): lista dos DOTs compartilhados (id, nome, descricao, status, usuarios) e
  `equipes` (id, nome, gerente_id, membros, pai, na ordem do usuario) para o Flexi agrupar como a lista de contatos.
  O Flexi desativa (`ativo = 0`) o que saiu da lista.
- `POST /app/sincronizar` a cada 4 s (ou na hora, por evento): `{versao, pendentes, dots, estados?, aprovacoes?,
  desempenho?}`. `estados` e a linha de cada DOT compartilhado na lista de contatos (`contacts.list`: situacao, texto,
  cargo, pulso, concluido, pendentes, ultima_atividade_em) e so vai quando mudou desde o ultimo aceite; o Flexi guarda
  em `agentes_dots_agentes.estado` e desenha a mesma linha (anel do avatar, batimento, cargo, equipes) no painel web
  (2026-10-23). Aprovacoes vao quando mudam (`approval.requested|resolved`); desempenho (7 e 30 dias) a cada 5 min ou
  60 s depois de uma tarefa terminar. A resposta traz `mensagens` (de usuarios, em `enviada`/`processando`) e `decisoes` (aprovacoes
  decididas no Flexi web ainda nao aplicadas).
- Cada mensagem nova vira uma mensagem do usuario na conversa unica "Flexi" do DOT (`send_conversation_message`, mesmo
  caminho do IPC `conversations.send`), com o texto `"<nome> (pelo Flexi): <mensagem>"` e metadados
  `{flexi:{mensagem_id, usuario_id, autor_nome}, task_id}`. O app marca a mensagem como `processando`, acompanha a
  mensagem do DOT (streaming) e envia `POST /app/mensagens` (upsert por `dots_message_id`, com `estado`, `situacao` da
  tarefa e `acoes` = ferramentas usadas); ao terminar, `POST /app/mensagens/:id/estado` com `concluida` ou `erro`.
  Depois de um reinicio, mensagens em `processando` sao reencontradas pelo `task_id` nos metadados.
- Decisoes do Flexi web passam por `decide_approval_by(..., "user", "o usuario (pelo Flexi)")` com a auditoria
  `approval.decide_via_flexi_web`, e o app confirma com `POST /app/aprovacoes/:id/aplicada`.
- Sem conta ou sem cookie, espera 30 s ou `apps.changed`. 401 chama `flexi_check` (relogin). Falhas: backoff de 5 s
  dobrando ate 60 s. O estado continua exposto pelo IPC `apps.flexi.link_status` e pelo evento `flexi_link.changed`
  (mesmos nomes da ADR 0015, para a interface); `apps.flexi.link_reconnect` agora significa "sincronizar agora".

No Flexi web (`/api/agentes-dots/*`, qualquer usuario logado): `status` (presenca do dono, `dono`, quantos agentes o
usuario ve), `agentes`, `agentes/:id/mensagens` (`?desde=` devolve so o que mudou, inclusive respostas em streaming),
`POST agentes/:id/mensagens`, e para o dono `aprovacoes`, `aprovacoes/:id/decidir` e `desempenho`. O painel
"Agentes DOTS" do TaskDock lista os agentes compartilhados e abre a conversa; as abas Aprovacoes e Desempenho so
aparecem para o dono. "Conectado" = ultimo contato do app ha menos de 90 s.

## Consequencias
- Funciona atras de qualquer rede e com o Flexi em blue/green: nao ha conexao persistente para cair.
- A conversa de cada agente compartilhado fica no banco do Flexi; o DOT ve quem falou. A conversa e de grupo: todos
  os usuarios escolhidos veem e escrevem na mesma thread.
- Aprovacoes continuam so com o dono; um DOT compartilhado que pede permissao fica "Aguardando aprovacao do dono" ate
  ele decidir no app ou no Flexi web.
- Latencia de ate 4 s para o DOT receber a mensagem e de ate 3 s para o Flexi web mostrar a resposta (polling).
- `tokio-tungstenite` e `futures-util` sairam do runtime. O screencast do navegador do DOT nao vai ao Flexi nesta versao.
