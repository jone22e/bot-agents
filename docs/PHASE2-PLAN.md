# Plano da Fase 2: execucao real dos DOTs

Data: 2026-10-15. Pre-requisito: Fase 1 aprovada pelo usuario. Base factual: `docs/engines/README.md`.

## Objetivo

Transformar os DOTs em agentes funcionais: conversar, receber tarefas, executar com Codex/Claude/AGY,
usar ferramentas autorizadas pelo runtime, preservar memoria e historico, rodar dois DOTs ao mesmo tempo,
cancelar e bloquear operacoes nao autorizadas.

## Decisoes arquiteturais

### 1. Engine Manager (crate `dots-engines`)

```rust
pub trait AgentEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    async fn detect(&self) -> EngineDetection;                 // ja existe (Fase 1)
    fn capabilities(&self) -> EngineCapabilities;              // flags verificadas por versao
    fn execute(&self, req: ExecutionRequest) -> ExecutionStream; // Stream<Item = ExecutionEvent>
    async fn cancel(&self, execution_id: &str) -> DotsResult<()>;
}
```

`ExecutionEvent` normalizado (um enum, serializado para a UI e gravado em `execution_events`):
`Started{engine_session_id}`, `TextDelta`, `Text`, `Reasoning`, `ToolCall{id,name,input}`, `ToolResult{id,ok,output}`,
`PermissionRequested{approval_id}`, `Usage{...}`, `Notice{level,text}`, `Completed{result_text,usage,cost_usd}`,
`Failed{kind: Auth|Config|Engine|Timeout|Cancelled|Budget, message}`, `Raw{line}` (linhas nao reconhecidas, nunca descartadas).

Cada adaptador converte o formato nativo observado em `docs/engines/fixtures` (testes de parser usam essas capturas):

| | Codex | Claude | AGY |
|---|---|---|---|
| Invocacao | `codex exec --json -C <ws> -s <sandbox> [--add-dir]* [--skip-git-repo-check] -` (prompt via stdin) | `claude -p --output-format stream-json --verbose --include-partial-messages --session-id <uuid> --tools "" --strict-mcp-config --mcp-config <json> --permission-prompt-tool mcp__dots__permission_prompt --permission-mode <m> --add-dir* --max-turns --max-budget-usd` | `agy --print=<prompt> --output-format stream-json --mode <m> [--sandbox] [--add-dir]* --print-timeout 0s` |
| Sessao | `thread_id` -> `codex exec resume <id>` | `session_id` (definido por nos) -> `--resume` | `conversation_id` -> `--conversation` |
| Instrucoes do DOT | `AGENTS.md` gerado em `<ws>/.dots/` + prefixo do prompt | `--append-system-prompt` | prefixo do prompt |
| Ferramentas built-in | sandbox do proprio Codex (Seatbelt) | desligadas (`--tools ""`); so MCP do DOTS | negadas em headless; `--mode`/`--sandbox` |
| Aprovacao de acao | nao roteavel em `exec` (sandbox decide) | `permission_prompt` via MCP do DOTS -> central de aprovacoes | nao roteavel (auto-nega); gateway MCP do DOTS passa pela central |
| Cancelar | SIGINT -> 5 s -> SIGKILL (grupo de processo) | idem (SIGINT encerra o turno; SIGTERM = 143 sem resultado) | idem |

Regras: nenhum processo CLI fica aberto entre tarefas; timeout = `DotLimits.max_execution_seconds`; concorrencia por DOT
(`max_concurrent_tasks`) e global (`settings.runtime.max_concurrent_executions`, padrao 2) via semaforos; erros de auth/config
viram `Failed{Auth|Config}` com orientacao (ex.: Claude `result.is_error` + texto, Codex `error`, AGY `result.status != SUCCESS`).

### 2. Tool Gateway (subcomando `dotsd mcp-gateway`)

Servidor MCP por stdio embutido no proprio binario `dotsd`, iniciado pelo CLI do motor. Ele se conecta ao `dotsd` principal pelo
socket IPC com um **token de execucao** (gerado por execucao, passado em `DOTS_EXECUTION_TOKEN` no ambiente do CLI; fallback: arquivo
`<ws>/.dots/execution.token` 0600 removido ao fim). Toda chamada e autorizada **no runtime**, fora da LLM, e auditada.

Ferramentas da v1 e classe de risco:

