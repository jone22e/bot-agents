export interface DotTemplate { id: string; name: string; purpose: string; trust: "read" | "write" | "exec" }

export const DOT_TEMPLATES: DotTemplate[] = [
  { id: "blank", name: "", purpose: "", trust: "read" },
  { id: "reviewer", name: "Revisor de codigo", trust: "read", purpose: "Revisa codigo com rigor. Le os arquivos da pasta de trabalho, aponta bugs, riscos de seguranca e melhorias, e explica cada ponto com o caminho do arquivo e a linha. Nunca altera arquivos; apenas recomenda." },
  { id: "writer", name: "Redator de documentacao", trust: "write", purpose: "Escreve e mantem documentacao clara em portugues: READMEs, guias de instalacao e notas de versao. Le o codigo para entender o que documentar e cria ou atualiza arquivos .md na pasta de trabalho." },
  { id: "qa", name: "QA", trust: "exec", purpose: "Testa o projeto. Roda a suite de testes com os comandos permitidos, interpreta as falhas, escreve novos testes quando faltam e entrega um relatorio objetivo do que passou e do que quebrou." },
  { id: "assistant", name: "Assistente geral", trust: "read", purpose: "Ajuda com perguntas e tarefas do dia a dia sobre os arquivos da pasta de trabalho: resume, explica, organiza e responde de forma direta." },
];

export const TRUST_OPTIONS: { value: "read" | "write" | "exec"; title: string; description: string }[] = [
  { value: "read", title: "So le e pergunta antes de mudar qualquer coisa", description: "Pode ler arquivos. Qualquer alteracao ou comando pede sua permissao." },
  { value: "write", title: "Pode editar arquivos nas pastas dele", description: "Cria e altera arquivos sem perguntar. Comandos e acoes irreversiveis pedem permissao." },
  { value: "exec", title: "Pode editar e rodar os comandos que eu liberar", description: "Alem de editar, roda os comandos da lista permitida sem perguntar. Acoes irreversiveis sempre pedem permissao." },
];
