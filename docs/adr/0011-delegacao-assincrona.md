# ADR 0011: delegacao entre DOTs assincrona com tarefas de continuacao

Data: 2026-10-15. Estado: aceito.

## Decisao
`dots_delegate` cria a tarefa no DOT alvo e retorna imediatamente; o turno do solicitante termina. Quando todas as delegacoes
de uma tarefa concluem, o runtime cria uma tarefa de continuacao (`origin = followup:<pai>`) para o solicitante, na mesma
conversa, com os resultados. O mesmo mecanismo serve a `dots_ask_user`: a pergunta entra na central (tipo `question`) e a
resposta volta por continuacao. Limites: `can_delegate_to`, `max_delegation_depth`, ciclo recusado pela cadeia; a tarefa
filha roda com a politica do DOT alvo; cancelar o pai cancela as filhas. Supervisor e um papel do DOT com instrucoes e
ferramentas extras, sem poder de alterar permissoes.

## Alternativas rejeitadas
Espera bloqueante dentro da chamada MCP (timeouts de ferramenta dos CLIs, ex. Codex `tool_timeout_sec`), processos de motor
permanentes.
