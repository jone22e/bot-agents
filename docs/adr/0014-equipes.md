# ADR 0014: equipes como entidade, delegacao derivada

Data: 2026-10-15. Estado: aceito (pedido do usuario).

## Contexto
A hierarquia por areas fixas (migracoes 0006/0007) nao deixava o usuario montar a propria estrutura. O usuario pediu
"grupos/equipes" criados por ele, com Gerente/Funcionario escolhidos ao criar o DOT.

## Decisao
Tabelas `teams` (nome, gerente unico) e `team_members` (um DOT e funcionario de no maximo uma equipe), migracao 0008.
Equipes se encaixam: o gerente de uma pode ser funcionario de outra. O runtime mantem derivado: `can_delegate_to` do
gerente = funcionarios da equipe, `max_delegation_depth` 3, `role` do gerente = supervisor (volta a worker ao deixar de
gerenciar). Ciclos sao recusados na gravacao. A aba Equipe do perfil e a tela Equipes editam a composicao; a politica de
delegacao deixa de ser editada a mao. As colunas `role_template` e `work_group` ficam como referencia do papel de origem.

## Consequencias
- A lista de contatos agrupa os DOTs por equipe (gerente primeiro). "Montar equipe de marketplaces" cria quatro equipes
  encaixadas (Marketplaces > Mercado Livre, Shopee, Google Ads; Agente de Publicacao em Marketplaces).
- Quem editava `can_delegate_to` pela aba Equipe passa a fazer isso adicionando funcionarios.
