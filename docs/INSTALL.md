# Instalacao do DOTS (macOS)

## Requisitos

- macOS 12 ou superior, Apple Silicon (arm64). Build Intel ainda nao validado.
- Para usar motores de IA: Codex CLI, Claude Code e/ou AGY instalados e autenticados
  no terminal. O DOTS nunca faz login por voce.

## Instalar

1. Abra o arquivo `DOTS_<versao>_aarch64.dmg` e arraste **DOTS** para **Aplicativos**.
2. Na primeira abertura, o macOS pode bloquear por falta de assinatura/notarizacao.
   Clique com o botao direito em **DOTS** > **Abrir** > **Abrir** (ou em
   Ajustes do Sistema > Privacidade e Seguranca > "Abrir mesmo assim").
3. O assistente de primeira execucao confere o runtime e detecta os motores.

## O que a instalacao cria

| Local | Conteudo |
|---|---|
| `~/Library/Application Support/DOTS/dots.db` | Banco SQLite (WAL) com todos os dados |
| `~/Library/Application Support/DOTS/backups/` | Backups automaticos (antes de migracoes) e manuais |
| `~/Library/Application Support/DOTS/logs/` | Logs do runtime (`dotsd.log.*`) |
| `~/Library/Application Support/DOTS/workspaces/<id>` | Workspace padrao de cada DOT |
| `~/Library/Application Support/DOTS/bin/dotsd-<versao>` | Copia estavel do runtime usada pelo launchd |
| `~/Library/Application Support/DOTS/run/` | Socket IPC e token (permissao 0700/0600) |
| `~/Library/LaunchAgents/com.c2s.dots.runtime.plist` | LaunchAgent que mantem o runtime ativo |

## Runtime em segundo plano

O runtime `dotsd` continua ativo quando a janela e fechada e e reiniciado pelo
launchd se falhar. Em **Configuracoes > Runtime** voce ve PID, versao, modo e pode
**Parar** (descarrega o LaunchAgent) ou **Reiniciar**.

Verificar manualmente:

```bash
launchctl print gui/$(id -u)/com.c2s.dots.runtime | head
pgrep -fl dotsd
```

## Usar os DOTs (Fase 2)

1. Em **Meus DOTs**, crie um DOT com motor principal e, na aba **Permissoes**, defina o nivel de aprovacao automatica,
   comandos permitidos e diretorios externos autorizados.
2. Em **Conversas**, envie mensagens; em **Atividades**, enfileire tarefas e acompanhe eventos, ferramentas e alteracoes.
3. Em **Aprovacoes**, decida as acoes que excedem o nivel automatico (validade de 10 minutos).
4. Para o **AGY**, ative a integracao em **Motores de IA** (registra o servidor MCP `dots` e regras na configuracao global do AGY;
   reversivel pelo botao "Remover integracao").

## Automacao e equipe (Fase 3)

1. Na aba **Gatilhos** do DOT, crie regras "Quando acontecer X, pedir Y" (arquivo mudar, commit, outro DOT concluir) e
   horarios (diario, semanal, a cada N minutos, cron). O DOT so trabalha nesses momentos ou quando voce pede.
2. Marque um DOT como **Supervisor** e, na aba **Equipe**, diga a quem ele pode pedir trabalho. Ele delega, acompanha
   e consolida; perguntas dele aparecem na faixa amarela com opcoes.
3. Em **Configuracoes**, ative "Iniciar o servico quando eu entrar no Mac" para gatilhos e horarios funcionarem sem abrir o app.

## Flexi (Fase Flexi)

1. Em **Conectar aplicativos**, no cartao Flexi, informe o e-mail e a senha do usuario do Flexi que os DOTs vao usar
   (a empresa e a base vem da conta, como no login do Flexi). Marque "guardar a senha no Keychain" se quiser que o DOTS reconecte sozinho quando a sessao expirar.
   Se a conta tiver 2FA, digite o codigo no cartao.
2. Toque em **Montar equipe de marketplaces** para criar os 11 DOTs prontos, ou crie um DOT novo e escolha um papel de
   marketplaces, ou do zero. No passo 3 voce diz se ele e Gerente (e de qual equipe: uma sem gerente ou uma nova) ou
   Funcionario (de qual equipe, com o gerente mostrado). No ultimo passo, com o Flexi conectado, escolhe se so consulta
   ou pode editar no Flexi. Em **Equipes** (menu do avatar) voce monta e ajusta as equipes; na lista de contatos os DOTs
   aparecem agrupados por equipe. Na aba **Aplicativos** do perfil voce escolhe qual conexao o DOT usa.
3. Leituras acontecem direto; alteracoes de preco, conteudo, estoque e campanhas pedem permissao no nivel padrao; criar,
   publicar, finalizar e copiar anuncios sempre pedem permissao. No Flexi tudo fica no nome do usuario conectado.

## Atualizar

Substitua o app em Aplicativos. Na proxima abertura, o DOTS copia o runtime novo para
`bin/`, atualiza o LaunchAgent e, se o esquema do banco mudou, grava um backup em
`backups/` antes de migrar. Dados existentes nunca sao sobrescritos.

## Desinstalar

1. Em Configuracoes > Runtime, clique em **Parar**.
2. Remova `~/Library/LaunchAgents/com.c2s.dots.runtime.plist` e o app.
3. Opcional: remova `~/Library/Application Support/DOTS` (apaga todos os dados).
