# Motores CLI: comportamento verificado (2026-10-15)

Capturas reais em `fixtures/` (prompt "Responda apenas: OK", diretorio temporario). Nada abaixo e presumido.

## Codex CLI 0.162.1

- Comando: `codex exec --json -s read-only -C <dir> --color never "<prompt>"` (stdin lido como contexto; usar `--skip-git-repo-check` fora de git).
- `-a/--ask-for-approval` **nao existe** em `exec` (so no comando raiz). Em `exec` a aprovacao e automatica e a protecao vem do sandbox (`-s read-only|workspace-write|danger-full-access`) e de `--add-dir`.
- Eventos JSONL: `thread.started{thread_id}`, `turn.started`, `item.started|completed{item:{id,type,...}}` com tipos `agent_message{text}`, `command_execution{command,status}`, `reasoning`, `file_change`, `mcp_tool_call`, `web_search`, `todo_list` (so os dois primeiros observados), `turn.completed{usage}`, `turn.failed`, `error`.
- Continuidade: `codex exec resume <thread_id> "<prompt>"`.
- Ultima mensagem em arquivo: `-o <file>`; saida estruturada: `--output-schema <file>`; sem gravar sessao: `--ephemeral`.
- Modelo: `-m, --model <MODEL>` em `exec` (verificado no `--help`; sem comando para listar modelos).
- MCP: `codex mcp add <name> -- <cmd>` (global) ou override por execucao com `-c mcp_servers.<name>.command=...` (a verificar no spike).
- Instrucoes de projeto: `AGENTS.md` no diretorio raiz (o usuario tem `~/.codex/AGENTS.md`).

## Claude Code 2.1.300

- Comando: `claude -p --output-format stream-json --verbose [--include-partial-messages] "<prompt>"`.
- Eventos: `system/init{session_id,tools,mcp_servers,model,permissionMode}`, `rate_limit_event`, `system/informational`, `system/api_retry`, `system/model_refusal_fallback`, `assistant{message.content[]}`, `user` (tool_result), `stream_event` (com `--include-partial-messages`), `result{subtype:success|error..., result, session_id, total_cost_usd, usage, permission_denials, num_turns, is_error}`.
- Continuidade: `--session-id <uuid>` na criacao e `--resume <uuid>` depois; `--no-session-persistence` desliga.
- Modelo: `--model <model>` (alias ou nome completo; verificado no `--help`; sem comando para listar modelos).
- Restricao de ferramentas: `--tools ""` remove as built-in (nao afeta MCP); `--strict-mcp-config --mcp-config <json>` deixa apenas os servidores informados; `--add-dir`; `--permission-mode`; `--max-budget-usd`; `--max-turns`.
- Aprovacoes sem TTY: `--permission-prompt-tool <mcp__server__tool>` (o tool recebe a solicitacao e devolve allow/deny) ou hook `PermissionRequest` via `--settings` (saida `hookSpecificOutput.decision.behavior = allow|deny`); `--permission-prompts none` nega tudo que pediria.
- `--bare` ignora OAuth (exige `ANTHROPIC_API_KEY`): **nao usar**, o usuario autentica via claude.ai.
- SIGTERM encerra com 143 sem gravar resultado; SIGINT encerra o turno.
- Observado nesta captura: um `model_refusal_fallback` para `claude-opus-4-8` por classificador de seguranca; o adaptador deve tratar `system/*` como informativo e nunca como erro fatal.

## AGY (Antigravity CLI) 1.3.5

- Comando: `agy --print='<prompt>' --output-format stream-json --mode plan|accept-edits|auto [--sandbox] [--add-dir] [--print-timeout 0s]`. **O prompt deve ir colado ao flag** (`--print=...`), senao o valor seguinte e tomado como prompt.
- Eventos: `init{conversation_id,init:{cwd,tools[]}}`, `step_update{step_update:{step_index,state:ACTIVE|DONE|ERROR,step_type:user_input|agent_response|tool,tool_name,tool_info{name,parameters,error},usage}}`, `result{result:{conversation_id,status,response,num_turns,usage,denied_actions[]}}`.
- Headless: qualquer ferramenta que exigiria permissao e **negada automaticamente** (stderr: "add an allow-rule under permissions.allow in settings.json (e.g. command(<target>))"); `--dangerously-skip-permissions` libera tudo (nao usar por padrao).
- Continuidade: `--conversation <conversation_id>`; `--continue` pega a mais recente.
- Modelo: `--model <model>` (verificado no `--help`); `agy models` lista os disponiveis.
- MCP: `agy mcp add [--env K=V] <name> <cmd> [args]` (configuracao global do usuario). Settings em `~/.gemini/antigravity-cli/settings.json` (hoje so `trustedWorkspaces`).
- Sem comando de status de login e sem flag de system prompt documentada: instrucoes vao no prompt.

## Spike de integracao com o gateway MCP do DOTS (2026-10-15, verificado)

Servidor MCP minimo (stdio, JSON-RPC 2.0: `initialize`, `notifications/initialized`, `tools/list`, `tools/call`, `ping`;
clientes tambem enviam `server/discover`, que pode responder `-32601` sem prejuizo).

