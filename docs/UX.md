# Principios de interface (revisao de 2026-10-15)

## Modelo de mensageiro (decisao do usuario, 2026-10-15)

A interface e um mensageiro, nao um painel:

- **Barra lateral** = lista de DOTs como contatos: avatar com iniciais, nome, previa da ultima mensagem (ou o estado atual:
  "Trabalhando: …", "Pede sua permissao"), hora, ponto de estado (azul trabalhando, ambar esperando voce) e contador de pedidos.
  Em cima, busca e "+" (novo DOT). Embaixo, avatar do usuario com menu discreto (Atividades, Configuracoes, Conectar aplicativos)
  e o botao "Conectar aplicativos".
- **Conversa** = uma thread por DOT (a conversa principal). Suas mensagens a direita, as do DOT a esquerda, separadores de data.
  Trabalhos iniciados por gatilhos, horarios ou outros DOTs entram na mesma thread com uma nota de origem e a resposta do DOT.
- **Pedidos de permissao e perguntas** sao cartoes dentro da conversa ("Posso escrever o arquivo x? Permitir / Negar";
  perguntas com opcoes). O contato fica ambar ate a resposta.
- **O que ele fez** fica sob a mensagem como "N acoes", expansivel. Sem JSON.
- **Novo DOT** em tres passos: escolher um modelo pronto (Revisor, Redator, QA, Assistente) ou dar um nome; dizer o que ele faz;
  confirmar. Assistente de IA e nivel de permissao vem com padroes seguros em uma linha ("Usa Claude Code · trocar",
  "So le e pergunta antes de mudar · mudar"). Pastas, gatilhos, equipe, papel de supervisor e limites ficam no perfil.
- **Computador do DOT** (botao de monitor no topo da conversa) abre um painel lateral com o navegador dele ao vivo,
  numa moldura de janela com a URL, como o "computer" dos agentes do ChatGPT: "Conectando ao computador de X…" ate o
  primeiro quadro, depois "Ao vivo". So visualizacao por enquanto; abrir o painel e o perfil sao exclusivos.
- **Perfil do DOT** abre ao clicar no nome no topo: abas Sobre, Permissoes, Gatilhos, Equipe, Memoria; pausar, duplicar,
  arquivar, excluir.
- **Entrada**: clipe para anexar, seta para enviar, quadrado para parar. Sem microfone (nao ha transcricao no projeto).
- **Colunas ajustaveis**: a lista de contatos, o perfil e o computador do DOT tem uma alca vertical na borda (arrastar
  muda a largura, duplo clique volta ao padrao); a largura fica guardada no localStorage (`dots.column.<nome>`).
- **Configuracoes**: pagina com navegacao lateral e icones (Geral, Servico em segundo plano, Assistentes de IA, Copias de
  seguranca, Diagnostico), interruptores para "permitir que os DOTs trabalhem" e "iniciar com o Mac".
