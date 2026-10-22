# ADR 0015: canal WebSocket persistente entre o runtime e o Flexi web

Data: 2026-10-22. Estado: aceito.

## Contexto
O Flexi web precisa mostrar, na coluna da direita, a Central de Aprovacoes e a Avaliacao de desempenho dos DOTs do
usuario, e decidir aprovacoes de la. Esses dados vivem so no SQLite local do `dotsd`; o canal com o Flexi era de mao
unica (REST com cookie, ADR 0013) e nao ha servidor local no DOTS (ADR 0002 rejeita TCP em localhost).

## Decisao
O `dotsd` abre um WebSocket persistente em `{base_url}/api/ws/agentes-dots` (`wss` para `https`, `ws` para `http`),
autenticado pelo cookie `userdata` da conta do usuario (`account.flexi_connection_id`) e com o mesmo `User-Agent` do
conector (`DOTS/<versao> (conector Flexi)`). O Flexi responde 401 sem sessao e fecha o socket antigo com codigo 4001
("replaced") quando o mesmo usuario conecta de novo. O Flexi web consulta o backend, que repassa ao app pelo socket;
nada e copiado para o banco do Flexi. "Vinculado" no Flexi significa "app conectado agora".

Mensagens sao frames de texto JSON. Ping/pong usa frames de controle.

App -> Flexi:
- `{"type":"hello","app":"dots","version":"<versao>","pending_approvals":N,"dots":N}` logo apos conectar.
- `{"type":"state","pending_approvals":N,"dots":N}` em `approval.requested`, `approval.resolved`, `dots.changed`, `teams.changed`.
- `{"type":"response","id":"<id>","ok":true,"result":<json>}` ou `{"type":"response","id":"<id>","ok":false,"error":"<msg>"}`.

Flexi -> App:
- `{"type":"ready"}` depois do hello.
- `{"type":"request","id":"<id>","method":"<m>","params":{...}}`.

Metodos atendidos (`flexi_link.rs`):
- `approvals.list` `{pending_only, limit}` -> `Approval[]` (mesmo JSON do IPC `approvals.list`).
- `approvals.decide` `{id, approved, reason?}` -> `Approval`; usa `decide_approval_by(..., "user", "o usuario (pelo Flexi)")`
  e grava a auditoria extra `approval.decide_via_flexi_web`.
- `performance.summary` `{days: 7|30}` -> `{start, end, days, total: DotPerformance, dots: DotPerformance[]}`; o calculo
  (`performance.rs`) e um porte fiel de `apps/desktop/src/lib/performance.ts`, com campos em camelCase.
- `status` `{}` -> `{pending_approvals, dots, version, started_at}`.
- Metodo desconhecido -> `ok:false`.

Cliente: tarefa de fundo `flexi_link::run`. Sem conta ou sem cookie, espera 30 s ou `apps.changed`. Reconexao com
backoff de 5 s dobrando ate 60 s; em 401/403 chama `flexi_check` para renovar a sessao antes de tentar de novo.
O estado do canal fica em memoria (`RuntimeService::flexi_link`), e exposto pelo IPC `apps.flexi.link_status` e
emitido como evento `flexi_link.changed` nas transicoes; o `EventSink` ganhou `subscribe()` para o runtime ouvir os
proprios eventos.

O app e quem vigia o canal (o custo fica no desktop, nao no servidor): manda um ping a cada 30 s e, sem nenhum frame
do Flexi por 75 s, fecha e reabre sozinho (um socket morto em silencio, por troca de rede ou descarte no servidor,
ficava "conectado" para sempre). O IPC `apps.flexi.link_reconnect` (botao "Reconectar canal" no cartao Flexi) emite
`flexi_link.reconnect`, e a tarefa de fundo fecha e reabre na hora.

## Consequencias
- Com o app fechado, o Flexi nao mostra nada dos agentes: o menu depende do app conectado.
- Decisoes feitas no Flexi web passam pela mesma politica e auditoria das feitas no app.
- A logica de desempenho existe em TS (app) e em Rust (runtime); mudancas de regra precisam ser feitas nas duas.
- Nova dependencia: `tokio-tungstenite` com rustls (sem native-tls), alinhada ao `reqwest` ja usado.
