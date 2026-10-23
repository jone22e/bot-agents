# ADR 0017: metas, comandos do dono e agendamentos no Flexi web

Data: 2026-10-23. Estado: aceito. Complementa a ADR 0016.

## Contexto
O espelho da conversa (ADR 0016) chegava ao Flexi com defeitos: as mensagens do dono e as notas de sistema
("Mariana agendou: cron ...") entravam como mensagens do DOT, toda resposta final aparecia como "Nao concluiu"
(o executor grava `status: "COMPLETED"` e o espelho comparava com `"completed"`) e os horarios saiam 3 h
deslocados (coluna `timestamp` sem fuso com `now()` local e `criada_em` em UTC). O usuario tambem pediu metas
("eu digo a meta no chat, o gerente acompanha e alimenta"), um menu de contexto no agente do Flexi web (meta,
tarefas periodicas, modelo da IA) e a saida da aba Desempenho do Flexi.

## Decisao
- **Espelho corrigido.** `reply_state` compara o status sem diferenciar maiusculas. Cada nota de sistema vai com
  `origem` (`metadata.origin` sem o id: `schedule`, `event`, `delegation`, `followup`, `autonomy`, `goal`) e
  `detalhe` (`metadata.prompt`). `MIRROR_VERSION` (setting `flexi_sync.mirror_version`): quando o formato muda,
  os cursores zeram uma vez e as 50 mensagens mais recentes de cada DOT sao reenviadas; o upsert do Flexi passa a
  corrigir papel, autor, origem, detalhe e data. O cron vira frase legivel (`describe_cron`: "todos os dias as
  08:00, 10:00"), e `mark_run` emite `automation.changed` para o Flexi ver o proximo disparo.
- **Metas** (migracao 0015: `goals`, `goal_updates`; `dots_core::Goal`). Ferramentas `dots_goal_create`,
  `dots_goal_progress` (valor acumulado ou delta, com nota), `dots_goal_list` e `dots_goal_close`. So quem nao e
  funcionario de uma equipe registra meta (o gerente); funcionario recebe a orientacao de relatar os numeros ao
  gerente. Cada acao deixa uma nota na conversa (origem `goal:<id>`), audita e emite `goal.changed`. IPCs
  `goals.list` e `goals.history` para o app.
- **Sincronizacao** (`POST /app/sincronizar`): `agendamentos` (lista inteira, em `automation.changed`) e `metas`
  (com os 30 lancamentos mais recentes, em `goal.changed`). O registro (`PUT /app/agentes`) leva `motores`
  (catalogo de modelos detectado por motor) e `motor`/`modelo` de cada DOT. `desempenho` deixou de ir ao Flexi.
- **Comandos do dono.** A resposta da sincronizacao traz `comandos` (`definir_modelo`, `pausar`, `retomar`,
  `agendamento_ativar`, `agendamento_cancelar`), aplicados pelos mesmos IPCs do app (`dots.update`, `dots.pause`,
  `schedules.update|delete`) com auditoria `flexi_web.command`, e confirmados em `POST /app/comandos/:id/aplicada`.
  Idempotentes: se a confirmacao falhar, aplicar de novo nao muda nada.
- **Flexi** (migracao `20261023_agentes_dots_metas_comandos.sql`): `origem` nas mensagens; `motor`, `modelo`,
  `agendamentos` nos agentes; `motores` no app; tabelas `agentes_dots_metas` e `agentes_dots_comandos`. Leitura
  das colunas `timestamp` por `at time zone current_setting('TimeZone')` e gravacao de datas do app por
  `::timestamptz`. Painel so com agentes (sem abas); botao direito no agente abre o menu; metas em cartao visual
  (anel, ritmo esperado, projecao, ultimos 7 dias); so o dono troca modelo, pausa e mexe nas tarefas periodicas.

## Consequencias
- O historico antigo no Flexi so e corrigido para as 50 mensagens mais recentes de cada DOT (reenvio unico).
- Numeros que o gerente nao lancar com `dots_goal_progress` nao existem para o Flexi: o prompt deixa isso claro.
- O app continua sem porta aberta; comandos e metas tem a latencia do polling (ate 4 s).
