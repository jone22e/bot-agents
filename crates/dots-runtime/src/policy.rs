//! Autorizacao de ferramentas: avaliada pelo runtime, fora da LLM.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use dots_core::{DotsError, DotsResult, ExecutionEvent, RiskLevel, ToolPolicy};
use serde_json::Value;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct AuthorizedDir {
    pub path: PathBuf,
    pub can_read: bool,
    pub can_write: bool,
}

/// Contexto de uma execucao em andamento, visivel ao gateway pelo token.
pub struct ExecutionContext {
    pub token: String,
    pub execution_id: String,
    pub task_id: String,
    pub dot_id: String,
    pub dot_name: String,
    pub workspace: PathBuf,
    pub dirs: Vec<AuthorizedDir>,
    pub policy: ToolPolicy,
    /// Canal para injetar eventos (aprovacoes) no stream da execucao.
    pub events: mpsc::Sender<ExecutionEvent>,
    /// Hosts dos apps conectados a este DOT (ex.: o Flexi): o navegador abre neles sem exigir rede liberada.
    pub app_hosts: Vec<String>,
    /// Ligado quando o runtime monta o catalogo ou recebe uma chamada do gateway nesta execucao.
    /// Isso nao confirma que o motor disponibilizou o catalogo ao modelo.
    pub gateway_seen: Arc<std::sync::atomic::AtomicBool>,
}

pub type SharedContext = Arc<ExecutionContext>;

/// Host de uma URL (sem esquema, credenciais, porta ou caminho), em minusculas. Vazio se nao houver.
pub fn url_host(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or("");
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    host.split(':').next().unwrap_or("").trim().to_ascii_lowercase()
}

/// Minusculas e sem acento, para comparar rotulos de botoes.
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' | 'ë' => 'e',
            'í' | 'î' | 'ì' | 'ï' => 'i',
            'ó' | 'ô' | 'õ' | 'ò' | 'ö' => 'o',
            'ú' | 'û' | 'ù' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Verbos de acoes irreversiveis (no infinitivo: "Excluidos" ou "Publicados", abas de filtro, nao contam): sempre pedem aprovacao.
const IRREVERSIBLE_WORDS: &[&str] = &["excluir", "exclusao", "deletar", "delete", "remover", "apagar", "finalizar", "publicar", "desvincular", "encerrar", "descartar", "pagar", "comprar"];
/// Rotulos de acoes que gravam algo: risco de escrita.
const CHANGE_WORDS: &[&str] = &["salvar", "enviar", "confirmar", "criar", "cadastrar", "atualizar", "aplicar", "pausar", "ativar", "alterar", "importar", "sincronizar", "gerar", "duplicar", "mover", "adicionar", "incluir", "aprovar", "rejeitar", "responder", "concluir", "vincular", "submit", "save", "send"];

fn has_word(label: &str, words: &[&str]) -> bool {
    let l = fold(label);
    words.iter().any(|w| l.contains(w)) || l.split(|c: char| !c.is_alphanumeric()).any(|t| t == "sim" || t == "ok")
}

/// Risco e descricao de uma acao no navegador. Ler, digitar, escolher opcao e teclas comuns valem o mesmo que abrir
/// a pagina (no app conectado, leitura); clicar em algo que grava e escrita; clicar em algo irreversivel, ou aceitar
/// um dialogo que fala disso, e critico. Elemento sem nome: escrita (nao da para saber o que faz).
fn browser_action(tool: &str, on_app: bool, element: &str, dialog: &str, key: &str, text: &str, option: &str) -> (RiskLevel, String) {
    let base = if on_app { RiskLevel::Read } else { RiskLevel::Exec };
    let element = if element.is_empty() { "elemento" } else { element };
    let named = element.contains('"');
    let clip = |t: &str| t.chars().take(60).collect::<String>();
    match tool {
        "dots_browser_read" => (RiskLevel::Read, "ler a pagina".into()),
        "dots_browser_click" => {
            let risk = if has_word(element, IRREVERSIBLE_WORDS) && !has_word(element, &["cancel"]) {
                RiskLevel::Critical
            } else if !named || has_word(element, CHANGE_WORDS) {
                base.max(RiskLevel::Write)
            } else {
                base
            };
            (risk, format!("clicar em {element}"))
        }
        "dots_browser_press" if !dialog.is_empty() => {
            if key.eq_ignore_ascii_case("Enter") {
                let risk = if has_word(dialog, IRREVERSIBLE_WORDS) { RiskLevel::Critical } else { base.max(RiskLevel::Write) };
                (risk, format!("aceitar o dialogo {}", clip(dialog)))
            } else {
                (base, format!("cancelar o dialogo {}", clip(dialog)))
            }
        }
        "dots_browser_press" => (base, format!("tecla {key}")),
        "dots_browser_type" => (base, format!("digitar \"{}\" em {element}", clip(text))),
        _ => (base, format!("escolher \"{}\" em {element}", clip(option))),
    }
}

