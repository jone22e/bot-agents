# ADR 0010: checkpoints persistentes e guarda de nao idempotencia no gateway

Data: 2026-10-15. Estado: aceito.

## Decisao
O runtime grava checkpoints automaticos (inicio de tentativa e apos cada ferramenta de escrita/execucao) e o DOT pode gravar
checkpoints explicitos (`dots_checkpoint`). Em nova tentativa (`retry_safe` com `max_attempts`), o prompt recebe o ultimo
checkpoint e a sessao do motor quando existir. Toda invocacao de `dots_write_file`, `dots_delete_file`, `dots_run_command` e
`dots_git` e registrada com hash da entrada; uma chamada identica ja concluida com sucesso na mesma tarefa devolve o resultado
gravado em vez de executar de novo. Nao se promete retomar o estado interno do CLI; a retomada e por checkpoint.