| Ferramenta | Risco | Regra |
|---|---|---|
| `dots_list_dir`, `dots_read_file`, `dots_search` | leitura | caminho canonico dentro de diretorio autorizado com `can_read` |
| `dots_write_file`, `dots_apply_patch` | escrita | diretorio com `can_write`; registra alteracao em `workspace_changes` |
| `dots_git` (status, diff, log, add, commit, branch) | escrita | repo dentro de dir autorizado; `push`/`reset --hard`/`clean` = critico |
| `dots_run_command` | execucao | allowlist por DOT (`tool_policies.allowed_commands`, prefixo) + denylist global (`sudo`, `rm -rf /`, `curl|sh`...), cwd autorizado, timeout, sem TTY |
| `dots_system_info` | leitura | SO, versoes dos motores, uso de CPU/mem |
| `permission_prompt` (Claude) | — | recebe `{tool_name, input}`; devolve `{"behavior":"allow"|"deny"}` conforme politica/aprovacao |

Politica por DOT (`tool_policies`): `auto_approve: read | write | exec | none`; acoes acima do nivel geram **aprovacao**
(`approvals` com validade de 10 min; a execucao fica `WAITING_APPROVAL`; o gateway bloqueia a chamada ate decisao ou expira = deny).
MCP: implementar com a crate oficial `rmcp` (stdio, tools) se compilar limpa; senao JSON-RPC minimo proprio (`initialize`, `tools/list`, `tools/call`, `ping`).

### 3. Task Manager (em `dotsd`)

- Toda solicitacao vira `tasks` (kind `chat` ou `task`), estados ja existentes em CHECK. Fila persistente = a propria tabela
  (`status='QUEUED' ORDER BY priority DESC, created_at`).
- Scheduler: loop acordado por notify (nunca polling de LLM); respeita DOT pausado/arquivado, semaforos, dependencia
  `WAITING_*`. Cada tentativa cria `executions` (attempt n) e grava `execution_events` (seq) para replay na UI.
- Recuperacao no boot: `executions.status='RUNNING'` de runtime morto -> `FAILED(interrupted)`; a tarefa volta a `QUEUED` apenas
  se `tasks.retry_safe=1` (padrao: chat = nao; tarefa marcada idempotente = sim); demais -> `WAITING_USER` com motivo.
- Cancelamento: `tasks.cancel` -> SIGINT no grupo -> 5 s -> SIGKILL -> `CANCELLED`.

### 4. Conversas

Uma conversa por DOT (varias permitidas). Mensagem do usuario -> `tasks(kind=chat)` -> execucao com continuidade do motor
(`conversations.engine_sessions_json` guarda o id por motor; se o resume falhar, reconstroi contexto com as ultimas N mensagens +
memorias relevantes, truncado a `settings.context.max_chars`). UI com streaming (`execution.event` via IPC), historico, chips de
ferramentas, estado do motor, cancelar, anexos = caminhos de arquivos **dentro de diretorios autorizados** inseridos como contexto
textual (imagens so no Codex via `-i`, deixado para depois).

### 5. Workspaces

`dot_workspaces` ganha `is_external`; adicionar diretorio externo exige acao explicita na UI (dialogo nativo). Historico de
alteracoes relevantes em `workspace_changes` (gateway e `file_change` do Codex). Temporarios em `<ws>/.dots/tmp` com limpeza ao fim.
Isolamento nativo: sandbox do Codex (Seatbelt) e `--sandbox` do AGY sao usados quando disponiveis; para Claude as built-in ficam
desligadas e so o gateway acessa disco. Documentar que o diretorio por si so nao e isolamento.

### 6. Memoria

Tabela `memories` (ja existe) + `memories_fts` (FTS5, disponivel no SQLite embutido). Tipos: `identity` (derivada das instrucoes),
`working` (estado da tarefa: etapas, decisoes, resultados intermediarios, atualizado pelo runtime a cada evento relevante),
`long_term` (notas do usuario + resultado final de tarefas promovido pelo usuario ou por regra), `history` (resumo por tarefa).
Recuperacao: FTS por termos do prompt + importancia + recencia, top-N limitado por caracteres; nunca o historico inteiro.
Compactacao na Fase 2 = truncamento por tamanho e limpeza por idade; resumo por LLM fica para Fase 3 (custa chamadas).
UI: listar, buscar, editar, excluir.

### 7. Seguranca

Sem privilegios; limites de tempo/processos/custo (`--max-budget-usd` no Claude; AGY/Codex por tokens informados no `usage`);
caminhos canonicalizados (symlink fora do permitido = negado); denylist de comandos; token de execucao por processo; credenciais
continuam nos CLIs; aprovacao para escrita/exec conforme politica; auditoria de toda chamada do gateway; modo irrestrito inexistente.

## Migracao 0002 (nova, sem tocar na 0001)

