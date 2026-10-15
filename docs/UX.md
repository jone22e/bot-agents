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
- **Perfil do DOT** abre ao clicar no nome no topo: abas Sobre, Permissoes, Gatilhos, Equipe, Memoria; pausar, duplicar,
  arquivar, excluir.
- **Entrada**: clipe para anexar, seta para enviar, quadrado para parar. Sem microfone (nao ha transcricao no projeto).
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
  "Perguntou ao gerente", sem pedir atencao do usuario. A tela Equipes (menu
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
