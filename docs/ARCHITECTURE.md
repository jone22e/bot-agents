# Arquitetura (Fases 1 a 3)

```text
DOTS.app (Tauri 2, Vue 3)
   |  Unix socket + NDJSON (JSON-RPC subset), token + uid do peer
   v
dotsd (Rust, tokio)  <-- launchd (LaunchAgent) ou spawn detached
   +-- dots-ipc      servidor, handshake hello, eventos push com seq
   +-- dots-storage  ator em thread dedicada -> rusqlite (bundled, WAL)
   +-- dots-engines  deteccao de Codex / Claude / AGY (sem presumir recursos)
   +-- dots-core     tipos de dominio (export ts-rs -> packages/shared-types)
```

## Crates

| Crate | Responsabilidade |
|---|---|
| `dots-core` | Entidades (`Dot`, `EngineDetection`, `RuntimeStatus`...), ids UUID v7, `DotsError` com codigos, geracao dos tipos TS |
| `dots-storage` | `Storage` (conexao + pragmas + migracoes + backup), repositorios, `StorageHandle` (ator mpsc/oneshot) |
| `dots-ipc` | `SocketPaths` (limite de 104 bytes, fallback TMPDIR), `IpcServer`, `IpcClient`, `RuntimeInfo` (`runtime.json`) |
| `dots-engines` | `candidate_dirs`, `resolve_executable`, `run_command` com timeout, `detect_one/detect_all`, `specs` com capacidades verificadas |
| `dotsd` (dots-runtime) | binario: lock `flock`, pid, sinais, `RuntimeService` (metodos IPC), deteccao inicial em background |
| `desktop` (src-tauri) | `supervisor` (sidecar -> `bin/dotsd-<v>`, plist, launchctl, spawn detached), `RuntimeConnection`, comandos |

## Fluxo de inicializacao

1. App abre; plugin single-instance garante janela unica.
2. `RuntimeConnection::ensure`: le `runtime.json`; se o pid vive e o socket responde ao `hello`, conecta.
3. Senao: em producao copia o sidecar para `bin/dotsd-<versao>`, grava/atualiza o LaunchAgent e faz `launchctl bootstrap` (fallback: spawn detached). Em dev usa sempre spawn detached do `target/debug/dotsd`.
4. `dotsd`: cria diretorios, `flock(runtime.lock)`, abre o banco (backup + migracoes se necessario), garante `installations`, gera token novo, abre o socket, grava `runtime.json`, detecta motores em background.
5. Webview: `useRuntimeEvents` conecta eventos (`dots.changed`, `engines.changed`, `settings.changed`) as stores Pinia.

## Protocolo IPC

- Request `{id, method, params}`; response `{id, result}` ou `{id, error:{code,message}}`; evento `{event, payload, seq}`.
- Primeiro frame obrigatorio: `hello {token, client, proto}`. Token e rotacionado a cada start do runtime, comparado em tempo constante; o uid do peer deve ser o do runtime.
- Metodos: `runtime.status|shutdown|backup|backups`, `installation.get`, `overview.get`, `settings.list|get|set`, `dots.list|get|create|update|duplicate|pause|resume|archive|delete|workspaces`, `engines.list|detect|set_override|set_enabled`, `audit.list`.

## Banco de dados

