# Fase 2: resumo tecnico, validacao e limitacoes

Data: 2026-10-15. Plataforma validada: macOS 26.2, Apple Silicon. Versao: 0.2.0.

## Entregue

- **Engine Manager** (`dots-engines`): trait `AgentEngine`, adaptadores Codex/Claude/AGY com flags verificadas, parsers testados
  contra capturas reais, runner de processos (grupo proprio, stdout/stderr em streaming, timeout, SIGINT->SIGKILL),
  semaforos de concorrencia, classificacao de falhas (auth, config, motor, timeout, cancelado, orcamento).
- **Tool Gateway** (`dotsd mcp-gateway` + `gateway.rs`): ferramentas `dots_list_dir`, `dots_read_file`, `dots_search`,
  `dots_write_file`, `dots_delete_file`, `dots_run_command`, `dots_git`, `dots_system_info`, `dots_remember`, `dots_recall`
  e `permission_prompt` (Claude). Politica por DOT (nivel de auto-aprovacao, comandos permitidos, rede, tamanho de arquivo,
  ferramentas nativas), denylist global, caminhos canonicos, symlinks que escapam negados, auditoria e `workspace_changes`.
- **Aprovacoes**: central com solicitante, tarefa, operacao, recursos, risco e parametros; validade de 10 min; a execucao fica
  `WAITING_APPROVAL` ate a decisao; aprovacao inline na conversa.
- **Task Manager**: fila em SQLite, scheduler por notificacao, estados completos, cancelamento, retry, pausa geral,
  recuperacao no boot com encerramento de processos orfaos.
- **Conversas**: streaming de texto (deltas), chips de ferramentas, estado do motor, cancelar, anexos de arquivos autorizados,
  continuidade de sessao por motor.
- **Workspaces**: diretorios externos autorizados por dialogo nativo com leitura/escrita; `.dots/` reservado; historico de alteracoes.
- **Memoria**: quatro tipos, FTS5, injecao limitada, edicao/exclusao/busca na UI, `dots_remember`/`dots_recall`.
- **UI**: Conversas, Atividades (fila, detalhe, execucoes, eventos, alteracoes, retry/cancelar, pausar fila), Aprovacoes,
  abas Permissoes/Memoria no DOT, Visao geral com numeros reais, integracao AGY na tela Motores.
- **Migracao 0002** (nova; 0001 intocada) com backup automatico ao atualizar.

## Testes automatizados

| Suite | Qtd |
|---|---|
| dots-core | 45 |
| dots-storage (inclui tarefas, execucoes, conversas, FTS, aprovacoes, politicas, workspaces) | 15 |
| dots-ipc | 10 |
| dots-engines (parsers com fixtures, runner, manager com CLI falso) | 23 |
| dotsd (politica, lock, args, e2e Fase 1, e2e Fase 2 com CLI falso que fala MCP: escrita, bloqueio, aprovacao/rejeicao, cancelamento, dois DOTs, conversa com resume e recall, recuperacao apos SIGKILL) | 12 |
| desktop (vitest) | 14 |

## Validacao dos criterios de aceite (CLIs reais, 2026-10-15)

| Criterio | Resultado |
|---|---|
| Criar DOT, selecionar motor, enviar tarefa, acompanhar em tempo real | feito com Claude, Codex e AGY; eventos `execution.event` ao vivo e persistidos |
| Executar ferramentas autorizadas e receber resultado | `dots_write_file` + `dots_list_dir` pelos tres motores; arquivos `hello-<motor>.txt` criados; resposta "FEITO" |
| Preservar historico e memoria; reiniciar sem perder | teste `restart_recovers_interrupted_tasks` (SIGKILL + reinicio) e e2e de conversa com `--resume` e memoria recuperada por `dots_recall` |
| Dois DOTs simultaneos | Claude e Codex `RUNNING` ao mesmo tempo (`dots_running = 2`) com CLIs reais; tambem no e2e |
| Cancelar tarefa | e2e: `CANCELLED` em < 8 s, processo encerrado |
| Bloquear operacao nao autorizada | escrita em `/etc/...` negada pelo runtime, auditada com `outcome=denied`; comandos `sudo`/`rm -rf` negados em testes de politica |
| Tarefa real concluida com CLI instalado | Claude (custo US$ 0,16), Codex e AGY; registrado em `docs/engines/README.md` |

## Validacao em producao (bundle 0.2.0 sobre os dados da Fase 1)

- Abrir `DOTS.app` 0.2.0 com o banco v1 existente: `bin/dotsd-0.2.0` copiado, LaunchAgent reescrito para a nova versao,
  backup `dots-pre-migrate-v1-<ts>.db` gravado, esquema migrado para v2, DOT "Validacao Fase 1" preservado.
- Conversa real com Claude pelo runtime em launchd: `dots_write_file` com politica `read` gerou aprovacao pendente
  (resumo, risco `write`, DOT solicitante); aprovada via IPC, arquivo `fase2.txt` criado, resposta "FEITO." na conversa
  com chip da ferramenta; nenhum processo `mcp-gateway` restante ao fim.
- Observacao: na primeira abertura apos o build, uma instancia do app ja estava em execucao desde o momento do empacotamento
  e conectou ao runtime antigo (0.1.0). Apos encerrar essa instancia e reabrir, a atualizacao ocorreu como esperado.
  Nao foi reproduzido; fica registrado para acompanhamento.

## Revisao de interface (apos a validacao)

A pedido do usuario, as telas foram simplificadas para uso sem vocabulario tecnico: criacao de DOT em tres campos com
modelos prontos, permissoes em frases, historico em frases, faixa global de pedidos de permissao e dados tecnicos
recolhidos. Detalhes em `docs/UX.md`. Testes do frontend: 19.

## Limitacoes conhecidas

- Aprovacao por acao para ferramentas **nativas** so existe no Claude (via `permission_prompt`). No Codex `exec` e no AGY headless
  as ferramentas nativas ficam sob o sandbox/negacao do proprio CLI; as ferramentas do DOTS passam pela central em todos.
- AGY exige configuracao global (servidor `dots` + regras) e o caminho registrado aponta para o binario da versao atual;
  apos atualizar o DOTS, reaplicar em Motores de IA. O ambiente do servidor MCP no AGY e fixo (`DOTS_DATA_DIR`), por isso o
  token de execucao vem do arquivo `<workspace>/.dots/execution.token`.
- Anexos sao inseridos como texto no prompt (limite de 40 k caracteres); imagens nao sao suportadas ainda.
- Compactacao de memoria por resumo com LLM nao implementada (poda por quantidade apenas).
- Isolamento de workspace para o Claude depende de desligar as ferramentas nativas; com `engine_native_write` ligado, a
  protecao e a politica de aprovacao, nao um sandbox de SO.
- `dots_search` usa `/usr/bin/grep`; `dots_git` usa `/usr/bin/git` (presentes no macOS com Xcode CLT).
- Custo so e conhecido para o Claude (`total_cost_usd`); Codex/AGY informam apenas tokens.
