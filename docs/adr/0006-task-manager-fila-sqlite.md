# ADR 0006: Task Manager com fila na propria tabela `tasks`

Data: 2026-10-15. Estado: aceito.

## Decisao
A fila persistente e a tabela `tasks` (`status='QUEUED' ORDER BY priority DESC, created_at`). O scheduler acorda por
`Notify` (criacao, termino, decisao) ou a cada 30 s para expirar aprovacoes; nunca executa inferencia sem tarefa.
Concorrencia: semaforo global (`runtime.max_concurrent_executions`, padrao 2) e `DotLimits.max_concurrent_tasks` por DOT.
Cada tentativa e uma linha em `executions`; eventos normalizados vao para `execution_events` (seq) e para a UI por IPC.
Recuperacao no boot: execucoes `RUNNING` viram `FAILED(interrompida)`; processos orfaos recebem SIGTERM no grupo;
tarefas `retry_safe` voltam a `QUEUED`, as demais a `WAITING_USER`.
Cancelamento: SIGINT no grupo de processo, 5 s de carencia, SIGKILL.

## Alternativas rejeitadas
Redis/BullMQ (fora dos principios), polling periodico de LLM (proibido pela especificacao).
