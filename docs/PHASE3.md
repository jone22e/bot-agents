# Fase 3: resumo tecnico, validacao e limitacoes

Data: 2026-10-15. Plataforma validada: macOS 26.2, Apple Silicon. Versao: 0.3.0.

## Entregue

- **Eventos locais** (`events`, `event_subscriptions`, `event_deliveries`): tipos `task.completed`, `task.failed`,
  `tool.execution.finished`, `user.message.received`, `approval.granted`, `approval.rejected`, `dot.task.requested`,
  `delegation.completed`, `file.changed`, `git.commit.created`, `schedule.due`. Chave de idempotencia, entrega unica por
  gatilho, profundidade maxima 4, cooldown por gatilho, DOT nao reage ao proprio trabalho.
- **Gatilhos** na aba Gatilhos do DOT, em frases ("Quando um arquivo mudar (*.md), pedir: ..."), com modelo de pedido.
- **Observador de arquivos e commits** (`notify`, debounce 2 s) apenas nas pastas autorizadas dos DOTs com gatilhos;
  ignora `.git`, `node_modules`, `target`, `.dots`.
- **Agendamentos** (a cada N min, diario, semanal, cron) com `next_run_at`; o loop dorme ate o proximo horario.
- **Checkpoints** automaticos e explicitos (`dots_checkpoint`); nova tentativa parte do ultimo; **guarda de repeticao**
  no gateway para escrita, exclusao, comandos e git.
- **Delegacao** assincrona (`dots_delegate`, `dots_delegation_status`, `dots_cancel_delegation`, `dots_list_team`) com
  limites, ciclos recusados, cancelamento em cascata e continuacao automatica com os resultados.
- **Supervisor**: papel do DOT com instrucoes extras e `dots_ask_user`; perguntas entram na central e na faixa do topo
  com opcoes; a resposta volta por continuacao. Em pedido delegado, `dots_ask_user` encaminha a pergunta a quem delegou
  (tarefa `question:<id>` no gerente, que responde com `dots_answer_question`); a delegacao fica aberta ate a resposta e a
  continuacao do funcionario vira a nova tarefa filha, de modo que o gerente so consolida depois. Pergunta expirada gera a
  continuacao "sem resposta" para nao travar a delegacao. `dotsd call <metodo> [json]` chama qualquer metodo IPC do runtime.
- **Permissao pelo gerente**: pedido de permissao de um funcionario em pedido delegado vira a tarefa `approval:<id>` no
  gerente, que decide com `dots_decide_approval` (o cartao continua na conversa do funcionario como alternativa para o
  usuario); na lista o funcionario aparece como "Pediu permissao ao gerente". Reinicio do runtime recoloca na fila toda
  tarefa interrompida enquanto `attempts <= max_attempts` (antes so as `retry_safe`): fechar e reabrir o app nao
  adormece os agentes.
- **Gerente responsavel pela equipe**: `dots_delegation_status` mostra o estado real do funcionario (na fila, trabalhando,
  parado com o erro, aguardando resposta); `dots_resume_delegation` recoloca na fila a tarefa parada/falha/cancelada de
  uma delegacao sua; as instrucoes do Supervisor mandam consultar o status antes de responder sobre andamento e retomar
  em vez de so relatar. Pedidos delegados interrompidos por reinicio do runtime voltam a fila sozinhos (nao ficam
  "aguardando o usuario"), porque o gerente espera por eles e os pontos de controle evitam repetir acoes.
- **Central de operacoes** em Atividades e Visao geral; "iniciar com o Mac" em Configuracoes; retomar trabalho parado.
- **Migracao 0003** nova; 0001 e 0002 intocadas.

## Testes automatizados

| Suite | Qtd |
|---|---|
| dots-core | 58 |
| dots-storage (eventos com dedupe, entregas unicas, agendamentos, checkpoints, invocacoes, delegacoes, perguntas) | 19 |
| dots-ipc | 10 |
| dots-engines | 23 |
| dotsd unitarios (politica, glob, templates, proximos horarios, watcher) | 11 |
| dotsd e2e Fase 1 e 2 | 5 |
| dotsd e2e Fase 3: gatilho de arquivo (um lote = uma tarefa; runtime ocioso sem processos), Supervisor com 3 DOTs e consolidacao, limites/ciclo/cancelamento em cascata, pergunta ao usuario com continuacao, reinicio por checkpoint sem repetir comando, cron | 6 |
| desktop (vitest) | 20 |

