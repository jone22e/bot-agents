# ADR 0005: Tool Gateway como servidor MCP embutido no `dotsd`

Data: 2026-10-15. Estado: aceito.

## Contexto
Os tres CLIs tem ferramentas internas proprias; a especificacao exige que toda ferramenta passe por autorizacao fora da LLM
e que o runtime seja a autoridade final.

## Decisao
`dotsd mcp-gateway` e um servidor MCP (stdio, JSON-RPC 2.0) que o CLI inicia; cada `tools/call` vira `gateway.call` no runtime
pelo socket IPC com um token por execucao (`DOTS_EXECUTION_TOKEN` no ambiente ou `<workspace>/.dots/execution.token`).
O runtime avalia a politica do DOT (caminhos canonicos dentro dos diretorios autorizados, allowlist/denylist de comandos,
classe de risco), pede aprovacao quando necessario, executa, audita e registra alteracoes no workspace.
Claude: ferramentas internas desligadas (`--tools ""`) por padrao e `--permission-prompt-tool` apontando para o gateway.
Codex: sandbox proprio + `default_tools_approval_mode="approve"` para o servidor `dots`. AGY: registro global + regras `mcp(dots/<tool>)`.

## Consequencias
- Aprovacao por acao e roteavel no Claude (nativas e MCP) e no gateway para todos; no Codex/AGY as ferramentas nativas ficam
  sob o sandbox/negacao do proprio CLI.
- Implementacao propria do protocolo MCP (quatro metodos) em vez de crate externa: menos dependencia, totalmente verificavel.
