# Fase 1: resumo tecnico, validacao e limitacoes

Data: 2026-10-15. Plataforma validada: macOS 26.2, Apple Silicon.

## Entregue

- App desktop Tauri 2 + Vue 3 + TypeScript + Tailwind 4 + Pinia + Vue Router, com navegacao
  Visao geral / Meus DOTs / Atividades / Conversas / Aprovacoes / Motores de IA / Configuracoes
  e assistente de primeira execucao. Atividades, Conversas e Aprovacoes mostram estado vazio honesto.
- Runtime `dotsd` (Rust/tokio) independente da janela: lock de instancia unica, pid, encerramento
  ordenado por SIGTERM/SIGINT/IPC, `runtime.json`, logs rotativos em `logs/`.
- LaunchAgent `com.c2s.dots.runtime` com binario copiado para `bin/dotsd-<versao>`; fallback de spawn desacoplado.
- SQLite embutido (rusqlite `bundled`, WAL), 14 tabelas, migracoes com checksum, backup automatico
  antes de migrar e manual pela UI, recuperacao de erro explicita (banco mais novo que o binario).
- IPC local autenticado (socket 0600 em diretorio 0700, uid do peer, token por sessao) com eventos push.
- CRUD completo de DOTs: criar, editar (instrucoes versionadas), duplicar, pausar, retomar, arquivar,
  excluir com confirmacao, motor principal, workspace (padrao `workspaces/<id>`), limites.
- Deteccao dos motores sem depender do PATH: Codex (`codex login status`), Claude (`claude auth status`),
  AGY (sem comando de status: reportado como "nao verificavel"). Capacidades listadas somente as
  conferidas no `--help` das versoes 0.162.1 / 2.1.300 / 1.3.5. Caminho manual por motor.
- Tipos TS gerados do Rust (`pnpm gen:types`).
- Auditoria de toda mutacao em `audit_logs`.

## Testes automatizados

| Suite | Cobre | Qtd |
|---|---|---|
| dots-core | validacao, ids, serde de `Option<Option<T>>`, export TS | 22 |
| dots-storage | migracao idempotente, upgrade com backup e dados preservados, CRUD, auditoria, ator concorrente, backups | 9 |
| dots-ipc | caminhos/permissoes, hello/token, request antes do hello, eventos, encerramento | 10 |
| dots-engines | versao, logado/deslogado, JSON do Claude, AGY desconhecido, ausente, timeout | 9 |
| dotsd | parser de args, lock, e2e com binario real (status, DOT, instancia duplicada, shutdown IPC e SIGTERM, reinicio) | 4 |
| desktop (vitest) | stores runtime/dots/engines | 11 |

Comandos: `pnpm test:rust`, `pnpm test:ui`.

## Validacao dos criterios de aceite (2026-10-15, bundle de release)

| Criterio | Resultado |
|---|---|
| App instalavel e iniciado no macOS | `DOTS_0.1.0_aarch64.dmg` gerado; `DOTS.app` aberto via Finder/`open`; assinatura ad-hoc |
| SQLite sem instalacao adicional | `dots.db` criado em `~/Library/Application Support/DOTS`, migracao 1 aplicada, `PRAGMA integrity_check` = ok |
| Dados persistem apos reinicializacao | DOT criado; runtime morto com `kill -9` e reiniciado pelo launchd; DOT e motores continuam presentes |
| Criar e editar DOTs | CRUD coberto por testes de storage, e2e do binario e stores da UI; criacao real validada contra o runtime em producao |
| Motores instalados identificados | Codex 0.162.1 (autenticado), Claude 2.1.300 (autenticado), AGY 1.3.5 (login nao verificavel), resolvidos em `~/.local/bin` sem PATH do shell |
| Runtime independente da janela | App fechado (`quit`): `dotsd` segue vivo, mesmo pid; app reaberto reconecta sem reiniciar o runtime |
| Instancia unica | Segunda abertura do app nao cria processo novo; segundo `dotsd` sai com codigo 3 |
| Parar explicitamente | `launchctl bootout` (botao Parar): saida limpa (codigo 0), socket/token/runtime.json removidos, nao reinicia |
| Testes automatizados | 64 testes verdes (53 Rust + 11 vitest) (`cargo test --workspace` + `vitest`) |
| Atualizacao nao apaga dados | Teste `upgrade_with_new_migration_keeps_data_and_creates_backup`: backup `pre-migrate-v1` gravado e dados preservados; binario antigo recusa banco mais novo |

Nao validado por clique: a interface foi exercitada pelo build (vue-tsc + Vite), pelos testes das stores e pelo
runtime real via IPC; a navegacao manual nas telas fica para a conferencia do usuario.

## Limitacoes conhecidas

- AGY 1.3.5 nao expoe verificacao de login nao interativa; o status fica "nao verificavel".
- Bundle sem assinatura de desenvolvedor nem notarizacao: Gatekeeper exige "Abrir mesmo assim".
- Build apenas arm64; x64/Windows/Linux estao previstos no codigo (caminhos, `cfg`), nao validados.
- Isolamento de workspace e apenas logico (diretorio por DOT); sandbox nativo fica para a Fase 2.
- A comparacao do sidecar com a copia em `bin/` usa nome por versao e tamanho; builds distintos da
  mesma versao com o mesmo tamanho nao sao recopiados (irrelevante em releases versionadas).
- Launchd nao herda permissoes TCC concedidas ao app (relevante so quando os DOTs acessarem pastas protegidas).
