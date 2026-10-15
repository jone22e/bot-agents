# ADR 0013: conector Flexi dentro do runtime, sessao por cookie e segredos no Keychain

Data: 2026-10-15. Estado: aceito.

## Contexto
O Flexi nao tem mais servidor MCP para marketplaces (removido em 2026-10-14) e os tokens de integracao `fit1.` nao alcancam
as rotas de anuncios. O usuario pediu conectar com um usuario que nao e necessariamente o logado e sem alterar o Flexi.

## Decisao
O `dotsd` faz login no Flexi por e-mail e senha (com 2FA) e guarda o cookie `userdata` e, se o usuario marcar, a senha no
Keychain do macOS (`keyring`, servico `com.c2s.dots`). O SQLite guarda apenas metadados da conexao. As ferramentas `flexi_*`
sao descritas em um catalogo (rota, metodo, parametros, risco) e executadas pelo gateway MCP que ja existe, sob a mesma
politica e central de aprovacoes dos outros tools. Cada DOT aponta para uma conexao; com uma so, o vinculo e automatico.

## Consequencias
- Tudo que um DOT faz no Flexi fica auditado la no nome do usuario conectado, e no DOTS em `audit_logs`.
- Sessao expirada: relogin automatico com a senha guardada, ou estado `needs_login` com aviso na interface.
- Se o Flexi voltar a expor MCP com ferramentas de marketplace, o catalogo pode ser trocado sem mudar os DOTs.
