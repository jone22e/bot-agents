# Plano da Fase 3: autonomia operacional e colaboracao local

Data: 2026-10-15. Pre-requisito: Fases 1 e 2 aprovadas. Sem comunicacao entre computadores (Fase 4).

## Objetivo

DOTs acionados por eventos locais e horarios explicitos, trabalhos longos com checkpoints e retomada segura,
delegacao entre DOTs no mesmo Mac, um DOT Supervisor coordenando outros, e uma central de operacoes.
A interface segue `docs/UX.md`: simples por padrao, detalhes sob demanda.

## Decisoes arquiteturais

### 1. Eventos (Event Dispatcher)

- Tabela `events(id, type, source, occurred_at, payload_json, correlation_id, idempotency_key UNIQUE, depth)`.
  Inserir um evento com `idempotency_key` repetida e ignorado silenciosamente (dedupe na origem).
- Tipos emitidos pelo runtime: `task.completed`, `task.failed`, `tool.execution.finished`, `user.message.received`,
  `approval.granted`, `approval.rejected`, `dot.task.requested`, `delegation.completed`, `file.changed`, `git.commit.created`,
  `schedule.due`.
- **Gatilhos** (`event_subscriptions`): um DOT se inscreve num tipo com filtro (ex.: pasta e padrao `*.md`; nome do DOT de origem),
  um texto de pedido ("Revise o arquivo {{path}}"), `retry_safe`, `cooldown_seconds` e `enabled`.
- Entrega idempotente: `event_deliveries(event_id, subscription_id) UNIQUE` -> no maximo uma tarefa por evento e gatilho.
  Evento repetido (mesma chave) nunca gera segunda tarefa. Encadeamento limitado por `depth` (maximo 4) e cooldown por gatilho,
  para um gatilho nao realimentar a si mesmo (ex.: DOT que escreve `.md` e esta inscrito em `*.md`).
- Fonte `file.changed`: observador de arquivos (`notify`) apenas nas pastas autorizadas dos DOTs com gatilho ativo, com
  debounce de 2 s e chave = caminho + mtime + tamanho; ignora `.git/`, `node_modules/`, `.dots/`, `target/`.
- Fonte `git.commit.created`: o mesmo observador acompanha `.git/HEAD` e `refs/heads/*`; chave = repositorio + sha.
- Fonte `schedule.due`: `schedules(dot_id, kind: every|daily|weekly|cron, spec, prompt, next_run_at, enabled)`; o loop
  acorda apenas no `next_run_at` mais proximo (sem polling de modelo). Expressao cron via crate verificada no spike.

### 2. Checkpoints e retomada

- `checkpoints(task_id, execution_id, step, state_json, context_json, results_json, dependencies_json, next_action,
  artifacts_json, created_at)`. O runtime grava automaticamente no inicio e apos cada ferramenta de escrita/execucao;
  o DOT pode gravar um explicito com a ferramenta `dots_checkpoint {step, state, next_action, artifacts}`.
- Retomada: `tasks.max_attempts` (padrao 2 para `retry_safe`). Ao reexecutar, o prompt recebe o ultimo checkpoint
  ("voce estava na etapa X, resultados Y, proxima acao Z, nao repita W") e a sessao do motor quando existir.
- Guarda de nao idempotencia: o gateway registra `tool_invocations(task_id, tool, input_hash, outcome, output)`; uma chamada
  de escrita/comando/git identica a uma ja concluida com sucesso na mesma tarefa devolve o resultado gravado em vez de repetir.
- Documentar que a retomada e por checkpoint, nao por estado interno do CLI.

### 3. Delegacao entre DOTs

- `delegations(id, from_dot_id, to_dot_id, parent_task_id, child_task_id, correlation_id, request, result, status, created_at)`.
- Ferramentas do gateway: `dots_delegate {to: nome, request, context?}` (cria tarefa no DOT alvo e retorna imediatamente),
  `dots_delegation_status {id?}`, `dots_cancel_delegation {id}`. Sem espera bloqueante: o turno do solicitante termina
  informando o que aguarda; quando a tarefa filha conclui, o evento `delegation.completed` cria uma tarefa de continuacao
  para o solicitante (mesma conversa e correlacao) com o resultado no pedido. Evita depender de timeouts de ferramenta dos CLIs.
- Limites na politica do DOT: `can_delegate_to` (lista de DOTs ou nenhum), `max_delegation_depth` (padrao 2).
  Ciclo impedido pela cadeia de delegacao guardada em `tasks.delegation_chain_json` (DOT ja presente na cadeia = recusa).
- Permissoes: a tarefa filha roda com a politica do DOT alvo; nada e herdado do solicitante. Cancelar a tarefa pai cancela as filhas.
- Toda mensagem entre DOTs fica em `delegations` e em `audit_logs`.

### 4. Supervisor

- `dots.role` = `worker | supervisor`. O Supervisor recebe instrucoes extras e as ferramentas `dots_list_team` (DOTs autorizados,
  com descricao e estado), `dots_delegate*` e `dots_ask_user {question, options[]}`.
- `dots_ask_user` cria um pedido de decisao na central de aprovacoes (tipo `question`, com opcoes); a tarefa fica `WAITING_USER`
  e continua numa tarefa de continuacao com a resposta.
- Consolidacao: quando todas as delegacoes de uma tarefa concluem, a continuacao recebe todos os resultados; o Supervisor
  consolida e responde ao usuario. O Supervisor nao altera politicas (nao ha ferramenta para isso).

### 5. Segundo plano e observabilidade

