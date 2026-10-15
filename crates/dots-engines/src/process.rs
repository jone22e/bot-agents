use std::path::{Path, PathBuf};
use std::time::Duration;

use dots_core::{DotsError, DotsResult};
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Executa um comando sem TTY, com timeout, HOME definido e PATH composto
/// pelos diretorios candidatos (necessario para wrappers que chamam `node`).
pub async fn run_command(exe: &Path, args: &[&str], home: &Path, path_dirs: &[PathBuf], timeout: Duration) -> DotsResult<CommandOutput> {
    let path_env = std::env::join_paths(path_dirs.iter().cloned())
        .map_err(|e| DotsError::Internal(format!("PATH invalido: {e}")))?;
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .env("HOME", home)
        .env("PATH", path_env)
        .env("USER", crate::adapters::current_username())
        .env("LOGNAME", crate::adapters::current_username())
        .env("TMPDIR", std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into()))
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env_remove("CLAUDECODE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = cmd.spawn().map_err(|e| DotsError::Engine(format!("falha ao iniciar {}: {e}", exe.display())))?;
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| DotsError::Engine(format!("{} excedeu {}s", exe.display(), timeout.as_secs())))?
        .map_err(|e| DotsError::Engine(format!("falha ao aguardar {}: {e}", exe.display())))?;
    Ok(CommandOutput {
        exit_code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}
