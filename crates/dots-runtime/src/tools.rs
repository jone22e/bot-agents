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
        json!({ "name": "dots_list_team", "description": "Lista os DOTs para os quais este DOT pode delegar trabalho, com descricao e disponibilidade.", "inputSchema": obj(json!({}), vec![]) }),
        json!({ "name": "dots_delegate", "description": "Delega um pedido a outro DOT autorizado. Nao bloqueia: o resultado chega em uma nova tarefa de continuacao quando o outro DOT terminar.", "inputSchema": obj(json!({ "to": { "type": "string", "description": "Nome ou id do DOT" }, "request": { "type": "string" }, "context": { "type": "string", "description": "Contexto util para o outro DOT" } }), vec!["to", "request"]) }),
        json!({ "name": "dots_delegation_status", "description": "Estado das delegacoes desta tarefa (ou de uma delegacao especifica).", "inputSchema": obj(json!({ "id": { "type": "string" } }), vec![]) }),
        json!({ "name": "dots_cancel_delegation", "description": "Cancela uma delegacao feita por esta tarefa.", "inputSchema": obj(json!({ "id": { "type": "string" } }), vec!["id"]) }),
        json!({ "name": "dots_browser_open", "description": "Abre uma URL (http/https) no navegador proprio deste DOT (perfil isolado, logins persistem entre tarefas) e devolve a pagina em texto: titulo, URL final e arvore de acessibilidade com [ref=N] por elemento. Exige rede liberada na politica do DOT.", "inputSchema": obj(json!({ "url": { "type": "string", "description": "URL completa, http ou https" } }), vec!["url"]) }),
        json!({ "name": "dots_ask_user", "description": "Faz uma pergunta (com opcoes opcionais). Em um pedido delegado por outro DOT, a pergunta vai para ele (seu gerente); fora disso, vai para o usuario. Nao bloqueia: a resposta chega em uma nova tarefa de continuacao.", "inputSchema": obj(json!({ "question": { "type": "string" }, "options": { "type": "array", "items": { "type": "string" } } }), vec!["question"]) }),
        json!({ "name": "dots_answer_question", "description": "Responde a pergunta de um funcionario em um pedido que voce delegou (o id da pergunta vem na tarefa). Ele continua o trabalho com a resposta.", "inputSchema": obj(json!({ "question_id": { "type": "string" }, "answer": { "type": "string" } }), vec!["question_id", "answer"]) }),
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