SQLite via rusqlite `bundled` (sem instalacao externa). Pragmas: WAL, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`.
Migracoes em `migrations/NNNN_nome.sql`, embutidas no binario, com checksum gravado em `schema_migrations`.
Um arquivo aplicado alterado ou um banco mais novo que o binario sao erros explicitos.

Tabelas: `installations`, `dots`, `dot_instructions` (versionadas), `dot_engines`, `dot_workspaces`, `tasks`, `executions`,
`conversations`, `messages`, `memories`, `engine_configurations`, `audit_logs`, `application_settings`, `schema_migrations`.
`tasks`/`executions`/`conversations`/`messages`/`memories` ja existem para a Fase 2, sem uso na Fase 1.

## Seguranca na Fase 1

- Socket em diretorio 0700, arquivo 0600, uid do peer verificado, token por sessao.
- Nenhum token de provedor e armazenado; a autenticacao dos CLIs permanece sob controle do proprio CLI.
- O runtime nao depende do PATH (sob launchd o PATH e minimo); executaveis sao resolvidos em diretorios conhecidos e podem ser fixados manualmente.
- Nenhuma execucao de modelo nesta fase; a deteccao roda apenas `--version` e comandos de status de login, com timeout.

## Execucao (Fase 2)

```text
UI -> conversations.send / tasks.create -> tasks(QUEUED) -> scheduler (Notify)
   -> executor: contexto (dot, politica, dirs, memorias) + token de execucao
   -> EngineManager.execute -> CLI (codex exec | claude -p | agy --print)
        CLI -> dotsd mcp-gateway (stdio MCP) -> IPC gateway.call -> politica/aprovacao -> ferramenta -> auditoria
   -> eventos normalizados -> execution_events + execution.event (IPC) -> UI
   -> fim: executions.finish, tasks.set_status, mensagem final, memoria working removida, history gravada
```

Metodos IPC adicionais: `tasks.*`, `executions.*`, `conversations.*`, `approvals.*`, `memories.*`, `policies.*`,
`workspaces.*`, `runtime.pause_all`, `engines.agy_*`, `gateway.call` (uso do gateway).
Eventos: `task.changed`, `execution.event`, `approval.requested|resolved`, `message.changed`, `conversation.changed`,
`memory.changed`, `runtime.paused`.

## Automacao e colaboracao (Fase 3)

```text
fontes de evento: runtime (task.*, tool.*, approval.*, user.message, delegation.*), watcher (file.changed, git.commit.created),
                  agendador (schedule.due)
   -> events (idempotency_key UNIQUE) -> gatilhos (event_subscriptions) -> event_deliveries UNIQUE -> tasks(origin=event:<id>)
delegacao: dots_delegate -> tasks(origin=delegation:<id>, parent_task_id, delegation_chain) -> ao concluir todas -> tasks(origin=followup:<pai>)
pergunta:  dots_ask_user -> approvals(kind=question) -> resposta -> tasks(origin=followup:<tarefa>)
retomada:  checkpoints (auto/dot) + tool_invocations (hash) -> nova tentativa com contexto; chamada identica devolve resultado gravado
```

Metodos IPC adicionais: `events.list`, `subscriptions.*`, `schedules.*`, `delegations.list`, `tasks.children`, `stats.get`,
`runtime.logs`. Eventos IPC: `event.recorded`, `automation.changed`, `delegation.changed`.

## Integracao com o Flexi (Fase Flexi)

```text
UI (Conectar aplicativos) --IPC apps.flexi.*--> dotsd::flexi (FlexiRegistry) --HTTP cookie--> Flexi (/api/auth/login, /api/me, rotas de marketplace)
                                                      |                 \-- Keychain: flexi:<id>:cookie | flexi:<id>:password
CLI do motor --MCP--> dotsd mcp-gateway --gateway.tools / tools/call--> gateway.execute_tool --flexi_*--> flexi_execute (catalogo dots-flexi, politica, aprovacao, auditoria)
```

`app_connections` guarda os metadados da conexao; `dot_integrations` liga cada DOT a uma conexao. Ver `docs/PHASE-FLEXI.md` e ADR 0013.

## Navegador por DOT (spike)

```text
CLI do motor --MCP--> dotsd mcp-gateway --dots_browser_open--> BrowserPool (um Chrome por DOT, perfil em <dados>/browsers/<dot_id>)
                                                                  \-- CDP por --remote-debugging-pipe (fd 3/4, sem porta) --> Page.navigate + Accessibility.getFullAXTree
```

Mesma politica, aprovacao e auditoria das demais ferramentas; exige `allow_network`. Ver `docs/BROWSER.md`.
