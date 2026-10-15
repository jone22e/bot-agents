# Testes

Os testes automatizados vivem junto de cada modulo:

- `crates/dots-core/src/**` — testes unitarios (validacao, ids, serde) e geracao de tipos TS.
- `crates/dots-storage/tests/storage_tests.rs` — migracoes idempotentes, upgrade com backup e preservacao de dados, CRUD de DOTs, auditoria, ator concorrente.
- `crates/dots-ipc/tests/ipc_tests.rs` — handshake, token invalido, request antes do hello, eventos com `seq`, encerramento.
- `crates/dots-engines/tests/detect_tests.rs` — deteccao com CLIs falsos (instalado/ausente/logado/deslogado/timeout).
- `crates/dots-runtime/tests/dotsd_e2e.rs` — sobe o binario real: status, criacao de DOT, instancia duplicada (codigo 3), shutdown por IPC e SIGTERM, reinicio preservando dados.
- `crates/dots-runtime/tests/browser_e2e.rs` — navegador por DOT pelo gateway real com Chrome de verdade (ignorado sem Chrome/Chromium): perfil isolado, negacao sem rede, auditoria, limpeza ao excluir e ao encerrar.
- `apps/desktop/src/stores/*.test.ts` e `src/lib/humanize.test.ts` — stores Pinia (chat, contatos, tarefas, aprovacoes, motores, runtime) e frases, com `invoke` simulado.

Executar tudo: `pnpm test:rust && pnpm test:ui`.
