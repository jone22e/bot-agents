# Navegador por DOT (spike, 2026-10-15)

Cada DOT tem um navegador proprio: um processo Chrome/Chromium com perfil isolado
(`<dados>/browsers/<dot_id>/`, passado como `--user-data-dir`), controlado pelo `dotsd` por CDP
(Chrome DevTools Protocol) sobre `--remote-debugging-pipe`. Nenhuma porta e aberta e nenhuma
dependencia externa foi adicionada: o CDP por pipe e JSON terminado em `\0` nos descritores 3 e 4,
implementado em `crates/dots-runtime/src/browser.rs`.

Por ser ferramenta do gateway, funciona igual nos tres motores (Claude, Codex, AGY) e passa pela
mesma politica, aprovacao e auditoria das demais.

## Ferramenta

`dots_browser_open { url }`: abre a URL (http/https) na aba de trabalho do navegador do DOT, espera
`document.readyState === "complete"` (ate 20 s) e devolve titulo, URL final e a arvore de
acessibilidade em texto: uma linha por elemento, `- role "nome" [ref=<backendDOMNodeId>] prop=valor`
(propriedades `url`, `checked`, `expanded`, `selected`, `disabled`, `required`, `level` e `value`).
Se a arvore vier vazia, devolve `document.body.innerText`. Saida limitada a 64 KB.

Politica (`policy.rs`): exige `allow_network`; risco `Exec`; apenas `http://` ou `https://`. Com
`auto_approve_level = exec` nao pede aprovacao; abaixo disso cada abertura aguarda o usuario.

## Ciclo de vida

- O navegador sobe na primeira chamada do DOT e fica vivo (perfil, cookies e logins persistem entre
  tarefas) ate o runtime encerrar (`BrowserPool::shutdown_all`, SIGTERM no grupo de processos e
  SIGKILL apos 3 s) ou o DOT ser excluido (`dots.delete` fecha o processo e apaga o perfil).
- Se o processo morrer, a proxima chamada inicia outro com o mesmo perfil.
- Binario: primeiro executavel entre Google Chrome (`/Applications`, `~/Applications`), Chromium do
  cache do Playwright (`~/Library/Caches/ms-playwright/chromium-<n>`, o mais novo primeiro),
  Chromium, Brave e Edge. Configuracoes (sem tela ainda): `browser.executable` (caminho fixo) e
  `browser.headless` (padrao `true`; `false` abre a janela, util para o usuario fazer login em sites
  que o DOT usara).
- Ambiente do processo: so `HOME`, `USER`, `LOGNAME`, `TMPDIR`, `LANG`, `PATH=/usr/bin:/bin`.

## Verificado em 2026-10-15 (Google Chrome 155, macOS)

- `browser::tests::launches_real_browser_with_isolated_profile`: sobe o Chrome detectado, abre duas
  paginas na mesma aba, le heading, link e textbox da arvore de acessibilidade, cria o perfil no
  diretorio informado e encerra.
- `tests/browser_e2e.rs`: CLI falso -> gateway MCP real -> `dots_browser_open` em um servidor HTTP
  local; DOT com rede liberada recebe a pagina e ganha `browsers/<id>/`; DOT sem rede e negado pela
  politica sem criar perfil; auditoria registra `tool.dots_browser_open` com `ok` e `denied`;
  excluir o DOT apaga o perfil; apos SIGTERM no runtime nenhum processo Chrome sobra.

## Fora do spike

Interacao (clicar, digitar, rolar, voltar) usando os `ref`, captura de tela no workspace, lista de
dominios permitidos por DOT, tela de configuracao (binario, janela visivel, "fazer login agora"),
eventos CDP (hoje ignorados) e limite de ociosidade para derrubar navegadores parados.
