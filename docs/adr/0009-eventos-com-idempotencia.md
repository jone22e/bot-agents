# ADR 0009: eventos locais com chave de idempotencia e entrega unica por gatilho

Data: 2026-10-15. Estado: aceito.

## Decisao
Todo evento tem `idempotency_key` unica (INSERT OR IGNORE): duplicados nunca sao gravados. A entrega a um gatilho e
reservada em `event_deliveries (event_id, subscription_id) UNIQUE` antes de criar a tarefa, entao dois disparos concorrentes
nao geram duas tarefas. Encadeamento limitado por `depth` (maximo 4) e cooldown por gatilho; um DOT nao reage aos proprios
`task.*`/`tool.*`. Fontes: runtime (tarefas, ferramentas, aprovacoes, mensagens, delegacoes), observador de arquivos com
debounce de 2 s e chave caminho+mtime+tamanho, commits via `.git/HEAD` com chave repo+sha, agendamentos com chave
agendamento+horario previsto.

## Alternativas rejeitadas
Polling periodico do modelo (proibido), fila externa, eventos sem chave (duplicatas inevitaveis com FSEvents).
