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

## Acoes na pagina (2026-10-15)

Depois de `dots_browser_open`, o DOT age na aba pelos `ref` da ultima leitura. Cada acao devolve a pagina atualizada
(mesmo formato do `open`, com refs novos); ref de uma pagina que ja mudou da erro pedindo nova leitura.

- `dots_browser_click { ref }`: rola ate o elemento e clica no centro dele com o mouse (`DOM.getContentQuads` +
  `Input.dispatchMouseEvent`); sem caixa visivel, `element.click()`.
- `dots_browser_type { ref, text, clear = true, submit = false }`: `DOM.focus`, seleciona o conteudo se `clear`,
  `Input.insertText` (dispara os eventos de digitacao, funciona com React) e Enter se `submit`.
- `dots_browser_select { ref, option }`: opcao de `<select>` pelo valor ou pelo texto (setter nativo + `input`/`change`).
- `dots_browser_press { key }`: Enter, Escape, Tab, Backspace, Delete, Space, setas, PageUp/PageDown, Home, End.
- `dots_browser_read {}`: le a pagina atual de novo, sem navegar.
- Dialogos nativos (`alert`/`confirm`/`prompt`): o runtime acompanha `Page.javascriptDialogOpening`; enquanto um esta
  aberto as acoes devolvem o texto dele e so `dots_browser_press` responde (Enter aceita, Escape cancela).

Politica (`policy.rs`, `browser_action`): o gateway le do navegador a URL atual (`_page_url`), o papel e nome do
elemento (`_element`, via `Accessibility.getPartialAXTree`) e o dialogo aberto (`_dialog`); o que o DOT mandar com
esses nomes e descartado. Pagina fora dos apps conectados exige `allow_network`.

| Acao | Risco |
|---|---|
| ler, digitar, escolher opcao, teclas, cancelar dialogo | igual a abrir a pagina: leitura no app conectado, exec nos demais sites |
| clicar em elemento cujo nome tem verbo que grava (salvar, enviar, confirmar, criar, atualizar, aplicar, pausar, ativar, alterar, sim, ok...) ou sem nome | escrita (no minimo) |
| clicar em verbo irreversivel (excluir, exclusao, deletar, remover, apagar, finalizar, publicar, desvincular, encerrar, descartar, pagar, comprar), exceto se o nome tem "cancel" | critico: sempre pede aprovacao |
| aceitar dialogo (Enter) | escrita; critico se o texto do dialogo tem verbo irreversivel |

Os verbos sao comparados no infinitivo, sem acento: abas de filtro como "Excluidos" ou "Publicados" contam como leitura.
Na equipe do Flexi (aprovacao automatica so de leitura), clique que grava ou e irreversivel vai para aprovacao: de
funcionario em pedido delegado, quem decide e o gerente; do gerente, o usuario.

## Ciclo de vida

- O navegador sobe na primeira chamada do DOT e fica vivo (perfil, cookies e logins persistem entre
  tarefas) ate o runtime encerrar (`BrowserPool::shutdown_all`, SIGTERM no grupo de processos e
  SIGKILL apos 3 s) ou o DOT ser excluido (`dots.delete` fecha o processo e apaga o perfil).
- Se o processo morrer, a proxima chamada inicia outro com o mesmo perfil.
- Binario: primeiro executavel entre Google Chrome, Chromium do cache do Playwright (o mais novo
  primeiro), Chromium, Brave e Edge. No macOS, procura em `/Applications`, `~/Applications` e
  `~/Library/Caches/ms-playwright`; no Linux, em `/usr/bin`, `/opt/google/chrome`, `/snap/bin` e
  `~/.cache/ms-playwright`. Configuracoes (sem tela ainda): `browser.executable` (caminho fixo) e
  `browser.headless` (padrao `true`; `false` abre a janela, util para o usuario fazer login em sites
  que o DOT usara).
- Ambiente do processo: so `HOME`, `USER`, `LOGNAME`, `TMPDIR`, `LANG`, `PATH=/usr/bin:/bin`.

## Assistir ao vivo ("Computador do DOT")

No chat, o botao de monitor no topo abre o painel lateral **Computador de <DOT>**, com a aba do navegador dele ao vivo,
como o "computer" dos agentes do ChatGPT. Funciona tambem sem janela (headless): o runtime liga o screencast do CDP
(`Page.startScreencast`, JPEG 55%, ate 1280x900) e repassa cada quadro como evento IPC
`browser.frame {dot_id, data (base64), width, height}`; a interface mostra o ultimo quadro e a URL/titulo atuais.

- IPC: `browser.status {dot_id}` -> `{running, watching, url, title, binary}` (nao inicia o navegador);
  `browser.watch {dot_id}` inicia o navegador se preciso, liga o screencast e devolve o estado com `started`;
  `browser.unwatch {dot_id}` encerra.
- O `watch` vale 15 s; a interface renova a cada 5 s enquanto o painel esta aberto, entao fechar o app ou o painel
  encerra a transmissao sozinho. Com o painel fechado o Chrome nao gera quadros.
- Se a aba for recriada, o screencast e religado. O navegador continua vivo depois de fechar o painel.
- Somente visualizacao: o usuario ainda nao clica nem digita pelo painel (o DOT ja clica e digita, ver "Acoes na pagina").

## Verificado em 2026-10-15 (Google Chrome 155, macOS)

- `browser::tests::launches_real_browser_with_isolated_profile`: sobe o Chrome detectado, abre duas
  paginas na mesma aba, le heading, link e textbox da arvore de acessibilidade, cria o perfil no
  diretorio informado e encerra.
- `tests/browser_e2e.rs`: CLI falso -> gateway MCP real -> `dots_browser_open` em um servidor HTTP
  local; DOT com rede liberada recebe a pagina e ganha `browsers/<id>/`; DOT sem rede e negado pela
  politica sem criar perfil; auditoria registra `tool.dots_browser_open` com `ok` e `denied`;
  excluir o DOT apaga o perfil; apos SIGTERM no runtime nenhum processo Chrome sobra. `browser.status` ve a
  pagina aberta, `browser.watch` entrega quadros JPEG pelo IPC, a renovacao nao abre outra transmissao e `unwatch` encerra.
- `browser::tests::real_browser_clicks_types_selects_and_answers_dialogs`: clique, digitacao substituindo o texto e com
  Enter, opcao de `<select>` (e erro listando as opcoes), `confirm` cancelado e aceito, ref vencido apos navegar.
- `tests/browser_e2e.rs` tambem clica, pelo gateway, em "Excluir anuncio": a chamada fica pendente como `critical` com
  o resumo `navegador: clicar em button "Excluir anuncio" (...)` e so clica depois de aprovada.
- `policy::tests::browser_actions_by_label_and_page`: riscos da tabela acima.
- `browser::tests::launches_real_browser_with_isolated_profile` tambem confirma que o screencast entrega um quadro JPEG
  com `deviceWidth` > 0 e que `page_info` le URL e titulo sem navegar.

## Fora do spike

Rolar e voltar como acoes proprias, controle do usuario pelo painel (assumir para fazer
login), captura de tela no workspace, lista de dominios permitidos por DOT, tela de configuracao (binario, janela
visivel) e limite de ociosidade para derrubar navegadores parados.