/// Primeira palavra da lista que aparece no rotulo (sem acento, minusculas).
fn first_word(label: &str, words: &[&'static str]) -> Option<&'static str> {
    let l = fold(label);
    words.iter().copied().find(|w| l.contains(w))
}

/// Tipo de uma acao para as permissoes aprendidas: aprovada uma vez, as do mesmo tipo do mesmo DOT nao pedem de novo.
/// Devolve (chave, descricao). `None` para o que sempre pede (demissao) ou nao tem tipo seguro (elemento sem nome,
/// ferramenta nativa desconhecida). `input` de acoes no navegador ja traz `_page_url`, `_element` e `_dialog`.
pub fn learn_key(tool: &str, input: &Value) -> Option<(String, String)> {
    let s = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if tool.starts_with("flexi_") {
        let ft = dots_flexi::find_tool(tool)?;
        return Some((tool.to_string(), format!("Flexi: {}", ft.description.split('.').next().unwrap_or(ft.name))));
    }
    let host = url_host(&s("_page_url"));
    match tool {
        "dots_dismiss_employee" | "dots_hire_employee" | "dots_pause_employee" | "dots_resume_employee" | "permission_prompt" => None,
        "dots_browser_click" => {
            let el = s("_element");
            if !el.contains('"') {
                return None;
            }
            let label = el.split('"').nth(1).unwrap_or("").trim().to_string();
            let what = first_word(&el, IRREVERSIBLE_WORDS).or_else(|| first_word(&el, CHANGE_WORDS)).map(str::to_string).unwrap_or_else(|| fold(&label));
            Some((format!("{tool}|{host}|{what}"), format!("clicar em \"{label}\" em {host}")))
        }
        "dots_browser_press" if !s("_dialog").is_empty() => {
            let d = s("_dialog");
            let what = first_word(&d, IRREVERSIBLE_WORDS).or_else(|| first_word(&d, CHANGE_WORDS)).map(str::to_string)?;
            Some((format!("{tool}|{host}|dialogo:{what}"), format!("confirmar dialogos de \"{what}\" em {host}")))
        }
        "dots_browser_open" => {
            let h = url_host(&s("url"));
            Some((format!("{tool}|{h}"), format!("abrir paginas de {h}")))
        }
        "dots_browser_press" | "dots_browser_type" | "dots_browser_select" | "dots_browser_read" => Some((format!("{tool}|{host}"), format!("{} em {host}", tool.trim_start_matches("dots_browser_")))),
        "dots_write_file" | "dots_delete_file" => Some((format!("{tool}|{}", s("path")), format!("{} {}", if tool == "dots_write_file" { "escrever" } else { "excluir" }, s("path")))),
        "dots_run_command" => Some((format!("{tool}|{}", s("command")), format!("rodar `{}`", s("command")))),
        "dots_git" => Some((format!("{tool}|{}", s("op")), format!("git {}", s("op")))),
        "dots_remember" => Some((tool.to_string(), "guardar memorias".into())),
        // Ferramentas nativas do CLI (permission_prompt).
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => Some((format!("native:{tool}|{}", s("file_path")), format!("editar {}", s("file_path")))),
        "Bash" | "PowerShell" => Some((format!("native:{tool}|{}", s("command")), format!("rodar `{}`", s("command")))),
        "WebFetch" | "WebSearch" => {
            let h = url_host(&s("url"));
            Some((format!("native:{tool}|{h}"), format!("{tool} em {h}")))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow { risk: RiskLevel, resources: Vec<String>, summary: String },
    NeedsApproval { risk: RiskLevel, resources: Vec<String>, summary: String },
    Deny(String),
}

/// Normaliza sem exigir existencia: resolve `..` lexicamente apos ancorar no workspace.
fn lexical_absolute(base: &Path, p: &str) -> PathBuf {
    let raw = PathBuf::from(p);
    let joined = if raw.is_absolute() { raw } else { base.join(raw) };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicaliza o maior prefixo existente (para arquivos novos) e reanexa o resto.
fn canonical_best_effort(p: &Path) -> PathBuf {
    let mut existing = p.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = vec![];
    while !existing.exists() {
        match existing.file_name() {
            Some(n) => {
                rest.push(n.to_os_string());
                existing.pop();
            }
            None => break,
        }
    }
    let mut base = std::fs::canonicalize(&existing).unwrap_or(existing);
    for r in rest.iter().rev() {
        base.push(r);
    }
    base
}

impl ExecutionContext {
    /// Resolve um caminho e garante que esta dentro de um diretorio autorizado
    /// com a permissao pedida. Symlinks que apontem para fora sao negados.
    pub fn authorize_path(&self, input: &str, write: bool) -> DotsResult<PathBuf> {
        if input.trim().is_empty() {
            return Err(DotsError::Validation("caminho vazio".into()));
        }
        let lexical = lexical_absolute(&self.workspace, input);
        let resolved = canonical_best_effort(&lexical);
        for d in &self.dirs {
            let root = std::fs::canonicalize(&d.path).unwrap_or(d.path.clone());
            if resolved.starts_with(&root) {
                if write && !d.can_write {
                    return Err(DotsError::Unauthorized(format!("sem permissao de escrita em {}", d.path.display())));
                }
                if !write && !d.can_read {
                    return Err(DotsError::Unauthorized(format!("sem permissao de leitura em {}", d.path.display())));
                }
                if resolved.components().any(|c| c.as_os_str() == ".dots") {
                    return Err(DotsError::Unauthorized("diretorio interno .dots e reservado".into()));
                }
                return Ok(resolved);
            }
        }
        Err(DotsError::Unauthorized(format!("caminho fora dos diretorios autorizados: {}", lexical.display())))
    }

    /// Avalia uma chamada de ferramenta do gateway.
    pub fn evaluate(&self, tool: &str, input: &Value) -> Decision {
        self.evaluate_with_network(tool, input, self.policy.allow_network)
    }

    /// Considera uma permissao de navegador para o dominio atual, sem liberar rede para outras ferramentas.
    pub fn evaluate_with_network(&self, tool: &str, input: &Value, browser_network_allowed: bool) -> Decision {
        let (risk, resources, summary) = match self.classify(tool, input, browser_network_allowed) {
            Ok(x) => x,
            Err(e) => return Decision::Deny(e.message().to_string()),
        };
        if risk == RiskLevel::Critical {
            return Decision::NeedsApproval { risk, resources, summary };
        }
        if self.policy.auto_approve_level.covers(risk) {
            Decision::Allow { risk, resources, summary }
        } else {
            Decision::NeedsApproval { risk, resources, summary }
        }
    }

    fn classify(&self, tool: &str, input: &Value, browser_network_allowed: bool) -> DotsResult<(RiskLevel, Vec<String>, String)> {
        let s = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        if let Some(ft) = tool.strip_prefix("flexi_").and_then(|_| dots_flexi::find_tool(tool)) {
            let risk = crate::flexi::risk_of(ft);
            let ids = input.get("ids").map(|v| v.to_string()).or_else(|| input.get("id").map(|v| v.to_string())).unwrap_or_default();
            let summary = format!("Flexi: {} {}", ft.description.split('.').next().unwrap_or(ft.name), ids);
            return Ok((risk, vec![format!("flexi:{}", ft.path)], summary));
        }
        match tool {
            "dots_list_dir" | "dots_read_file" | "dots_search" => {
                let p = self.authorize_path(&s("path"), false)?;
                Ok((RiskLevel::Read, vec![p.to_string_lossy().into()], format!("{tool} {}", p.display())))
            }
            "dots_write_file" => {
                let p = self.authorize_path(&s("path"), true)?;
                let len = s("content").len() as u64;
                if len > self.policy.max_file_bytes {
                    return Err(DotsError::Unauthorized(format!("conteudo excede o limite de {} bytes", self.policy.max_file_bytes)));
                }
                Ok((RiskLevel::Write, vec![p.to_string_lossy().into()], format!("escrever {} ({len} bytes)", p.display())))
            }
            "dots_delete_file" => {
                let p = self.authorize_path(&s("path"), true)?;
                Ok((RiskLevel::Critical, vec![p.to_string_lossy().into()], format!("excluir {}", p.display())))
            }
            "dots_run_command" => {
                let cmd = s("command");
                let cwd = if s("cwd").is_empty() { self.workspace.clone() } else { self.authorize_path(&s("cwd"), false)? };
                let risk = classify_command(&cmd, &self.policy)?;
                Ok((risk, vec![cwd.to_string_lossy().into()], format!("executar `{cmd}` em {}", cwd.display())))
            }
            "dots_git" => {
                let op = s("op");
                let cwd = if s("cwd").is_empty() { self.workspace.clone() } else { self.authorize_path(&s("cwd"), false)? };
                let args: Vec<String> = input.get("args").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                let risk = match op.as_str() {
                    "status" | "diff" | "log" | "show" | "branch_list" | "remote" => RiskLevel::Read,
                    "add" | "commit" | "checkout" | "branch_create" | "stash" | "restore" => {
                        if args.iter().any(|a| a == "--hard" || a == "-f" || a == "--force") { RiskLevel::Critical } else { RiskLevel::Write }
                    }
                    "push" | "reset" | "clean" | "rebase" | "merge" | "tag" => RiskLevel::Critical,
                    other => return Err(DotsError::Validation(format!("operacao git desconhecida: {other}"))),
                };
                for a in &args {
                    if a.starts_with('-') && (a.contains("exec") || a == "-c" || a.starts_with("--upload-pack") || a.starts_with("--receive-pack")) {
                        return Err(DotsError::Unauthorized(format!("argumento git nao permitido: {a}")));
                    }
                }
                Ok((risk, vec![cwd.to_string_lossy().into()], format!("git {op} {} em {}", args.join(" "), cwd.display())))
            }
            "dots_system_info" => Ok((RiskLevel::Read, vec![], "informacoes do sistema".into())),
            "dots_checkpoint" => Ok((RiskLevel::Read, vec![], format!("checkpoint: {}", s("step")))),
            "dots_delegation_status" | "dots_list_team" => Ok((RiskLevel::Read, vec![], tool.to_string())),
            // Agendar o proprio trabalho nao altera nada fora do DOTS; o usuario ve e apaga no perfil do DOT.
            "dots_schedule_create" => Ok((RiskLevel::Read, vec![], format!("agendar ({} {}): {}", s("kind"), s("spec"), s("prompt").chars().take(60).collect::<String>()))),
            "dots_schedule_list" => Ok((RiskLevel::Read, vec![], "listar agendamentos".into())),
            "dots_schedule_cancel" => Ok((RiskLevel::Read, vec![s("id")], format!("cancelar agendamento {}", s("id")))),
            // Metas so mudam dados do DOTS; o usuario acompanha no Flexi web.
            "dots_goal_create" => Ok((RiskLevel::Read, vec![], format!("registrar meta: {}", s("title").chars().take(60).collect::<String>()))),
            "dots_goal_progress" => Ok((RiskLevel::Read, vec![s("id")], format!("lancar progresso da meta {}", s("id")))),
            "dots_goal_update" => Ok((RiskLevel::Read, vec![s("id")], format!("alterar meta {}", s("id")))),
            "dots_goal_list" => Ok((RiskLevel::Read, vec![], "listar metas".into())),
            "dots_goal_close" => Ok((RiskLevel::Read, vec![s("id")], format!("encerrar meta {}", s("id")))),
            "dots_delegate" => Ok((RiskLevel::Read, vec![s("to")], format!("delegar a {}: {}", s("to"), s("request").chars().take(60).collect::<String>()))),
            "dots_cancel_delegation" => Ok((RiskLevel::Read, vec![s("id")], "cancelar delegacao".into())),
            "dots_ask_user" => Ok((RiskLevel::Read, vec![], format!("perguntar: {}", s("question").chars().take(60).collect::<String>()))),
            "dots_notify_user" => Ok((RiskLevel::Read, vec![], format!("avisar o usuario: {}", s("title").chars().take(60).collect::<String>()))),
            "dots_resume_delegation" => Ok((RiskLevel::Read, vec![], format!("retomar delegacao {}", s("id")))),
            "dots_decide_approval" => Ok((RiskLevel::Read, vec![], format!("decidir permissao do funcionario ({}): {}", s("approved"), s("approval_id")))),
            "dots_answer_question" => Ok((RiskLevel::Read, vec![], format!("responder ao funcionario: {}", s("answer").chars().take(60).collect::<String>()))),
            // Contratar cria um DOT na equipe do gerente, dentro do limite do usuario e sem mais permissoes que o gerente.
            "dots_hire_employee" => {
                if s("name").trim().is_empty() || s("role").trim().is_empty() {
                    return Err(DotsError::Validation("informe o nome (name) e a funcao (role) do funcionario".into()));
                }
                Ok((RiskLevel::Read, vec![s("name")], format!("contratar {}: {}", s("name"), s("role").chars().take(80).collect::<String>())))
            }
            // Pausar/retomar um funcionario da propria equipe e reversivel: gestao, sem aprovacao.
            "dots_pause_employee" | "dots_resume_employee" => {
                if s("dot").trim().is_empty() {
                    return Err(DotsError::Validation("informe o funcionario (dot)".into()));
                }
                Ok((RiskLevel::Read, vec![s("dot")], format!("{} {}", if tool == "dots_pause_employee" { "pausar" } else { "retomar" }, s("dot"))))
            }
            // Irreversivel (exclui o funcionario): sempre passa pelo usuario.
            "dots_dismiss_employee" => {
                if s("dot").trim().is_empty() || s("reason").trim().is_empty() {
                    return Err(DotsError::Validation("informe o funcionario (dot) e o motivo (reason)".into()));
                }
                Ok((RiskLevel::Critical, vec![s("dot")], format!("demitir {}: {}", s("dot"), s("reason").chars().take(100).collect::<String>())))
            }
            "dots_browser_open" => {
                let url = s("url");
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(DotsError::Validation("a URL deve comecar com http:// ou https://".into()));
                }
                let host = url_host(&url);
                // App conectado (ex.: Flexi): e o trabalho normal do DOT, so leitura de tela, sem exigir rede liberada.
                if !host.is_empty() && self.app_hosts.iter().any(|h| host == *h || host.ends_with(&format!(".{h}"))) {
                    return Ok((RiskLevel::Read, vec![url.clone()], format!("abrir no app conectado: {}", url.chars().take(100).collect::<String>())));
                }
                if !browser_network_allowed {
                    return Err(DotsError::Unauthorized("acesso a rede desabilitado para este DOT: sites externos exigem permissao do usuario".into()));
                }
                Ok((RiskLevel::Exec, vec![url.clone()], format!("abrir no navegador: {}", url.chars().take(100).collect::<String>())))
            }
            "dots_browser_click" | "dots_browser_type" | "dots_browser_press" | "dots_browser_select" | "dots_browser_read" => {
                // `_page_url`, `_element` e `_dialog` sao preenchidos pelo gateway a partir do navegador (nao pelo DOT).
                let url = s("_page_url");
                let host = url_host(&url);
                if url.is_empty() || url == "about:blank" {
                    return Err(DotsError::Validation("o navegador do DOT nao tem pagina aberta: use dots_browser_open antes".into()));
                }
                let on_app = !host.is_empty() && self.app_hosts.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")));
                if !on_app && !browser_network_allowed {
                    return Err(DotsError::Unauthorized(format!("a pagina atual ({host}) nao e de um app conectado e o acesso a rede esta desabilitado para este DOT")));
                }
                let place = url.split(['?', '#']).next().unwrap_or("").trim_start_matches("https://").trim_start_matches("http://").chars().take(100).collect::<String>();
                let (risk, what) = browser_action(tool, on_app, &s("_element"), &s("_dialog"), &s("key"), &s("text"), &s("option"));
                Ok((risk, vec![url.clone()], format!("navegador: {what} ({place})")))
            }
            "dots_remember" => Ok((RiskLevel::Write, vec![], format!("gravar memoria: {}", s("content").chars().take(60).collect::<String>()))),
            "dots_recall" => Ok((RiskLevel::Read, vec![], format!("buscar memoria: {}", s("query")))),
            other => Err(DotsError::Validation(format!("ferramenta desconhecida: {other}"))),
        }
    }

    /// Classifica uma ferramenta nativa do CLI (quando `engine_native_write`), para o `permission_prompt`.
    pub fn evaluate_native(&self, tool_name: &str, input: &Value) -> Decision {
        let s = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let name = tool_name.trim_start_matches("mcp__");
        let result: DotsResult<(RiskLevel, Vec<String>, String)> = (|| match name {
            "Read" | "Glob" | "Grep" | "LS" | "NotebookRead" => {
                let p = s("file_path");
                let p = if p.is_empty() { s("path") } else { p };
                let resolved = if p.is_empty() { self.workspace.clone() } else { self.authorize_path(&p, false)? };
                Ok((RiskLevel::Read, vec![resolved.to_string_lossy().into()], format!("{name} {}", resolved.display())))
            }
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
                let p = self.authorize_path(&s("file_path"), true)?;
                Ok((RiskLevel::Write, vec![p.to_string_lossy().into()], format!("{name} {}", p.display())))
            }
            "Bash" | "PowerShell" => {
                let cmd = s("command");
                let risk = classify_command(&cmd, &self.policy)?;
                Ok((risk, vec![self.workspace.to_string_lossy().into()], format!("comando `{cmd}`")))
            }
            "WebFetch" | "WebSearch" => {
                if self.policy.allow_network { Ok((RiskLevel::Exec, vec![s("url")], format!("{name} {}", s("url")))) } else { Err(DotsError::Unauthorized("acesso a rede desabilitado para este DOT".into())) }
            }
            "AskUserQuestion" | "TodoWrite" | "Task" | "Skill" | "EndConversation" => Ok((RiskLevel::Read, vec![], name.to_string())),
            other => Ok((RiskLevel::Critical, vec![], format!("ferramenta nativa {other}"))),
        })();
        match result {
            Err(e) => Decision::Deny(e.message().to_string()),
            Ok((risk, resources, summary)) => {
                if risk != RiskLevel::Critical && self.policy.auto_approve_level.covers(risk) {
                    Decision::Allow { risk, resources, summary }
                } else {
                    Decision::NeedsApproval { risk, resources, summary }
                }
            }
        }
    }
}

const READ_ONLY_PREFIXES: &[&str] = &[
    "ls", "cat ", "head ", "tail ", "wc ", "pwd", "echo ", "which ", "file ", "stat ", "du ", "df ", "date", "uname",
    "git status", "git log", "git diff", "git show", "git branch", "git remote -v", "grep ", "rg ", "find ", "tree",
    "cargo check", "cargo test", "cargo build", "npm test", "pnpm test", "npm run test", "pnpm run test", "pytest", "go test", "make test",
];
const DENY_PATTERNS: &[&str] = &[
    "sudo ", "su ", "rm -rf /", "rm -rf ~", "rm -rf *", "mkfs", ":(){", "> /dev/", "chmod -r 777", "chown -r", "launchctl", "osascript",
    "shutdown", "reboot", "diskutil", "dd if=", "curl ", "wget ", "nc ", "ssh ", "scp ", "rsync ", "open ", "defaults write", "security ",
    "kill -9 -1", "pkill ", "killall ", "crontab", "| sh", "| bash", "|sh", "|bash", "eval ", "base64 -d", "python -c", "python3 -c", "perl -e", "ruby -e", "node -e",
];
const NETWORK_HINTS: &[&str] = &["npm install", "pnpm install", "pip install", "cargo install", "brew ", "git push", "git fetch", "git pull", "git clone", "apt ", "gem install", "go get"];

/// Classifica um comando shell segundo denylist global, allowlist do DOT e conjunto somente-leitura.
pub fn classify_command(cmd: &str, policy: &ToolPolicy) -> DotsResult<RiskLevel> {
    let c = cmd.trim();
    if c.is_empty() {
        return Err(DotsError::Validation("comando vazio".into()));
    }
    let lower = c.to_lowercase();
    for d in DENY_PATTERNS {
        if lower.contains(d) || lower.starts_with(d.trim()) {
            return Err(DotsError::Unauthorized(format!("comando bloqueado pela politica global: `{}`", d.trim())));
        }
    }
    if lower.contains(" -delete") || lower.contains(" -exec") {
        return Err(DotsError::Unauthorized("find com -delete/-exec nao e permitido".into()));
    }
    let is_network = NETWORK_HINTS.iter().any(|h| lower.contains(h));
    if is_network && !policy.allow_network {
        return Err(DotsError::Unauthorized("comando exige rede e o DOT nao tem rede habilitada".into()));
    }
    // Comandos encadeados herdam o maior risco; separadores de shell exigem allowlist explicita.
    let chained = lower.contains("&&") || lower.contains("||") || lower.contains(';') || lower.contains('|') || lower.contains('`') || lower.contains("$(");
    if policy.allowed_commands.iter().any(|p| !p.is_empty() && (lower == p.to_lowercase() || lower.starts_with(&format!("{} ", p.to_lowercase())))) {
        return Ok(RiskLevel::Exec);
    }
    if !chained && READ_ONLY_PREFIXES.iter().any(|p| lower == p.trim() || lower.starts_with(p)) {
        return Ok(RiskLevel::Read);
    }
    Err(DotsError::Unauthorized(format!("comando nao esta na lista permitida do DOT: `{c}`")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dots_core::AutoApproveLevel;

    fn ctx(dir: &Path, level: AutoApproveLevel) -> ExecutionContext {
        let (tx, _rx) = mpsc::channel(8);
        let mut policy = ToolPolicy::default_for("d");
        policy.auto_approve_level = level;
        policy.allowed_commands = vec!["npm run lint".into()];
        ExecutionContext {
            token: "t".into(), execution_id: "e".into(), task_id: "k".into(), dot_id: "d".into(), dot_name: "D".into(),
            workspace: dir.join("ws"),
            dirs: vec![AuthorizedDir { path: dir.join("ws"), can_read: true, can_write: true }, AuthorizedDir { path: dir.join("ro"), can_read: true, can_write: false }],
            policy, events: tx, app_hosts: vec!["flexi.exemplo.com".into()], gateway_seen: Default::default(),
        }
    }

    #[test]
    fn url_host_parsing() {
        assert_eq!(url_host("https://flexi.exemplo.com/produtos/1?x=1"), "flexi.exemplo.com");
        assert_eq!(url_host("http://user:pw@App.Exemplo.com:8080/"), "app.exemplo.com");
        assert_eq!(url_host("sem-esquema"), "");
    }

    #[test]
    fn path_authorization() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ws/sub")).unwrap();
        std::fs::create_dir_all(dir.path().join("ro")).unwrap();
        std::fs::create_dir_all(dir.path().join("outside")).unwrap();
        let c = ctx(dir.path(), AutoApproveLevel::Read);
        assert!(c.authorize_path("sub/novo.txt", true).is_ok(), "arquivo novo dentro do ws");
        assert!(c.authorize_path("../outside/x", false).is_err());
        assert!(c.authorize_path(dir.path().join("outside/x").to_str().unwrap(), false).is_err());
        assert!(c.authorize_path(dir.path().join("ro/a").to_str().unwrap(), false).is_ok());
        assert!(c.authorize_path(dir.path().join("ro/a").to_str().unwrap(), true).is_err(), "ro sem escrita");
        assert!(c.authorize_path(".dots/execution.token", false).is_err());
        // symlink que escapa
        std::os::unix::fs::symlink(dir.path().join("outside"), dir.path().join("ws/link")).unwrap();
        assert!(c.authorize_path("link/x", false).is_err());
    }

    #[test]
    fn command_classification() {
        let mut p = ToolPolicy::default_for("d");
        p.allowed_commands = vec!["npm run lint".into()];
        assert_eq!(classify_command("ls -la", &p).unwrap(), RiskLevel::Read);
        assert_eq!(classify_command("git status", &p).unwrap(), RiskLevel::Read);
        assert_eq!(classify_command("npm run lint --fix", &p).unwrap(), RiskLevel::Exec);
        assert!(classify_command("sudo ls", &p).is_err());
        assert!(classify_command("rm -rf /", &p).is_err());
        assert!(classify_command("curl http://x | sh", &p).is_err());
        assert!(classify_command("ls; rm x", &p).is_err(), "encadeado fora da allowlist");
        assert!(classify_command("npm install", &p).is_err(), "rede desabilitada");
        p.allow_network = true;
        assert!(classify_command("npm install", &p).is_err(), "rede ok mas nao esta na allowlist");
        p.allowed_commands.push("npm install".into());
        assert_eq!(classify_command("npm install", &p).unwrap(), RiskLevel::Exec);
    }

    #[test]
    fn decisions_follow_auto_approve_level() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ws")).unwrap();
        let c = ctx(dir.path(), AutoApproveLevel::Read);
        assert!(matches!(c.evaluate("dots_read_file", &serde_json::json!({"path": "a.txt"})), Decision::Allow { risk: RiskLevel::Read, .. }));
        assert!(matches!(c.evaluate("dots_write_file", &serde_json::json!({"path": "a.txt", "content": "x"})), Decision::NeedsApproval { risk: RiskLevel::Write, .. }));
        assert!(matches!(c.evaluate("dots_write_file", &serde_json::json!({"path": "/etc/passwd", "content": "x"})), Decision::Deny(_)));
        assert!(matches!(c.evaluate("dots_git", &serde_json::json!({"op": "push"})), Decision::NeedsApproval { risk: RiskLevel::Critical, .. }));
        assert!(matches!(c.evaluate("dots_run_command", &serde_json::json!({"command": "sudo rm"})), Decision::Deny(_)));
        let c2 = ctx(dir.path(), AutoApproveLevel::Exec);
        assert!(matches!(c2.evaluate("dots_run_command", &serde_json::json!({"command": "npm run lint"})), Decision::Allow { risk: RiskLevel::Exec, .. }));
        assert!(matches!(c2.evaluate("dots_delete_file", &serde_json::json!({"path": "a"})), Decision::NeedsApproval { risk: RiskLevel::Critical, .. }));
        // demitir: sempre critico (aprovacao do usuario), mesmo com aprovacao automatica ate exec
        assert!(matches!(c2.evaluate("dots_dismiss_employee", &serde_json::json!({"dot": "Lula", "reason": "publicou sem revisar"})), Decision::NeedsApproval { risk: RiskLevel::Critical, .. }));
        assert!(matches!(c2.evaluate("dots_dismiss_employee", &serde_json::json!({"dot": "Lula"})), Decision::Deny(_)), "sem motivo nao passa");
        // nativas do Claude
        assert!(matches!(c2.evaluate_native("Write", &serde_json::json!({"file_path": "b.txt"})), Decision::Allow { .. }));
        assert!(matches!(c2.evaluate_native("Write", &serde_json::json!({"file_path": "/tmp/fora"})), Decision::Deny(_)));
        assert!(matches!(c2.evaluate_native("WebFetch", &serde_json::json!({"url": "http://x"})), Decision::Deny(_)));
        // navegador: exige rede liberada; risco Exec; so http/https
        assert!(matches!(c2.evaluate("dots_browser_open", &serde_json::json!({"url": "https://x"})), Decision::Deny(_)), "rede desabilitada");
        assert!(matches!(c2.evaluate_with_network("dots_browser_open", &serde_json::json!({"url": "https://x"}), true), Decision::Allow { risk: RiskLevel::Exec, .. }), "dominio aprovado so libera o navegador");
        assert!(matches!(c2.evaluate_with_network("dots_browser_open", &serde_json::json!({"url": "file:///etc/passwd"}), true), Decision::Deny(_)));
        assert!(matches!(c2.evaluate_with_network("dots_run_command", &serde_json::json!({"command": "npm install"}), true), Decision::Deny(_)), "dominio aprovado nao libera comandos de rede");
        // app conectado: permitido sem rede liberada, risco de leitura (inclusive subdominio)
        assert!(matches!(c2.evaluate("dots_browser_open", &serde_json::json!({"url": "https://flexi.exemplo.com/produtos/1"})), Decision::Allow { risk: RiskLevel::Read, .. }));
        assert!(matches!(c2.evaluate("dots_browser_open", &serde_json::json!({"url": "https://api.flexi.exemplo.com/"})), Decision::Allow { risk: RiskLevel::Read, .. }));
        assert!(matches!(c2.evaluate("dots_browser_open", &serde_json::json!({"url": "https://naoflexi.exemplo.com/"})), Decision::Deny(_)));
        let mut c3 = ctx(dir.path(), AutoApproveLevel::Exec);
        c3.policy.allow_network = true;
        assert!(matches!(c3.evaluate("dots_browser_open", &serde_json::json!({"url": "https://x"})), Decision::Allow { risk: RiskLevel::Exec, .. }));
        assert!(matches!(c3.evaluate("dots_browser_open", &serde_json::json!({"url": "file:///etc/passwd"})), Decision::Deny(_)));
        let mut c4 = ctx(dir.path(), AutoApproveLevel::Write);
        c4.policy.allow_network = true;
        assert!(matches!(c4.evaluate("dots_browser_open", &serde_json::json!({"url": "https://x"})), Decision::NeedsApproval { risk: RiskLevel::Exec, .. }));
    }

    #[test]
    fn browser_actions_by_label_and_page() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ws")).unwrap();
        // Equipe do Flexi: aprovacao automatica so de leitura, sem internet; o Flexi e app conectado.
        let c = ctx(dir.path(), AutoApproveLevel::Read);
        let page = "https://flexi.exemplo.com/produtos/1520501?editor_tab=Anuncios";
        let act = |tool: &str, extra: serde_json::Value| {
            let mut a = serde_json::json!({ "_page_url": page });
            for (k, v) in extra.as_object().unwrap() {
                a[k] = v.clone();
            }
            c.evaluate(tool, &a)
        };
        assert!(matches!(act("dots_browser_read", serde_json::json!({})), Decision::Allow { risk: RiskLevel::Read, .. }));
        assert!(matches!(act("dots_browser_click", serde_json::json!({"ref": 3, "_element": "tab \"Anúncios\""})), Decision::Allow { risk: RiskLevel::Read, .. }));
        assert!(matches!(act("dots_browser_click", serde_json::json!({"ref": 3, "_element": "tab \"Excluídos\""})), Decision::Allow { risk: RiskLevel::Read, .. }), "aba de filtro nao e acao");
        assert!(matches!(act("dots_browser_type", serde_json::json!({"ref": 4, "text": "kit", "_element": "textbox \"Busca\""})), Decision::Allow { risk: RiskLevel::Read, .. }));
        assert!(matches!(act("dots_browser_press", serde_json::json!({"key": "Enter"})), Decision::Allow { risk: RiskLevel::Read, .. }));
        assert!(matches!(act("dots_browser_click", serde_json::json!({"ref": 5, "_element": "button \"Salvar\""})), Decision::NeedsApproval { risk: RiskLevel::Write, .. }));
        assert!(matches!(act("dots_browser_click", serde_json::json!({"ref": 6, "_element": "button"})), Decision::NeedsApproval { risk: RiskLevel::Write, .. }), "sem nome: nao da para saber");
        match act("dots_browser_click", serde_json::json!({"ref": 7, "_element": "menuitem \"Excluir anúncio\""})) {
            Decision::NeedsApproval { risk: RiskLevel::Critical, summary, resources } => {
                assert_eq!(summary, "navegador: clicar em menuitem \"Excluir anúncio\" (flexi.exemplo.com/produtos/1520501)");
                assert_eq!(resources, vec![page.to_string()]);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(act("dots_browser_click", serde_json::json!({"ref": 8, "_element": "button \"Cancelar exclusão\""})), Decision::Allow { risk: RiskLevel::Read, .. }), "cancelar e seguro");
        assert!(matches!(act("dots_browser_press", serde_json::json!({"key": "Enter", "_dialog": "confirm: \"Deseja excluir o anuncio?\""})), Decision::NeedsApproval { risk: RiskLevel::Critical, .. }));
        assert!(matches!(act("dots_browser_press", serde_json::json!({"key": "Escape", "_dialog": "confirm: \"Deseja excluir o anuncio?\""})), Decision::Allow { risk: RiskLevel::Read, .. }));
        // Pagina fora dos apps conectados, sem internet: negado; sem pagina: negado.
        assert!(matches!(c.evaluate("dots_browser_click", &serde_json::json!({"ref": 1, "_page_url": "https://outro.com/", "_element": "link \"x\""})), Decision::Deny(_)));
        assert!(matches!(c.evaluate("dots_browser_read", &serde_json::json!({})), Decision::Deny(_)));
        // Com internet liberada e aprovacao ate exec, outros sites valem como abrir (exec); irreversivel segue pedindo.
        let mut n = ctx(dir.path(), AutoApproveLevel::Exec);
        n.policy.allow_network = true;
        assert!(matches!(n.evaluate("dots_browser_click", &serde_json::json!({"ref": 1, "_page_url": "https://outro.com/", "_element": "link \"Mais\""})), Decision::Allow { risk: RiskLevel::Exec, .. }));
        assert!(matches!(n.evaluate("dots_browser_click", &serde_json::json!({"ref": 1, "_page_url": "https://outro.com/", "_element": "button \"Comprar agora\""})), Decision::NeedsApproval { risk: RiskLevel::Critical, .. }));
    }

    #[test]
    fn learned_key_is_the_kind_of_action_not_the_instance() {
        let click = |url: &str, el: &str| learn_key("dots_browser_click", &serde_json::json!({ "ref": 3, "_page_url": url, "_element": el }));
        let a = click("https://flexi.exemplo.com/produtos/1", "button \"Publicar\"").unwrap();
        let b = click("https://flexi.exemplo.com/produtos/2", "button \"Publicar\"").unwrap();
        assert_eq!(a.0, b.0, "publicar em outro produto e o mesmo tipo");
        assert_eq!(a.0, "dots_browser_click|flexi.exemplo.com|publicar");
        assert!(a.1.contains("Publicar"));
        assert_ne!(a.0, click("https://flexi.exemplo.com/produtos/1", "button \"Excluir\"").unwrap().0);
        assert_ne!(a.0, click("https://outro.com/x", "button \"Publicar\"").unwrap().0, "outro site e outro tipo");
        assert!(click("https://flexi.exemplo.com/x", "button").is_none(), "elemento sem nome nao aprende");
        assert!(learn_key("dots_dismiss_employee", &serde_json::json!({ "dot": "Ana", "reason": "x" })).is_none(), "demissao sempre pede");
        assert_eq!(learn_key("dots_write_file", &serde_json::json!({ "path": "a.txt" })).unwrap().0, "dots_write_file|a.txt");
    }
}