- `tasks`: `kind`, `retry_safe`, `requested_by`, `engine` (snapshot), `cancel_requested`.
- `executions`: `engine_session_id` ja existe; `events_count`, `cost_usd`, `usage_json`, `workspace_path`.
- `execution_events(id, execution_id, seq, type, payload_json, created_at)`.
- `conversations`: `engine_sessions_json`; `messages`: ja tem `task_id/execution_id`.
- `approvals(id, execution_id, dot_id, tool, input_json, risk, status, requested_at, expires_at, resolved_at, decision, reason)`.
- `tool_policies(dot_id PK, auto_approve_level, allowed_commands_json, allow_network, max_file_bytes)`.
- `workspace_changes(id, execution_id, path, kind, summary, created_at)`.
- `dot_workspaces`: `is_external`.
- `memories_fts` (FTS5 content=memories) + triggers.

## Etapas

1. **Spike de verificacao** (sem UI): (a) `dotsd mcp-gateway` minimo respondendo `tools/list`; (b) Claude com `--strict-mcp-config` +
   `--permission-prompt-tool` chamando o gateway e recebendo deny/allow; (c) Codex com `-c mcp_servers.dots.command=...` e sandbox
   `workspace-write`; (d) AGY com `agy mcp add dots ...` e `permissions.allow`; (e) passagem de `DOTS_EXECUTION_TOKEN` ao servidor
   MCP em cada CLI. Resultado registrado em `docs/engines/README.md`. Define o que entra e o que vira limitacao.
2. `ExecutionEvent` + parsers dos tres formatos com testes sobre as fixtures.
3. Runner de processos (spawn, grupo, streaming stdout/stderr, timeout, cancel, semaforos) + adaptadores `execute/cancel`.
4. Migracao 0002 + repositorios (tasks, executions, events, conversations, messages, memories+FTS, approvals, policies, changes).
5. Task Manager/scheduler + recuperacao + metodos IPC (`tasks.*`, `executions.*`, `conversations.*`, `messages.*`, `approvals.*`,
   `memories.*`, `policies.*`) + eventos (`task.changed`, `execution.event`, `approval.requested`).
6. Gateway MCP completo + politica + auditoria + aprovacoes.
7. Montagem de prompt (identidade + regras + memorias) e continuidade por motor.
8. UI: Conversas (chat com streaming), Atividades (tarefas/execucoes/eventos), Aprovacoes, abas Permissoes e Memoria no editor do DOT,
   Visao geral com numeros reais.
9. E2E automatizado com CLI falso (script que emite o JSONL das fixtures) e E2E manual com CLI real: tarefa que cria um arquivo no
   workspace via gateway, dois DOTs simultaneos, cancelamento, operacao bloqueada, reinicio preservando tudo.
10. Docs (`docs/PHASE2.md`, ADRs 0005-0008), limitacoes, resumo tecnico.

## Criterios de aceite (como serao demonstrados)

| Criterio | Demonstracao |
|---|---|
| Criar DOT, escolher motor, enviar tarefa, acompanhar | UI Conversas com Claude real; eventos ao vivo |
| Executar ferramentas autorizadas e receber resultado | tarefa "crie `hello.txt` no workspace" via `dots_write_file`; arquivo existe; auditoria registrada |
| Preservar historico e memoria; reiniciar sem perder | `launchctl bootout`/`bootstrap`; conversa e memorias intactas |
| Dois DOTs simultaneos | duas execucoes `RUNNING` ao mesmo tempo (uma Codex, uma Claude) |
| Cancelar tarefa | botao Cancelar -> `CANCELLED` em < 6 s, processo morto |
| Bloquear operacao nao autorizada | tarefa pede escrever fora do workspace -> gateway nega, evento visivel, auditoria |
| Tarefa real concluida com CLI instalado | registrada em `docs/PHASE2.md` com ids e saida |

## Riscos e pontos a confirmar no spike

- Passagem de ambiente ao servidor MCP varia por CLI; fallback por arquivo de token no workspace.
- `--permission-prompt-tool` exige resposta num formato especifico; validar com deny forcado antes de depender dele.
- AGY: `permissions.allow` nao tem doc publica; se nao funcionar, AGY roda em `--mode plan` + gateway MCP (escrita so pelo gateway).
- Codex `exec` nao permite aprovacao por acao; a protecao e o sandbox. Documentar como limitacao de aprovacao granular.
- Custo: cada E2E real consome creditos das contas; usar fixtures e CLI falso nos testes automatizados.

## Fora de escopo (Fase 3+)

Eventos de sistema (file.changed etc.), checkpoints de retomada exata, delegacao entre DOTs, Supervisor, compactacao de memoria por
LLM, execucao agendada, Hub.
