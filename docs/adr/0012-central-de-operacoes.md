# ADR 0012: central de operacoes sem nova entrada no menu

Data: 2026-10-15. Estado: aceito.

## Decisao
`stats.get` agrega: DOTs ativos, execucoes em andamento, fila, bloqueios, pendencias (permissoes e perguntas), falhas e
eventos em 24 h, consumo por assistente (execucoes, tokens, custo quando informado, tempo) e amostragem de CPU/memoria dos
processos de motor via `ps`. `runtime.logs` devolve as ultimas linhas do log. A interface mostra isso em Atividades e Visao
geral, mantendo os sete itens do menu da especificacao. "Iniciar com o Mac" e `RunAtLoad` do LaunchAgent.
