use std::process::Stdio;
use std::time::Duration;

use dots_core::{DotsError, DotsResult};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::request::PreparedCommand;

/// Eventos brutos do processo.
#[derive(Debug, Clone)]
pub enum RunnerEvent {
    Stdout(String),
    Stderr(String),
    Exited { code: Option<i32>, timed_out: bool, killed: bool },
}

/// Controle do processo em execucao.
pub struct ProcessHandle {
    pub pid: u32,
    cancel_tx: tokio::sync::watch::Sender<bool>,
}

impl ProcessHandle {
    /// Pede cancelamento: SIGINT no grupo, depois SIGKILL apos a carencia.
    pub fn cancel(&self) {
        let _ = self.cancel_tx.send(true);
    }
}

fn signal_group(pid: u32, sig: i32) {
    unsafe {
        // Grupo de processo = pid (process_group(0) no spawn).
        if libc::killpg(pid as i32, sig) != 0 {
            libc::kill(pid as i32, sig);
        }
    }
}

/// Inicia o comando em grupo de processo proprio, sem TTY, com timeout.
/// Linhas de stdout/stderr chegam pelo canal; `Exited` e sempre o ultimo evento.
pub async fn spawn_prepared(cmd: PreparedCommand, timeout: Duration, grace: Duration) -> DotsResult<(ProcessHandle, mpsc::Receiver<RunnerEvent>)> {
    let mut c = Command::new(&cmd.program);
    c.args(&cmd.args)
        .current_dir(&cmd.cwd)
        .env_clear()
        .envs(cmd.env.iter().cloned())
        .stdin(if cmd.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    let mut child = c.spawn().map_err(|e| DotsError::Engine(format!("falha ao iniciar {}: {e}", cmd.program.display())))?;
    let pid = child.id().ok_or_else(|| DotsError::Engine("processo sem pid".into()))?;

    if let (Some(input), Some(mut stdin)) = (cmd.stdin.clone(), child.stdin.take()) {
        tokio::spawn(async move {
            let _ = stdin.write_all(input.as_bytes()).await;
            let _ = stdin.shutdown().await;
        });
    }
    let stdout = child.stdout.take().ok_or_else(|| DotsError::Engine("sem stdout".into()))?;
    let stderr = child.stderr.take().ok_or_else(|| DotsError::Engine("sem stderr".into()))?;

    let (tx, rx) = mpsc::channel::<RunnerEvent>(1024);
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);

    let tx_out = tx.clone();
    let out_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx_out.send(RunnerEvent::Stdout(line)).await.is_err() {
                break;
            }
        }
    });
    let tx_err = tx.clone();
    let err_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx_err.send(RunnerEvent::Stderr(line)).await.is_err() {
                break;
            }
        }
    });

    tokio::spawn(async move {
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        let mut timed_out = false;
        let mut killed = false;
        let status = loop {
            tokio::select! {
                st = child.wait() => break st.ok(),
                _ = &mut deadline => {
                    timed_out = true;
                    signal_group(pid, libc::SIGINT);
                    tokio::time::sleep(grace).await;
                    signal_group(pid, libc::SIGKILL);
                    killed = true;
                    break child.wait().await.ok();
                }
                changed = cancel_rx.changed() => {
                    if changed.is_err() || !*cancel_rx.borrow() { continue; }
                    signal_group(pid, libc::SIGINT);
                    let exited = tokio::time::timeout(grace, child.wait()).await;
                    match exited {
                        Ok(st) => break st.ok(),
                        Err(_) => {
                            signal_group(pid, libc::SIGKILL);
                            killed = true;
                            break child.wait().await.ok();
                        }
                    }
                }
            }
        };
        let _ = out_task.await;
        let _ = err_task.await;
        let code = status.and_then(|s| s.code());
        let _ = tx.send(RunnerEvent::Exited { code, timed_out, killed }).await;
    });

    Ok((ProcessHandle { pid, cancel_tx }, rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sh(script: &str, stdin: Option<&str>) -> PreparedCommand {
        PreparedCommand {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".into(), script.into()],
            env: vec![("PATH".into(), "/usr/bin:/bin".into())],
            cwd: std::env::temp_dir(),
            stdin: stdin.map(String::from),
        }
    }

    async fn drain(mut rx: mpsc::Receiver<RunnerEvent>) -> (Vec<String>, Vec<String>, Option<RunnerEvent>) {
        let (mut out, mut err, mut last) = (vec![], vec![], None);
        while let Some(ev) = rx.recv().await {
            match ev {
                RunnerEvent::Stdout(l) => out.push(l),
                RunnerEvent::Stderr(l) => err.push(l),
                e @ RunnerEvent::Exited { .. } => last = Some(e),
            }
        }
        (out, err, last)
    }

    #[tokio::test]
    async fn streams_stdout_stderr_and_exit_code() {
        let (_h, rx) = spawn_prepared(sh("cat; echo err >&2; exit 3", Some("a\nb\n")), Duration::from_secs(5), Duration::from_millis(200)).await.unwrap();
        let (out, err, last) = drain(rx).await;
        assert_eq!(out, vec!["a", "b"]);
        assert_eq!(err, vec!["err"]);
        assert!(matches!(last, Some(RunnerEvent::Exited { code: Some(3), timed_out: false, .. })));
    }

    #[tokio::test]
    async fn timeout_kills_process() {
        let (_h, rx) = spawn_prepared(sh("trap '' INT; sleep 30", None), Duration::from_millis(300), Duration::from_millis(200)).await.unwrap();
        let start = std::time::Instant::now();
        let (_, _, last) = drain(rx).await;
        assert!(matches!(last, Some(RunnerEvent::Exited { timed_out: true, .. })));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn cancel_stops_process() {
        let (h, rx) = spawn_prepared(sh("sleep 30", None), Duration::from_secs(30), Duration::from_millis(300)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        h.cancel();
        let start = std::time::Instant::now();
        let (_, _, last) = drain(rx).await;
        assert!(matches!(last, Some(RunnerEvent::Exited { timed_out: false, .. })));
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
