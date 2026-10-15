# ADR 0001: runtime separado (`dotsd`) supervisionado pelo launchd

Data: 2026-10-15. Estado: aceito.

## Contexto
O runtime precisa sobreviver ao fechamento da janela, a falhas e a reinicializacoes, sem terminal aberto.

## Decisao
`dotsd` e um binario Rust proprio, embutido no app como sidecar Tauri. Em producao o app copia o
binario para `~/Library/Application Support/DOTS/bin/dotsd-<versao>` e registra um LaunchAgent
`com.c2s.dots.runtime` apontando para essa copia, com `RunAtLoad` e `KeepAlive{SuccessfulExit=false}`.
O app e apenas cliente IPC. Em dev (ou se o launchd falhar) o app inicia o runtime desacoplado.

## Alternativas rejeitadas
- Runtime dentro do processo do app: morre com a janela.
- LaunchAgent apontando para dentro do `.app`: quebra em atualizacoes, ao mover o app e com App Translocation.
- `KeepAlive=true`: reiniciaria o runtime apos um "Parar" explicito.