- **Conectar aplicativos** tem o cartao **Flexi** (os assistentes de IA ficam em Configuracoes > Assistentes de IA): e-mail e senha do usuario que o DOT
  vai usar (nao precisa ser o seu; a empresa e a base vem da conta, como no login do Flexi), opcao marcavel "guardar a senha no Keychain para reconectar sozinho", codigo 2FA quando
  pedido, estado da conexao e "Montar equipe de marketplaces". Com o Flexi conectado, o **Novo DOT** oferece os papeis de
  marketplaces (Gerente, subgerentes e agentes de Mercado Livre, Shopee, Google Ads e Publicacao) no lugar dos modelos
  genericos, o passo 3 pergunta se e Gerente ou Funcionario e de qual equipe, e o passo final se so consulta ou pode editar no Flexi.
  No passo do nome, gerente escolhe o setor (Marketplaces (tudo), Mercado Livre, Shopee, Google); funcionario ve chips de setor
  (pre-selecionado pelo nome da equipe) e so os papeis daquele setor, com nome curto ("Criador de anuncios", "Revisor de
  anuncios", "Atendimento de perguntas").
- **Equipes**: a lista de contatos agrupa os DOTs por equipe (gerente primeiro, com o cargo abaixo do nome; funcionarios em
  linha compacta sob ele; "Sem equipe" no fim). O usuario conversa com o gerente: perguntas que um funcionario faz dentro de um
  pedido delegado vao ao gerente (tarefa "Pergunta de X", respondida com `dots_answer_question`) e o funcionario aparece como
  "Perguntou ao gerente" ou "Pediu permissao ao gerente", sem pedir atencao do usuario. Na linha compacta o nome vem
  inteiro e o cargo sem o prefixo da area ("Publicacao"). A tela Equipes (menu
  do avatar) cria, renomeia, define gerente e funcionarios. A aba Equipe do perfil mostra "Gerente de" e "Funcionario de". O perfil ganha a aba **Aplicativos** (qual conexao o DOT usa). Uma conexao que precisa de atencao aparece como
  aviso ambar no topo da lista de contatos.
- **Atividades** continua como pagina (atalho no menu do avatar) para a lista completa de trabalhos, o que aconteceu, pedidos
  pendentes e o painel de operacoes.

O menu de sete itens da especificacao foi substituido por esse modelo a pedido do usuario; Atividades e Configuracoes
permanecem como atalhos discretos.

## Revisao anterior (simples por padrao, detalhes sob demanda)

Objetivo: uma pessoa que ja usa Codex, Claude Code ou Antigravity no proprio Mac deve operar o DOTS sem
conhecer runtime, socket, workspace, politica ou eventos. Regra geral: **simples por padrao, detalhes sob demanda**.

## O que o usuario ve

| Tela | Conteudo principal | Detalhes tecnicos (recolhidos) |
|---|---|---|
| Visao geral | DOTs com uma linha de estado ("Trabalhando em…", "Esperando sua permissao", "Livre"), pedidos de permissao, assistentes prontos | — |
| Meus DOTs | cartoes com Conversar / Editar e menu com Duplicar, Pausar, Arquivar, Excluir | — |
| Novo DOT | 3 campos: nome, "o que ele faz", assistente de IA (pre-escolhido); modelos prontos; 3 niveis de confianca em frases | "Avancado": pasta, tempo, gasto, trabalhos simultaneos |
| Permissoes do DOT | niveis em frases; comandos liberados so no nivel "pode rodar comandos"; pastas com "Dar acesso a uma pasta…" | "Avancado": rede, ferramentas nativas do assistente, tamanho de arquivo |
| Conversas | chat; estado em uma palavra; pedido de permissao inline com Permitir/Negar | — |
| Atividades | "Pedir um trabalho"; lista com estados em portugues; "O que aconteceu" em frases ("Escreveu o arquivo x", "Rodou npm test") | "Detalhes tecnicos (N eventos)" com o JSON |
| Aprovacoes | "DOT X quer escrever o arquivo y" com Permitir/Negar; respondidos abaixo | "Detalhes tecnicos": ferramenta, recursos, parametros |
| Motores de IA | estado em uma palavra (Pronto, Precisa entrar, Nao instalado, Falta integrar) com a instrucao de uma linha | "Detalhes tecnicos": caminho, login, capacidades, caminho manual |
| Configuracoes | servico ativo/parado com Ligar/Parar; iniciar com o Mac; copias de seguranca | "Diagnostico": versao, pid, socket, launchd, pastas, reiniciar |
| Gatilhos do DOT | "Quando X acontecer, pedir Y" e "Em um horario marcado" em frases | — |
| Equipe do DOT | a quem ele pode pedir trabalho (ninguem, qualquer um, alguns) | "Avancado": profundidade de cadeia |
| Operacoes (em Atividades e Visao geral) | trabalhando, na fila, esperando voce, falhas, assistentes em execucao, consumo | "Registro do servico" |

Alem disso, qualquer pedido de permissao pendente aparece numa faixa amarela no topo de todas as telas, com
Permitir/Negar, e como contador no item Aprovacoes do menu.

## Vocabulario

| Evitado na interface | Usado |
|---|---|
| motor, CLI | assistente de IA |
| workspace | pasta de trabalho / pastas que ele pode usar |
| politica, nivel de auto-aprovacao | quanto ele pode fazer sozinho |
| aprovacao (acao) | pedido de permissao; Permitir / Negar |
| execucao, evento, seq | o que aconteceu |
| runtime, launchd, socket, pid | servico em segundo plano |
| tarefa FAILED | nao deu certo (com a causa em frase: login, tempo, cancelado) |
| evento, subscription | gatilho ("quando acontecer X") |
| delegacao | pedido a outro DOT |
| checkpoint | ponto de controle |

O menu principal mantem os sete itens da especificacao.

## Onde fica a logica

`apps/desktop/src/lib/humanize.ts` traduz eventos, aprovacoes, falhas e estados em frases (testado em `humanize.test.ts`);
`apps/desktop/src/lib/templates.ts` guarda os modelos de DOT e os niveis de confianca. O runtime nao mudou.

## Elementos trazidos do ovrgit (2026-10-15)

- **Batimento cardiaco na lista de DOTs** (`components/Heartbeat.vue`, `lib/heartbeat.ts`, copiados do ovrgit): o tracado de
  eletrocardiograma ao lado de cada contato diz o estado antes de ler o texto. O runtime calcula `Contact.pulse` em
  `contacts.list`: `live` (trabalhando, firme), `waiting` (espera voce ou o gerente, lento e laranja), `standby` (na fila,
  linha reta apagada), `error` (o ultimo trabalho falhou, linha reta vermelha), `idle` (nada: o tracado nao aparece).
- **Uso das assinaturas** (`components/UsageChip.vue`, `stores/usage.ts`): o anel com a media semanal fica na barra de baixo
  da lista de contatos; o painel mostra Claude (Max/Pro), ChatGPT (Codex) e Antigravity com as janelas de semana e 5 horas,
  a versao do CLI, **Atualizar** (roda o `update` do proprio CLI) e **Instalar** (instalador oficial) para quem nao esta na
  maquina. O mesmo aparece em Configuracoes > Assistentes de IA, dentro de cada cartao. Fonte dos dados: `dotsd`
  (`usage.rs`, `cli_updates.rs`), copiados do ovrgit: endpoint de uso do Claude com o login do Claude Code, registros de
  sessao do Codex, token do Antigravity.
- **Temas (skins)** (`lib/themes.ts`, `lib/theme.ts`, Configuracoes > Geral > Aparencia): os temas do ovrgit (Dracula, One
  Dark Pro, Tokyo Night, Monokai, Nord, GitHub Dark/Light, Dust2, Mirage, Terminal). Cada tema sobrescreve as variaveis do
  ovrgit (`--accent`, `--bg`, `--panel`...) e a escala `zinc`/`indigo`/`amber`/`emerald`/`red`/`sky` do Tailwind v4
  (`--color-zinc-900` etc.), entao a interface inteira muda sem tocar nos componentes. A escolha fica em `ui.theme`
  (configuracao do servico) e num espelho local para pintar antes de o servico responder.
- **Demissao pelo gerente** (`dots_dismiss_employee`, `crates/dots-runtime/src/dismissal.rs`): o gerente demite um
  funcionario da propria equipe por um motivo (sempre pede permissao ao usuario, como toda acao irreversivel). A memoria e a
  conversa do demitido zeram (a conversa nova nasce limpa) e ele volta como funcionario novo na mesma equipe; motivo e licao ficam na tabela
  `dismissals` e entram nas instrucoes de todos os funcionarios da equipe (e do gerente) para ninguem repetir o erro.
  Perfil do DOT > Equipe > Demissoes e a pagina Equipes mostram o historico.
- **Pausa pelo gerente** (`dots_pause_employee` / `dots_resume_employee`, `crates/dots-runtime/src/staffing.rs`): o gerente
  pausa e retoma um funcionario da propria equipe sem aprovacao do usuario (reversivel). Pausado, o funcionario nao recebe
  trabalho novo e o que estava na fila espera. Pelo usuario, pausar o gerente pausa os funcionarios ativos e retomar o
  gerente retoma os pausados (menu de contexto do botao direito na lista e na Visao geral, ou o perfil do DOT).