| | Claude 2.1.300 | Codex 0.162.1 | AGY 1.3.5 |
|---|---|---|---|
| Registrar servidor por execucao | `--strict-mcp-config --mcp-config '{"mcpServers":{"dots":{"command":..,"args":[..],"env":{..}}}}'` | `-c 'mcp_servers.dots.command="..."' -c 'mcp_servers.dots.args=[..]' -c 'mcp_servers.dots.env={K="v"}'` | **nao ha**: so registro global `agy mcp add [-e K=V] dots <cmd> [args]` |
| Aprovar chamadas do servidor | `--permission-prompt-tool mcp__dots__permission_prompt`; o tool recebe `{tool_name,input,tool_use_id}` e devolve texto JSON `{"behavior":"allow","updatedInput":{..}}` ou `{"behavior":"deny",...}` | `-c 'mcp_servers.dots.default_tools_approval_mode="approve"'` (sem isso: "MCP tool call requires approval, but approval policy is never"); por tool: `mcp_servers.dots.tools.<tool>.approval_mode` (relato, nao testado) | `permissions.allow: ["mcp(dots/<tool>)"]` em `~/.gemini/antigravity-cli/settings.json` (global) |
| Ambiente do servidor MCP | herda o ambiente do processo `claude` (`DOTS_EXECUTION_TOKEN` chegou) | **nao herda**; usar `mcp_servers.dots.env` | so o `-e` fixo do registro global |
| cwd do servidor MCP | diretorio de trabalho do CLI | idem | idem |
| Evento da chamada | `assistant.tool_use{name:"mcp__dots__<tool>"}` + `user.tool_result` | `item.started/completed{type:"mcp_tool_call",server,tool,arguments,result,error,status}` | `step_update{step_type:"tool",tool_name:"call_mcp_tool",tool_info{parameters{ServerName,ToolName,Arguments},output}}` |

Consequencias de projeto:
- Identificacao da execucao pelo gateway: `DOTS_EXECUTION_TOKEN` no ambiente quando possivel, senao arquivo `<workspace>/.dots/execution.token` (0600, criado antes do spawn e removido ao fim). Valido para os tres.
- AGY exige configuracao global (servidor `dots` + regras `mcp(dots/*)`), feita uma vez com consentimento na tela Motores e revertivel.
- Codex: aprovacao por acao nao e roteavel em `exec`; a politica do DOTS e aplicada dentro do gateway (o Codex "aprova" chamar o gateway, o gateway decide).

## Ambiente minimo dos processos (verificado 2026-10-15)

O runtime inicia os CLIs com ambiente explicito (`env_clear`). Com apenas `HOME` e `PATH`, o Claude Code responde
"Not logged in · Please run /login" mesmo autenticado; com `USER` e `LOGNAME` (obtidos por `getpwuid` quando o launchd
nao os define) o login e encontrado. O ambiente base e: `HOME`, `PATH` (diretorios candidatos), `USER`, `LOGNAME`, `SHELL`,
`TMPDIR`, `LANG`, `NO_COLOR=1`, `TERM=dumb`, mais `DOTS_EXECUTION_TOKEN` e `DOTS_DATA_DIR`.

## Demonstracao real (2026-10-15, runtime de desenvolvimento)

Tarefa "crie `hello-<motor>.txt` via `dots_write_file` e liste o diretorio", politica `auto_approve_level = write`:

| Motor | Resultado | Sessao gravada | Observacoes |
|---|---|---|---|
| Claude 2.1.300 | COMPLETED, arquivo criado, 9 eventos, custo US$ 0,16 | uuid fixado pelo runtime (`--session-id`) | rodou em paralelo com o Codex (dois DOTs `RUNNING`) |
| Codex 0.162.1 | COMPLETED, arquivo criado, 9 eventos | `thread_id` | `mcp_tool_call` com `default_tools_approval_mode="approve"` |
| AGY 1.3.5 | COMPLETED, arquivo criado, 10 eventos | `conversation_id` | exige `engines.agy_setup` (global); o modelo primeiro inspecionou `~/.gemini/antigravity-cli/mcp/dots/dots_write_file.json` e depois chamou `call_mcp_tool` |

Auditoria registrou `tool.dots_write_file` e `tool.dots_list_dir` com `outcome=ok` para cada DOT.

## Escolha de modelo (2026-10-15)

O DOTS so passa a flag de modelo quando ha um valor definido; sem valor, cada CLI usa o proprio padrao. Precedencia:
modelo do DOT (`dot_engines.config_json.model`, campo Avancado do DOT) > modelo padrao do motor
(`engine_configurations.model`, tela Configuracoes > Assistentes de IA) > padrao do CLI. Trocar o motor principal de um DOT
descarta o modelo, porque o nome e especifico de cada CLI. A lista oferecida no seletor vem, como no ovrgit, do proprio
CLI na deteccao (`EngineDetection.models`): Codex le `~/.codex/models_cache.json` (ou `$CODEX_HOME`), sem os
`visibility: "hide"` e na ordem de `priority`; AGY roda `agy models` (linhas `id<TAB>nome`); o Claude nao lista modelos e
usa a lista fixa da interface (`apps/desktop/src/lib/models.ts`). Nomes digitados em "Outro" nao sao validados: um nome
invalido falha na execucao com a mensagem do proprio CLI.
