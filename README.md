# DOTS

Plataforma desktop (macOS e Linux x86_64) de agentes autonomos persistentes. Cada DOT e um
trabalhador digital com identidade, objetivo, instrucoes, memoria e um motor de IA
(Codex CLI, Claude Code ou AGY) usado como ferramenta de execucao.

Estado atual: **Fase Flexi** (apos a Fase 3: gatilhos, checkpoints, delegacao, Supervisor, central de operacoes). Os DOTs
se conectam ao Flexi com o usuario que voce escolher e trabalham com anuncios, perguntas, analises e Google Ads por meio de
ferramentas `flexi_*` sob a mesma politica de permissoes. O Hub e a comunicacao entre computadores chegam na Fase 4.

- Instalacao e uso: [docs/INSTALL.md](docs/INSTALL.md)
- Instalacao no Ubuntu/Debian: [docs/INSTALL-LINUX.md](docs/INSTALL-LINUX.md)
- Arquitetura: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Decisoes: [docs/adr/](docs/adr/)
- Resumo tecnico da Fase 1: [docs/PHASE1.md](docs/PHASE1.md)
- Resumo tecnico da Fase 2: [docs/PHASE2.md](docs/PHASE2.md)
- Resumo tecnico da Fase 3 e limitacoes: [docs/PHASE3.md](docs/PHASE3.md)
- Fase Flexi (conexao, ferramentas, equipe de marketplaces): [docs/PHASE-FLEXI.md](docs/PHASE-FLEXI.md)
- Comportamento verificado dos CLIs: [docs/engines/README.md](docs/engines/README.md)
- Navegador por DOT (spike): [docs/BROWSER.md](docs/BROWSER.md)
- Principios de interface: [docs/UX.md](docs/UX.md)

## Desenvolvimento

Pre-requisitos: Xcode (ou Command Line Tools), Rust estavel via rustup, Node 24 e pnpm.

```bash
pnpm install
cargo build -p dotsd                # runtime (fica em target/debug/dotsd, usado pelo app em dev)
scripts/build-sidecar.sh debug      # copia o runtime como sidecar do app
pnpm dev                            # abre o app (tauri dev); o app inicia o runtime sozinho
```

Testes:

```bash
pnpm test:rust    # cargo test --workspace (storage, ipc, engines, runtime e2e)
pnpm test:ui      # vitest (stores)
pnpm gen:types    # regenera packages/shared-types a partir do Rust
```

Build do instalador (`.app` e `.dmg` em `target/release/bundle/`):

```bash
scripts/build-sidecar.sh release
pnpm build
```

Pacote Ubuntu/Debian x86_64 (`.deb` em `tmp/linux/`):

```bash
make build-linux  # requer Docker; make update inclui o pacote no release
```
