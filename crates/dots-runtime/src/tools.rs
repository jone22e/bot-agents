//! Implementacao das ferramentas do gateway. Todas recebem caminhos ja autorizados.

use std::path::{Path, PathBuf};
use std::time::Duration;

use dots_core::{DotsError, DotsResult};
use serde_json::{Value, json};

pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
    /// Alteracoes relevantes no workspace (path, kind, summary).
    pub changes: Vec<(String, String, String)>,
}

impl ToolOutput {
    pub fn ok(text: String) -> Self {
        Self { text, is_error: false, changes: vec![] }
    }
    pub fn err(text: String) -> Self {
        Self { text, is_error: true, changes: vec![] }
    }
}

const MAX_OUTPUT: usize = 64 * 1024;

pub(crate) fn cap(s: String) -> String {
    if s.len() > MAX_OUTPUT {
        let mut t: String = s.chars().take(MAX_OUTPUT).collect();
        t.push_str("\n…[saida truncada]");
        t
    } else {
        s
    }
}

/// Definicoes MCP das ferramentas (nome, descricao, schema de entrada).
pub fn tool_definitions() -> Vec<Value> {
    let obj = |props: Value, required: Vec<&str>| json!({ "type": "object", "properties": props, "required": required });
    vec![
        json!({ "name": "dots_list_dir", "description": "Lista arquivos e pastas de um diretorio autorizado do workspace.", "inputSchema": obj(json!({ "path": { "type": "string", "description": "Caminho (relativo ao workspace ou absoluto autorizado)" }, "depth": { "type": "integer", "minimum": 1, "maximum": 4 } }), vec!["path"]) }),
        json!({ "name": "dots_read_file", "description": "Le um arquivo de texto autorizado (com offset/limit em linhas).", "inputSchema": obj(json!({ "path": { "type": "string" }, "offset": { "type": "integer", "minimum": 0 }, "limit": { "type": "integer", "minimum": 1, "maximum": 5000 } }), vec!["path"]) }),
        json!({ "name": "dots_search", "description": "Busca um padrao (regex basico) em arquivos de um diretorio autorizado.", "inputSchema": obj(json!({ "path": { "type": "string" }, "pattern": { "type": "string" }, "glob": { "type": "string", "description": "Ex.: *.ts" } }), vec!["path", "pattern"]) }),
        json!({ "name": "dots_write_file", "description": "Cria ou sobrescreve um arquivo de texto em diretorio autorizado com escrita. Pode exigir aprovacao do usuario.", "inputSchema": obj(json!({ "path": { "type": "string" }, "content": { "type": "string" } }), vec!["path", "content"]) }),
        json!({ "name": "dots_delete_file", "description": "Exclui um arquivo. Sempre exige aprovacao do usuario.", "inputSchema": obj(json!({ "path": { "type": "string" } }), vec!["path"]) }),
        json!({ "name": "dots_run_command", "description": "Executa um comando de terminal permitido pela politica do DOT (sem TTY, com timeout). Comandos fora da lista sao negados.", "inputSchema": obj(json!({ "command": { "type": "string" }, "cwd": { "type": "string" }, "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 600 } }), vec!["command"]) }),
        json!({ "name": "dots_git", "description": "Operacoes git controladas: status, diff, log, show, branch_list, add, commit, checkout, branch_create, stash, restore, push, reset, clean. Operacoes destrutivas exigem aprovacao.", "inputSchema": obj(json!({ "op": { "type": "string" }, "args": { "type": "array", "items": { "type": "string" } }, "cwd": { "type": "string" } }), vec!["op"]) }),
        json!({ "name": "dots_system_info", "description": "Informacoes do sistema e dos motores disponiveis.", "inputSchema": obj(json!({}), vec![]) }),
        json!({ "name": "dots_remember", "description": "Grava uma memoria de longo prazo do DOT (fato util para tarefas futuras).", "inputSchema": obj(json!({ "content": { "type": "string" }, "importance": { "type": "integer", "minimum": 0, "maximum": 10 } }), vec!["content"]) }),
        json!({ "name": "dots_recall", "description": "Busca memorias do DOT relevantes para uma consulta.", "inputSchema": obj(json!({ "query": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 50 } }), vec!["query"]) }),
        json!({ "name": "dots_checkpoint", "description": "Grava um ponto de controle do trabalho (etapa concluida, estado, proxima acao, arquivos produzidos) para retomada segura se a execucao for interrompida.", "inputSchema": obj(json!({ "step": { "type": "string" }, "state": { "type": "object" }, "results": { "type": "string" }, "next_action": { "type": "string" }, "artifacts": { "type": "array", "items": { "type": "string" } } }), vec!["step"]) }),
        json!({ "name": "dots_schedule_create", "description": "Agenda um pedido para o proprio DOT: no horario, o runtime cria uma tarefa com este pedido e voce trabalha nela. Use para rodadas periodicas, relatorios por hora e qualquer coisa prometida para depois. kind: every (spec = minutos, ex.: \"60\"), daily (spec = \"HH:MM\"), weekly (spec = \"D HH:MM\", D de 0=domingo a 6=sabado) ou cron (5 campos). Horario local do Mac do usuario. O pedido deve ser completo, pois chega sem o contexto desta conversa.", "inputSchema": obj(json!({ "kind": { "type": "string", "enum": ["every", "daily", "weekly", "cron"] }, "spec": { "type": "string" }, "prompt": { "type": "string", "description": "O que fazer em cada disparo (instrucoes completas)" } }), vec!["kind", "spec", "prompt"]) }),
        json!({ "name": "dots_schedule_list", "description": "Lista os agendamentos do proprio DOT (id, quando, proximo disparo, pedido).", "inputSchema": obj(json!({}), vec![]) }),
        json!({ "name": "dots_schedule_cancel", "description": "Cancela (apaga) um agendamento do proprio DOT pelo id (veja dots_schedule_list).", "inputSchema": obj(json!({ "id": { "type": "string" } }), vec!["id"]) }),
        json!({ "name": "dots_list_team", "description": "Lista os DOTs para os quais este DOT pode delegar trabalho, com descricao e disponibilidade.", "inputSchema": obj(json!({}), vec![]) }),
        json!({ "name": "dots_delegate", "description": "Delega um pedido a outro DOT autorizado. Nao bloqueia: o resultado chega em uma nova tarefa de continuacao quando o outro DOT terminar.", "inputSchema": obj(json!({ "to": { "type": "string", "description": "Nome ou id do DOT" }, "request": { "type": "string" }, "context": { "type": "string", "description": "Contexto util para o outro DOT" } }), vec!["to", "request"]) }),
        json!({ "name": "dots_delegation_status", "description": "Estado das delegacoes desta tarefa (ou de uma delegacao especifica), incluindo se o funcionario esta na fila, trabalhando, parado ou aguardando resposta.", "inputSchema": obj(json!({ "id": { "type": "string" } }), vec![]) }),
        json!({ "name": "dots_resume_delegation", "description": "Recoloca na fila a tarefa de um funcionario que parou (interrompida, com falha ou cancelada) em uma delegacao sua.", "inputSchema": obj(json!({ "id": { "type": "string", "description": "id da delegacao" } }), vec!["id"]) }),
        json!({ "name": "dots_cancel_delegation", "description": "Cancela uma delegacao feita por esta tarefa.", "inputSchema": obj(json!({ "id": { "type": "string" } }), vec!["id"]) }),
        json!({ "name": "dots_hire_employee", "description": "Contrata um funcionario novo para a SUA equipe (so o gerente usa). Voce cria o nome (unico, de pessoa) e define a funcao. Use sempre que nao houver funcionario para a tarefa: o gerente nunca executa o trabalho, mesmo sem funcionarios. Ele nasce com o seu assistente, suas pastas, seu app conectado e suas permissoes (nunca mais que voce), sem memoria. Respeita o limite de funcionarios por gerente definido pelo usuario.", "inputSchema": obj(json!({ "name": { "type": "string", "description": "Nome do funcionario (voce escolhe; nao pode repetir o de outro DOT)" }, "role": { "type": "string", "description": "Funcao curta, ex.: Revisor de anuncios" }, "instructions": { "type": "string", "description": "Instrucoes permanentes: o que ele faz, como e com quais cuidados (opcional; sem isso, vale a funcao)" } }), vec!["name", "role"]) }),
        json!({ "name": "dots_dismiss_employee", "description": "Demite um funcionario da SUA equipe (so o gerente usa) por um motivo: erro grave ou repetido, mesmo depois de instrucoes melhores. Sempre exige aprovacao do usuario. O funcionario e EXCLUIDO (memoria, conversa e trabalhos dele); depois contrate um substituto com dots_hire_employee. Motivo e licao ficam registrados e entram nas instrucoes de toda a equipe para ninguem repetir o erro.", "inputSchema": obj(json!({ "dot": { "type": "string", "description": "Nome ou id do funcionario" }, "reason": { "type": "string", "description": "O que ele fez de errado" }, "lesson": { "type": "string", "description": "O que o proximo deve fazer diferente (opcional; sem isso, vale o motivo)" } }), vec!["dot", "reason"]) }),
        json!({ "name": "dots_pause_employee", "description": "Pausa um funcionario da SUA equipe (so o gerente usa): ele para de receber trabalho novo e o que estava na fila espera. Reversivel, sem aprovacao do usuario. Use quando quiser segurar alguem (ex.: ate corrigir instrucoes ou enquanto outro termina); para erro grave, use dots_dismiss_employee.", "inputSchema": obj(json!({ "dot": { "type": "string", "description": "Nome ou id do funcionario" }, "reason": { "type": "string", "description": "Por que esta pausando (opcional; fica no historico)" } }), vec!["dot"]) }),
        json!({ "name": "dots_resume_employee", "description": "Retoma um funcionario pausado da SUA equipe (so o gerente usa): ele volta a receber trabalho e o que estava na fila dele segue.", "inputSchema": obj(json!({ "dot": { "type": "string", "description": "Nome ou id do funcionario" } }), vec!["dot"]) }),
        json!({ "name": "dots_browser_open", "description": "Abre uma URL (http/https) no navegador proprio deste DOT (perfil isolado, logins persistem entre tarefas) e devolve a pagina em texto: titulo, URL final e arvore de acessibilidade com [ref=N] por elemento. Use diretamente: apps conectados ja estao autorizados; se a internet estiver desabilitada, o runtime pede permissao ao usuario para o dominio externo antes de abrir.", "inputSchema": obj(json!({ "url": { "type": "string", "description": "URL completa, http ou https" } }), vec!["url"]) }),
        json!({ "name": "dots_browser_click", "description": "Clica, como um mouse, no elemento [ref=N] da ultima leitura da pagina do navegador do DOT e devolve a pagina depois do clique. Clicar em algo que grava (Salvar, Confirmar...) ou irreversivel (Excluir, Publicar...) pode aguardar aprovacao; o runtime pede sozinho, nao pergunte antes.", "inputSchema": obj(json!({ "ref": { "type": "integer", "description": "numero do [ref=N] na leitura da pagina" } }), vec!["ref"]) }),
        json!({ "name": "dots_browser_type", "description": "Digita texto no campo [ref=N] da pagina do navegador do DOT (foca o campo antes) e devolve a pagina. clear (padrao true) apaga o que havia; submit=true tecla Enter no fim.", "inputSchema": obj(json!({ "ref": { "type": "integer" }, "text": { "type": "string" }, "clear": { "type": "boolean" }, "submit": { "type": "boolean" } }), vec!["ref", "text"]) }),
        json!({ "name": "dots_browser_press", "description": "Tecla na pagina do navegador do DOT: Enter, Escape, Tab, Backspace, Delete, Space, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, PageUp, PageDown, Home, End. Com um dialogo do navegador aberto (alert/confirm), Enter aceita e Escape cancela.", "inputSchema": obj(json!({ "key": { "type": "string" } }), vec!["key"]) }),
        json!({ "name": "dots_browser_select", "description": "Escolhe uma opcao (pelo valor ou pelo texto) na lista de opcoes <select> [ref=N] da pagina do navegador do DOT.", "inputSchema": obj(json!({ "ref": { "type": "integer" }, "option": { "type": "string" } }), vec!["ref", "option"]) }),
        json!({ "name": "dots_browser_read", "description": "Le de novo a pagina atual do navegador do DOT, sem navegar (conteudo que carregou depois, refs atualizados).", "inputSchema": obj(json!({}), vec![]) }),
        json!({ "name": "dots_ask_user", "description": "Faz uma pergunta. Quando houver alternativas claras, inclua 2 a 4 respostas objetivas em options para o usuario clicar; perguntas abertas aceitam resposta em texto. Em um pedido delegado por outro DOT, a pergunta vai para ele (seu gerente); fora disso, vai para o usuario. Nao bloqueia: a resposta chega em uma nova tarefa de continuacao.", "inputSchema": obj(json!({ "question": { "type": "string" }, "options": { "type": "array", "items": { "type": "string" } } }), vec!["question"]) }),
        json!({ "name": "dots_notify_user", "description": "Avisa o usuario de algo que precisa da atencao dele (erro que te impede de continuar, pendencia que so ele resolve, resultado importante). So o gerente usa (ou um DOT sem equipe): funcionario relata ao seu gerente no resultado da tarefa. Aparece no topo da tela do DOTS e fica la ate ele clicar; nao e uma pergunta e nao bloqueia: continue ou encerre o turno normalmente. Use com moderacao (uma por assunto; nao repita o mesmo aviso) e so quando a atencao do usuario for mesmo necessaria. Para decisoes, use dots_ask_user.", "inputSchema": obj(json!({ "title": { "type": "string", "description": "Frase curta do que aconteceu (ate 80 caracteres), ex.: \"3 notas nao transmitidas\"" }, "detail": { "type": "string", "description": "Complemento opcional, ex.: \"R$ 14.820,00 · aguardando decisao\"" }, "action": { "type": "string", "description": "Rotulo do botao que abre a conversa com voce (opcional, ate 20 caracteres), ex.: \"Ver notas\"" } }), vec!["title"]) }),
        json!({ "name": "dots_answer_question", "description": "Responde a pergunta de um funcionario em um pedido que voce delegou (o id da pergunta vem na tarefa). Ele continua o trabalho com a resposta.", "inputSchema": obj(json!({ "question_id": { "type": "string" }, "answer": { "type": "string" } }), vec!["question_id", "answer"]) }),
        json!({ "name": "dots_decide_approval", "description": "Decide o pedido de permissao de um funcionario em um pedido que voce delegou (o id vem na tarefa). approved=true libera a acao; false nega.", "inputSchema": obj(json!({ "approval_id": { "type": "string" }, "approved": { "type": "boolean" }, "reason": { "type": "string" } }), vec!["approval_id", "approved"]) }),
        json!({ "name": "permission_prompt", "description": "Uso interno do DOTS: decide permissoes de ferramentas nativas do CLI.", "inputSchema": obj(json!({ "tool_name": { "type": "string" }, "input": { "type": "object" }, "tool_use_id": { "type": "string" } }), vec!["tool_name", "input"]) }),
    ]
}

pub fn list_dir(path: &Path, depth: usize) -> DotsResult<ToolOutput> {
    fn walk(p: &Path, depth: usize, prefix: &str, out: &mut Vec<String>, count: &mut usize) {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if *count >= 2000 {
                out.push("…[lista truncada]".into());
                return;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if name == ".git" || name == "node_modules" || name == "target" || name == ".dots" {
                out.push(format!("{prefix}{name}/ (omitido)"));
                continue;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(if is_dir { format!("{prefix}{name}/") } else { format!("{prefix}{name} ({size} B)") });
            *count += 1;
            if is_dir && depth > 1 {
                walk(&e.path(), depth - 1, &format!("{prefix}  "), out, count);
            }
        }
    }
    if !path.is_dir() {
        return Ok(ToolOutput::err(format!("nao e um diretorio: {}", path.display())));
    }
    let mut out = vec![];
    let mut count = 0;
    walk(path, depth.clamp(1, 4), "", &mut out, &mut count);
    Ok(ToolOutput::ok(cap(out.join("\n"))))
}

pub fn read_file(path: &Path, offset: usize, limit: usize, max_bytes: u64) -> DotsResult<ToolOutput> {
    let meta = std::fs::metadata(path).map_err(|e| DotsError::Io(format!("{}: {e}", path.display())))?;
    if !meta.is_file() {
        return Ok(ToolOutput::err(format!("nao e um arquivo: {}", path.display())));
    }
    if meta.len() > max_bytes {
        return Ok(ToolOutput::err(format!("arquivo excede {max_bytes} bytes; use offset/limit em um arquivo menor ou peca ao usuario")));
    }
    let bytes = std::fs::read(path)?;
    let text = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => return Ok(ToolOutput::err("arquivo binario; nao e possivel ler como texto".into())),
    };
    let lines: Vec<&str> = text.lines().collect();
    let end = (offset + limit.max(1)).min(lines.len());
    let slice = if offset >= lines.len() { vec![] } else { lines[offset..end].to_vec() };
    let numbered: Vec<String> = slice.iter().enumerate().map(|(i, l)| format!("{:>5}\t{}", offset + i + 1, l)).collect();
    Ok(ToolOutput::ok(cap(format!("[{} linhas no total]\n{}", lines.len(), numbered.join("\n")))))
}

pub fn write_file(path: &Path, content: &str) -> DotsResult<ToolOutput> {
    let existed = path.exists();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("{}.dots-tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("")));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    let mut out = ToolOutput::ok(format!("{} {} ({} bytes)", if existed { "atualizado" } else { "criado" }, path.display(), content.len()));
    out.changes.push((path.to_string_lossy().into(), if existed { "modify".into() } else { "create".into() }, format!("{} bytes", content.len())));
    Ok(out)
}

pub fn delete_file(path: &Path) -> DotsResult<ToolOutput> {
    if !path.is_file() {
        return Ok(ToolOutput::err(format!("nao e um arquivo: {}", path.display())));
    }
    std::fs::remove_file(path)?;
    let mut out = ToolOutput::ok(format!("excluido {}", path.display()));
    out.changes.push((path.to_string_lossy().into(), "delete".into(), String::new()));
    Ok(out)
}

pub async fn search(path: &Path, pattern: &str, glob: Option<&str>, path_dirs: &[PathBuf], home: &Path) -> DotsResult<ToolOutput> {
    let mut args = vec!["-rnI".to_string(), "--exclude-dir=.git".into(), "--exclude-dir=node_modules".into(), "--exclude-dir=target".into(), "--exclude-dir=.dots".into(), "-m".into(), "200".into()];
    if let Some(g) = glob {
        args.push(format!("--include={g}"));
    }
    args.push("-e".into());
    args.push(pattern.to_string());
    args.push(path.to_string_lossy().to_string());
    let argv: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = dots_engines::run_command(Path::new("/usr/bin/grep"), &argv, home, path_dirs, Duration::from_secs(30)).await?;
    match out.exit_code {
        Some(0) => Ok(ToolOutput::ok(cap(out.stdout))),
        Some(1) => Ok(ToolOutput::ok("nenhuma ocorrencia".into())),
        _ => Ok(ToolOutput::err(cap(out.stderr))),
    }
}

pub async fn run_command(command: &str, cwd: &Path, timeout: Duration, home: &Path, path_dirs: &[PathBuf]) -> DotsResult<ToolOutput> {
    let out = dots_engines::run_command(Path::new("/bin/sh"), &["-c", command], home, path_dirs, timeout).await;
    match out {
        Ok(o) => {
            let mut text = String::new();
            if !o.stdout.is_empty() {
                text.push_str(&o.stdout);
            }
            if !o.stderr.is_empty() {
                text.push_str("\n[stderr]\n");
                text.push_str(&o.stderr);
            }
            text.push_str(&format!("\n[exit {:?}]", o.exit_code));
            let mut r = if o.exit_code == Some(0) { ToolOutput::ok(cap(text)) } else { ToolOutput::err(cap(text)) };
            r.changes.push((cwd.to_string_lossy().into(), "command".into(), command.chars().take(200).collect()));
            Ok(r)
        }
        Err(e) => Ok(ToolOutput::err(e.message().to_string())),
    }
}

pub async fn git(op: &str, args: &[String], cwd: &Path, home: &Path, path_dirs: &[PathBuf]) -> DotsResult<ToolOutput> {
    let mut argv: Vec<String> = match op {
        "status" => vec!["status".into(), "--short".into(), "--branch".into()],
        "diff" => vec!["diff".into()],
        "log" => vec!["log".into(), "--oneline".into(), "-n".into(), "50".into()],
        "show" => vec!["show".into(), "--stat".into()],
        "branch_list" => vec!["branch".into(), "-a".into()],
        "remote" => vec!["remote".into(), "-v".into()],
        "add" => vec!["add".into()],
        "commit" => vec!["commit".into()],
        "checkout" => vec!["checkout".into()],
        "branch_create" => vec!["checkout".into(), "-b".into()],
        "stash" => vec!["stash".into()],
        "restore" => vec!["restore".into()],
        "push" => vec!["push".into()],
        "reset" => vec!["reset".into()],
        "clean" => vec!["clean".into()],
        "rebase" => vec!["rebase".into()],
        "merge" => vec!["merge".into()],
        "tag" => vec!["tag".into()],
        other => return Ok(ToolOutput::err(format!("operacao git desconhecida: {other}"))),
    };
    argv.extend(args.iter().cloned());
    let mut full = vec!["-C".to_string(), cwd.to_string_lossy().to_string(), "--no-pager".into()];
    full.extend(argv);
    let refs: Vec<&str> = full.iter().map(|s| s.as_str()).collect();
    let out = dots_engines::run_command(Path::new("/usr/bin/git"), &refs, home, path_dirs, Duration::from_secs(120)).await?;
    let text = format!("{}{}", out.stdout, if out.stderr.is_empty() { String::new() } else { format!("\n[stderr]\n{}", out.stderr) });
    let mut r = if out.exit_code == Some(0) { ToolOutput::ok(cap(text)) } else { ToolOutput::err(cap(text)) };
    if matches!(op, "add" | "commit" | "checkout" | "branch_create" | "stash" | "restore" | "push" | "reset" | "clean" | "rebase" | "merge" | "tag") {
        r.changes.push((cwd.to_string_lossy().into(), "git".into(), format!("git {op} {}", args.join(" "))));
    }
    Ok(r)
}

pub fn system_info(version: &str, engines: &[(String, Option<String>)]) -> ToolOutput {
    let info = json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "dotsd_version": version,
        "engines": engines.iter().map(|(k, v)| json!({ "engine": k, "version": v })).collect::<Vec<_>>(),
        "now": dots_core::now().to_rfc3339(),
    });
    ToolOutput::ok(serde_json::to_string_pretty(&info).unwrap_or_default())
}