- Ja existe: runtime sob launchd, pausar DOT, pausar tudo, cancelar, retry, recuperacao no boot. Adicionar: configuracao
  "iniciar com o Mac" (RunAtLoad no plist), retomar tarefa `WAITING_USER` com um clique, e cancelamento em cascata.
- Central de operacoes (dentro de Atividades e Visao geral, sem nova entrada no menu): DOTs ativos, execucoes em andamento
  com tempo decorrido, fila, bloqueadas, aprovacoes/perguntas pendentes, erros das ultimas 24 h, consumo por assistente
  (tokens e custo quando informado), uso de CPU/memoria dos processos de motor (amostragem por pid), ultimas linhas do log.
- Metodos IPC: `stats.get`, `runtime.logs {lines}`, `events.list`, `subscriptions.*`, `schedules.*`, `delegations.list`,
  `checkpoints.list`, `questions.answer`.

## Migracao 0003 (nova; 0001 e 0002 intocadas)

`events`, `event_subscriptions`, `event_deliveries`, `schedules`, `checkpoints`, `delegations`, `tool_invocations`,
`dots.role`, `tool_policies.can_delegate_to_json`, `tool_policies.max_delegation_depth`, `tasks.parent_task_id`,
`tasks.delegation_chain_json`, `tasks.max_attempts`, `tasks.origin` (`user | event:<id> | delegation:<id> | schedule:<id>`).

## Interface (seguindo docs/UX.md)

- DOT: aba **Gatilhos** ("Quando acontecer X, pedir Y"): lista de gatilhos em frases, criacao guiada (tipo, pasta/padrao ou
  DOT de origem, horario, o que pedir). Aba **Equipe** (so para Supervisor): quais DOTs ele pode acionar.
- Novo DOT: opcao "Este DOT coordena outros (Supervisor)".
- Atividades: origem do trabalho em frase ("Pedido pelo Supervisor", "Arquivo x mudou", "Horario das 9h"); arvore de delegacao
  no detalhe; painel **Operacoes** com os numeros da central.
- Aprovacoes: perguntas do Supervisor ("Supervisor pergunta: qual banco usar? [A] [B]") junto dos pedidos de permissao, tambem na faixa do topo.

## Etapas

1. Spike curto: crate de cron (`croner` ou `cron`), `notify` no macOS (FSEvents, debounce), amostragem de CPU/memoria por pid.
2. Tipos (`dots-core`), migracao 0003 e repositorios com testes (eventos com dedupe, entregas unicas, agendamentos, checkpoints, delegacoes, invocacoes).
3. Dispatcher de eventos: emissao interna, casamento de gatilhos, criacao de tarefas, profundidade e cooldown; testes.
4. Observador de arquivos e git; agendador por `next_run_at`; testes com pastas temporarias.
5. Checkpoints automaticos e explicitos; retomada com `max_attempts`; guarda de nao idempotencia; teste de reinicio com checkpoint.
6. Delegacao: tabelas, ferramentas, continuacao, limites, ciclos, cancelamento em cascata; teste com CLI falso.
7. Supervisor: papel, instrucoes, `dots_list_team`, `dots_ask_user`, consolidacao; teste com 3 DOTs falsos.
8. Operacoes: `stats.get`, `runtime.logs`, iniciar com o Mac, retomar WAITING_USER.
9. UI: Gatilhos, Equipe, origem e arvore em Atividades, painel Operacoes, perguntas na central e na faixa.
10. E2E automatizado (CLI falso): evento de arquivo gera tarefa; evento duplicado nao gera segunda; Supervisor coordena 3 DOTs;
    reinicio retoma por checkpoint sem repetir comando; runtime ocioso nao executa nada (contagem de processos por 60 s).
11. Demonstracao real: um Supervisor (Claude) delegando a um Revisor (Codex) e um Redator (Claude) sobre um repositorio de teste.
12. Docs, ADRs 0009-0012, `docs/PHASE3.md`, limitacoes.

## Criterios de aceite (como serao demonstrados)

| Criterio | Demonstracao |
|---|---|
| Evento local inicia tarefa | gatilho `file.changed *.md` no workspace; salvar um `.md` cria a tarefa |
| DOT delega a outro | `dots_delegate` cria tarefa no alvo; continuacao recebe o resultado |
| Supervisor coordena >= 3 DOTs | demo real + e2e com 3 DOTs falsos |
| Usuario recebe solicitacao | aprovacao (Fase 2) e pergunta do Supervisor na faixa do topo |
| Execucao continua com janela fechada | ja validado na Fase 2; repetir com tarefa longa e app fechado |
| Tarefa interrompida recuperada com seguranca | SIGKILL no meio; nova tentativa parte do checkpoint; comando ja executado nao repete |
| Sem inferencia em loop ocioso | 60 s com runtime ocioso: zero processos de motor, zero tarefas criadas |
| Eventos duplicados nao repetem operacoes | mesmo `idempotency_key` duas vezes -> uma entrega, uma tarefa |

## Riscos

- Observador de arquivos ruidoso (editores gravam varias vezes): debounce + chave por conteudo mitigam; cooldown por gatilho.
- Qualidade da coordenacao depende do modelo do Supervisor; o runtime garante limites, nao a inteligencia.
- Demonstracao real custa creditos (varias chamadas); e2e usa CLIs falsos.
- Timeouts de ferramenta nos CLIs: por isso a delegacao e assincrona, nunca bloqueante.

## Fora de escopo

Hub, comunicacao entre computadores, sincronizacao de memoria, compactacao de memoria por LLM.
