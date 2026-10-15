//! Adaptadores por motor: montam o comando e traduzem o formato nativo em `ExecutionEvent`.

mod agy;
mod claude;
mod codex;

use dots_core::{DotsResult, EngineKind, ExecutionEvent, FailureKind};

use crate::request::{ExecutionRequest, PreparedCommand};

/// Estado acumulado durante o parse de uma execucao.
#[derive(Debug, Default, Clone)]
pub struct ParserState {
    pub session_id: Option<String>,
    pub last_text: String,
    pub completed: bool,
    pub failed: bool,
    pub stderr_tail: Vec<String>,
    pub usage: serde_json::Value,
    pub cost_usd: Option<f64>,
}

impl ParserState {
    pub fn note_stderr(&mut self, line: &str) {
        if self.stderr_tail.len() >= 20 {
            self.stderr_tail.remove(0);
        }
        self.stderr_tail.push(line.to_string());
    }
}

pub trait AgentEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    /// Monta o comando a partir do pedido (sem executar).
    fn prepare(&self, req: &ExecutionRequest) -> DotsResult<PreparedCommand>;
    /// Traduz uma linha de stdout em eventos normalizados.
    fn parse_line(&self, line: &str, state: &mut ParserState) -> Vec<ExecutionEvent>;
    /// Eventos finais apos o processo sair (garante Completed ou Failed).
    fn finish(&self, exit_code: Option<i32>, timed_out: bool, cancelled: bool, state: &mut ParserState) -> Vec<ExecutionEvent> {
        default_finish(exit_code, timed_out, cancelled, state)
    }
}

pub fn adapter_for(kind: EngineKind) -> Box<dyn AgentEngine> {
    match kind {
        EngineKind::Codex => Box::new(codex::CodexAdapter),
        EngineKind::Claude => Box::new(claude::ClaudeAdapter),
        EngineKind::Agy => Box::new(agy::AgyAdapter),
    }
}

pub(crate) fn default_finish(exit_code: Option<i32>, timed_out: bool, cancelled: bool, state: &mut ParserState) -> Vec<ExecutionEvent> {
    if state.completed || state.failed {
        return vec![];
    }
    if cancelled {
        state.failed = true;
        return vec![ExecutionEvent::Failed { kind: FailureKind::Cancelled, message: "execucao cancelada".into() }];
    }
    if timed_out {
        state.failed = true;
        return vec![ExecutionEvent::Failed { kind: FailureKind::Timeout, message: "tempo limite da execucao excedido".into() }];
    }
    match exit_code {
        Some(0) => {
            state.completed = true;
            vec![ExecutionEvent::Completed { result_text: state.last_text.clone() }]
        }
        code => {
            state.failed = true;
            let tail = state.stderr_tail.join("\n");
            vec![ExecutionEvent::Failed { kind: classify_failure(&tail), message: format!("motor saiu com codigo {code:?}: {}", tail.trim()) }]
        }
    }
}

/// Heuristica simples sobre o texto de erro do CLI.
pub(crate) fn classify_failure(text: &str) -> FailureKind {
    let t = text.to_lowercase();
    if t.contains("not logged in") || t.contains("unauthorized") || t.contains("401") || t.contains("api key") || t.contains("login") || t.contains("authentication") {
        FailureKind::Auth
    } else if t.contains("budget") {
        FailureKind::Budget
    } else if t.contains("unknown option") || t.contains("unexpected argument") || t.contains("invalid") && t.contains("config") {
        FailureKind::Config
    } else {
        FailureKind::Engine
    }
}

pub(crate) fn add_dirs(args: &mut Vec<String>, flag: &str, dirs: &[std::path::PathBuf]) {
    for d in dirs {
        args.push(flag.to_string());
        args.push(d.to_string_lossy().to_string());
    }
}

/// Nome do usuario via getpwuid (launchd pode nao definir USER).
pub fn current_username() -> String {
    if let Ok(u) = std::env::var("USER") {
        if !u.is_empty() {
            return u;
        }
    }
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if !pw.is_null() {
            return std::ffi::CStr::from_ptr((*pw).pw_name).to_string_lossy().to_string();
        }
    }
    "user".into()
}

/// Ambiente minimo e explicito para os CLIs. Verificado: o Claude Code exige USER/LOGNAME
/// para localizar as credenciais (sem eles responde "Not logged in").
pub(crate) fn base_env(req: &ExecutionRequest) -> Vec<(String, String)> {
    let path = std::env::join_paths(req.path_dirs.iter().cloned()).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let user = current_username();
    let mut env = vec![
        ("HOME".to_string(), req.home.to_string_lossy().to_string()),
        ("PATH".to_string(), path),
        ("USER".to_string(), user.clone()),
        ("LOGNAME".to_string(), user),
        ("SHELL".to_string(), std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())),
        ("NO_COLOR".to_string(), "1".to_string()),
        ("TERM".to_string(), "dumb".to_string()),
        ("LANG".to_string(), "en_US.UTF-8".to_string()),
        ("TMPDIR".to_string(), std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into())),
    ];
    env.extend(req.env.iter().cloned());
    env
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::path::PathBuf;

    pub fn request(kind: EngineKind) -> ExecutionRequest {
        ExecutionRequest {
            execution_id: "exec-1".into(),
            engine: kind,
            executable: PathBuf::from(format!("/usr/local/bin/{}", kind.as_str())),
            model: None,
            workspace: PathBuf::from("/tmp/ws"),
            extra_dirs: vec![PathBuf::from("/tmp/extra")],
            prompt: "Faca X".into(),
            system_instructions: "Voce e o DOT QA.".into(),
            resume_session: None,
            new_session_id: Some("11111111-1111-7111-8111-111111111111".into()),
            native_write: false,
            network_allowed: false,
            gateway: Some(crate::request::GatewayConfig {
                server_name: "dots".into(),
                command: PathBuf::from("/opt/dotsd"),
                args: vec!["mcp-gateway".into()],
                env: vec![("DOTS_EXECUTION_TOKEN".into(), "tok".into())],
            }),
            timeout: std::time::Duration::from_secs(60),
            max_turns: Some(10),
            max_budget_usd: Some(1.5),
            home: PathBuf::from("/Users/x"),
            path_dirs: vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
            env: vec![("DOTS_EXECUTION_TOKEN".into(), "tok".into())],
        }
    }

    pub fn fixture(name: &str) -> Vec<String> {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/engines/fixtures/");
        std::fs::read_to_string(format!("{p}{name}")).unwrap().lines().map(String::from).collect()
    }

    pub fn run_fixture(kind: EngineKind, name: &str) -> (Vec<ExecutionEvent>, ParserState) {
        let a = adapter_for(kind);
        let mut st = ParserState::default();
        let mut evs = vec![];
        for l in fixture(name) {
            evs.extend(a.parse_line(&l, &mut st));
        }
        evs.extend(a.finish(Some(0), false, false, &mut st));
        (evs, st)
    }
}
