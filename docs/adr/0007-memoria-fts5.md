# ADR 0007: memoria com FTS5 e injecao limitada no prompt

Data: 2026-10-15. Estado: aceito.

## Decisao
Quatro tipos em `memories` (`identity`, `working`, `long_term`, `history`) e tabela `memories_fts` autonoma (FTS5 unicode61,
sem depender de rowid estavel), sincronizada por triggers. Recuperacao por bm25 ajustado pela importancia, com fallback por
importancia/recencia; apenas os trechos relevantes entram no prompt (limite de 6000 caracteres). `working` e atualizada pelo
runtime durante a tarefa e removida ao fim; `history` recebe um resumo por tarefa (poda em 200 por DOT).
O DOT pode gravar/buscar memorias pelo gateway (`dots_remember`, `dots_recall`). Compactacao por LLM fica para a Fase 3.
