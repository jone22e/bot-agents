# ADR 0008: continuidade de sessao por motor, com reconstrucao de contexto como fallback

Data: 2026-10-15. Estado: aceito.

## Decisao
`conversations.engine_sessions_json` guarda o id de sessao por motor: Claude (`--session-id` definido pelo runtime na criacao e
`--resume` depois), Codex (`thread_id` -> `exec resume`), AGY (`conversation_id` -> `--conversation`). Se uma retomada falhar
com erro do motor, a sessao e descartada e a proxima mensagem reconstroi o contexto a partir das instrucoes, memorias e
historico persistidos. Nenhum processo CLI permanece aberto entre mensagens.