## Validacao dos criterios de aceite

| Criterio | Demonstracao |
|---|---|
| Evento local inicia tarefa | e2e: gravar `notas.md` cria tarefa `event:*` com o pedido do gatilho |
| DOT delega a outro | e2e e demo real: `dots_delegate` cria tarefa `delegation:*` no alvo |
| Supervisor coordena >= 3 DOTs | e2e com Alfa/Beta/Gama; demo real com Revisor (Codex) e Redator (Claude) |
| Usuario recebe solicitacao | permissoes (Fase 2) e perguntas do Supervisor (e2e `ask_user`) |
| Execucao continua com janela fechada | runtime sob launchd (Fase 1/2) + "iniciar com o Mac" |
| Tarefa interrompida recuperada com seguranca | e2e: SIGKILL durante comando; 2a tentativa recebe o checkpoint e o comando identico e devolvido gravado (executado uma unica vez na auditoria) |
| Runtime nao executa inferencia em loop | e2e: 5 s ocioso com zero processos de motor e zero tarefas novas; scheduler/agendador acordam por notificacao |
| Eventos duplicados nao causam execucao indevida | storage: chave repetida ignorada; e2e: lote com duas gravacoes gera uma tarefa |

## Demonstracao real (2026-10-15, runtime de desenvolvimento, CLIs reais)

Repositorio de teste com `calc.py`. DOTs: **Supervisor** (Claude, papel supervisor, equipe = todos), **Revisor** (Codex,
so leitura), **Redator** (Claude, pode escrever). Pedido ao Supervisor: "delegue a revisao ao Revisor e um README ao Redator,
depois consolide".

| Momento | O que aconteceu |
|---|---|
| 11:25:34 | Supervisor comeca; chama `dots_list_team` e `dots_delegate` duas vezes; termina o turno "aguardando" |
| 11:25:40 / 11:26:06 | Revisor (Codex) e Redator (Claude) executam **em paralelo** as tarefas `delegation:*` |
| 11:26:06 | Revisor conclui com revisao real ("tratar divisor zero", "adicionar tipos") |
| 11:26:20 | Redator conclui: README.md criado via `dots_write_file` (na propria pasta de trabalho, pois usou caminho relativo) |
| 11:26:20 | Continuacao do Supervisor criada automaticamente com os dois resultados |
| 11:26:30 | Na consolidacao, o Supervisor pediu uma acao de escrita; com politica "so leitura" o pedido foi para a central e, sem resposta no script, expirou apos 10 min |

Custos informados pelo Claude: US$ 0,19 (Supervisor) e US$ 0,28 (Redator); Codex nao informa custo.
Observacao: o Redator gravou o README na sua pasta de trabalho e nao na pasta externa autorizada porque usou um caminho
relativo; as instrucoes do runtime passam a explicar que caminhos relativos apontam para a pasta de trabalho.

## Limitacoes conhecidas

- O observador usa FSEvents via `notify`; gravacoes de editores podem gerar mais de um lote se separadas por mais de 2 s
  (cada lote vira um evento; o cooldown do gatilho, 60 s por padrao, evita rajadas de tarefas).
- Commits sao detectados pela mudanca de `.git/HEAD`/refs; `git commit --amend` gera evento (sha novo), rebase gera varios.
- A guarda de repeticao compara a entrada exata da ferramenta; um comando equivalente com texto diferente e executado de novo.
- Delegacao e pergunta sao assincronas: o Supervisor precisa de um turno extra (continuacao) para consolidar; isso consome
  uma chamada adicional do assistente.
- A qualidade da coordenacao depende do modelo; o runtime garante limites, auditoria e isolamento de permissoes.
- Amostragem de CPU/memoria usa `ps` a cada consulta de estatisticas (5 s na UI), sem historico.
