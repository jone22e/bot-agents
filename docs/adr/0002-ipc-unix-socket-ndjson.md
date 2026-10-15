# ADR 0002: IPC por Unix socket com NDJSON (subconjunto JSON-RPC)

Data: 2026-10-15. Estado: aceito.

## Decisao
Conexao persistente por cliente, requisicoes multiplexadas por `id`, eventos push com `seq`.
Autenticacao: diretorio 0700 + socket 0600 + uid do peer + token rotacionado por start enviado no `hello`.
O caminho do socket e validado contra o limite de 104 bytes do macOS com fallback para `$TMPDIR`.

## Alternativas rejeitadas
- TCP em localhost: exposto a qualquer processo local; exige porta.
- Uma conexao por requisicao: inviabiliza eventos e multiplica handshakes.
