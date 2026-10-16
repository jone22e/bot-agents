//! Supervisao do processo `dotsd`.
//!
//! Producao (macOS): o binario embutido (sidecar) e copiado para
//! `<data>/bin/dotsd-<versao>` e um LaunchAgent `com.c2s.dots.runtime` aponta
//! para essa copia estavel (nunca para dentro do .app, que muda em
//! atualizacoes, ao mover o app e com App Translocation).
//! Dev ou falha do launchd: spawn detached.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use dots_core::{DotsError, DotsResult, LAUNCH_AGENT_LABEL};
use dots_ipc::RuntimeInfo;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AppPaths {
    pub start_at_login: bool,
    pub data_dir: String,
    pub logs_dir: String,
    pub bin_dir: String,
    pub launch_agent_plist: String,
    pub bundled_dotsd: Option<String>,
    pub dev_mode: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchdStatus {
    pub supported: bool,
    pub plist_installed: bool,
    pub loaded: bool,
    pub pid: Option<u32>,
    pub last_exit_status: Option<i32>,
}

pub fn dev_mode() -> bool {
    if let Ok(v) = std::env::var("DOTS_RUNTIME_MODE") {
        return v == "detached";
    }
    cfg!(debug_assertions)
}

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("DOTS_DATA_DIR") {
        return PathBuf::from(d);
    }
    dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")).join(".local/share"))
        .join(dots_core::APP_NAME)
}

pub fn launch_agent_plist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
}

/// Sidecar: o Tauri coloca `dotsd` ao lado do executavel principal
/// (Contents/MacOS em bundle; target/debug em dev, onde tambem e o build do workspace).
pub fn bundled_dotsd() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let p = dir.join("dotsd");
    if p.is_file() {
        return Some(p);
    }
    None
}

pub fn app_paths() -> AppPaths {
    let data = data_dir();
    AppPaths {
        start_at_login: start_at_login_enabled(),
        data_dir: data.to_string_lossy().into(),
        logs_dir: data.join("logs").to_string_lossy().into(),
        bin_dir: data.join("bin").to_string_lossy().into(),
        launch_agent_plist: launch_agent_plist_path().to_string_lossy().into(),
        bundled_dotsd: bundled_dotsd().map(|p| p.to_string_lossy().into()),
        dev_mode: dev_mode(),
    }
}

fn dotsd_version(path: &Path) -> DotsResult<String> {
    let out = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| DotsError::Io(format!("executar {}: {e}", path.display())))?;
    let s = String::from_utf8_lossy(&out.stdout);
    s.split_whitespace()
        .nth(1)
        .map(|v| v.to_string())
        .ok_or_else(|| DotsError::Internal(format!("saida inesperada de dotsd --version: {s}")))
}

/// Versao do sidecar embutido e seu caminho.
pub fn bundled_runtime_fingerprint() -> Option<(String, PathBuf)> {
    let src = bundled_dotsd()?;
    let version = dotsd_version(&src).ok()?;
    Some((version, src))
}

/// Dois binarios tem o mesmo conteudo? Compara os bytes (tamanho igual nao basta: builds da mesma versao
/// costumam ter o mesmo tamanho com conteudo diferente, e o runtime antigo continuaria rodando).
pub fn same_content(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.len() == mb.len() => matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y),
        _ => false,
    }
}

/// O runtime em execucao esta desatualizado em relacao ao sidecar deste app?
/// (versao diferente, ou mesma versao com binario instalado de conteudo diferente).
pub fn runtime_is_stale(running_version: &str, data: &Path) -> bool {
    let Some((version, src)) = bundled_runtime_fingerprint() else { return false };
    if version != running_version {
        return true;
    }
    let installed = data.join("bin").join(format!("dotsd-{version}"));
    if !installed.exists() {
        return !dev_mode();
    }
    !same_content(&src, &installed)
}

/// Garante `<data>/bin/dotsd-<versao>` igual ao sidecar embutido. Retorna o caminho.
pub fn install_runtime_binary(data: &Path) -> DotsResult<PathBuf> {
    let src = bundled_dotsd().ok_or_else(|| DotsError::RuntimeUnavailable("binario dotsd nao encontrado no app".into()))?;
    let version = dotsd_version(&src)?;
    let bin_dir = data.join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let dest = bin_dir.join(format!("dotsd-{version}"));
    let needs_copy = !same_content(&src, &dest);
    if needs_copy {
        let tmp = bin_dir.join(format!(".dotsd-{version}.tmp"));
        std::fs::copy(&src, &tmp)?;
        std::fs::rename(&tmp, &dest)?;
        tracing::info!(dest = %dest.display(), "runtime copiado para diretorio estavel");
        // Mantem apenas a versao atual e a anterior.
        let mut olds: Vec<PathBuf> = std::fs::read_dir(&bin_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("dotsd-")).unwrap_or(false) && *p != dest)
            .collect();
        olds.sort();
        while olds.len() > 1 {
            let _ = std::fs::remove_file(olds.remove(0));
        }
    }
    Ok(dest)
}

/// Preferencia "iniciar com o Mac" (RunAtLoad). Guardada ao lado do plist para sobreviver a atualizacoes.
pub fn start_at_login_enabled() -> bool {
    !data_dir().join("no-start-at-login").exists()
}

pub fn set_start_at_login(enabled: bool) -> DotsResult<()> {
    let marker = data_dir().join("no-start-at-login");
    if enabled {
        let _ = std::fs::remove_file(&marker);
    } else {
        std::fs::write(&marker, "1")?;
    }
    // Reescreve o plist se ja houver binario instalado.
    let data = data_dir();
    if let Some(bin) = latest_installed_binary(&data) {
        let plist = launch_agent_plist_path();
        let xml = plist_xml(&bin, &data);
        if plist.exists() {
            std::fs::write(&plist, xml)?;
        }
    }
    Ok(())
}

fn latest_installed_binary(data: &Path) -> Option<PathBuf> {
    let mut bins: Vec<PathBuf> = std::fs::read_dir(data.join("bin")).ok()?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("dotsd-")).unwrap_or(false)).collect();
    bins.sort();
    bins.pop()
}

fn plist_xml(binary: &Path, data: &Path) -> String {
    let logs = data.join("logs");
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>--data-dir</string>
        <string>{data}</string>
        <string>--mode</string>
        <string>launchd</string>
    </array>
    <key>RunAtLoad</key><{run_at_load}/>
    <key>KeepAlive</key>
    <dict><key>SuccessfulExit</key><false/></dict>
    <key>ProcessType</key><string>Background</string>
    <key>ThrottleInterval</key><integer>5</integer>
    <key>WorkingDirectory</key><string>{data}</string>
    <key>StandardOutPath</key><string>{logs}/launchd-stdout.log</string>
    <key>StandardErrorPath</key><string>{logs}/launchd-stderr.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>DOTS_DATA_DIR</key><string>{data}</string>
        <key>PATH</key><string>/usr/bin:/bin:/usr/sbin:/sbin</string>
    </dict>
</dict>
</plist>
"#,
        label = LAUNCH_AGENT_LABEL,
        run_at_load = if start_at_login_enabled() { "true" } else { "false" },
        bin = esc(&binary.to_string_lossy()),
        data = esc(&data.to_string_lossy()),
        logs = esc(&logs.to_string_lossy()),
    )
}

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

fn launchctl(args: &[&str]) -> DotsResult<std::process::Output> {
    Command::new("/bin/launchctl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| DotsError::Io(format!("launchctl: {e}")))
}

pub fn launchd_status() -> LaunchdStatus {
    let plist = launch_agent_plist_path();
    let mut st = LaunchdStatus {
        supported: cfg!(target_os = "macos"),
        plist_installed: plist.exists(),
        loaded: false,
        pid: None,
        last_exit_status: None,
    };
    if !st.supported {
        return st;
    }
    if let Ok(out) = launchctl(&["print", &format!("gui/{}/{}", uid(), LAUNCH_AGENT_LABEL)]) {
        if out.status.success() {
            st.loaded = true;
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let l = line.trim();
                if let Some(v) = l.strip_prefix("pid = ") {
                    st.pid = v.trim().parse().ok();
                }
                if let Some(v) = l.strip_prefix("last exit code = ") {
                    st.last_exit_status = v.trim().split_whitespace().next().and_then(|x| x.parse().ok());
                }
            }
        }
    }
    st
}

/// Escreve o plist (se mudou) e faz bootstrap. Retorna true se foi (re)carregado.
pub fn launchd_install_and_start(binary: &Path, data: &Path) -> DotsResult<()> {
    let plist = launch_agent_plist_path();
    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let xml = plist_xml(binary, data);
    let changed = std::fs::read_to_string(&plist).map(|cur| cur != xml).unwrap_or(true);
    let status = launchd_status();
    if changed {
        if status.loaded {
            let _ = launchctl(&["bootout", &format!("gui/{}/{}", uid(), LAUNCH_AGENT_LABEL)]);
        }
        std::fs::write(&plist, xml)?;
        tracing::info!(plist = %plist.display(), "LaunchAgent gravado");
    }
    let status = launchd_status();
    if !status.loaded {
        let out = launchctl(&["bootstrap", &format!("gui/{}", uid()), &plist.to_string_lossy()])?;
        if !out.status.success() {
            return Err(DotsError::RuntimeUnavailable(format!(
                "launchctl bootstrap falhou: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
    } else if status.pid.is_none() {
        let out = launchctl(&["kickstart", &format!("gui/{}/{}", uid(), LAUNCH_AGENT_LABEL)])?;
        if !out.status.success() {
            return Err(DotsError::RuntimeUnavailable(format!(
                "launchctl kickstart falhou: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
    }
    Ok(())
}

pub fn launchd_stop() -> DotsResult<()> {
    let st = launchd_status();
    if st.loaded {
        let out = launchctl(&["bootout", &format!("gui/{}/{}", uid(), LAUNCH_AGENT_LABEL)])?;
        if !out.status.success() {
            return Err(DotsError::RuntimeUnavailable(format!(
                "launchctl bootout falhou: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
    }
    Ok(())
}

/// Inicia `dotsd` desacoplado (nova sessao, sem herdar stdio).
pub fn spawn_detached(binary: &Path, data: &Path) -> DotsResult<u32> {
    use std::os::unix::process::CommandExt;
    let child = Command::new(binary)
        .args(["--data-dir", &data.to_string_lossy(), "--mode", "detached"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| DotsError::RuntimeUnavailable(format!("iniciar dotsd: {e}")))?;
    Ok(child.id())
}

/// Aguarda o socket do runtime ficar disponivel.
pub async fn wait_runtime_info(data: &Path, timeout: Duration) -> DotsResult<RuntimeInfo> {
    let start = Instant::now();
    loop {
        if let Ok(Some(info)) = RuntimeInfo::read(data) {
            if Path::new(&info.socket_path).exists() && process_alive(info.pid) {
                return Ok(info);
            }
        }
        if start.elapsed() > timeout {
            return Err(DotsError::RuntimeUnavailable("runtime nao respondeu a tempo".into()));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Binario que deve ser usado para iniciar o runtime.
/// Dev: o proprio build do workspace (ao lado do executavel). Producao: copia estavel.
pub fn runtime_binary(data: &Path) -> DotsResult<PathBuf> {
    if dev_mode() {
        return bundled_dotsd().ok_or_else(|| DotsError::RuntimeUnavailable("dotsd nao encontrado ao lado do app (rode `cargo build -p dotsd`)".into()));
    }
    install_runtime_binary(data)
}

#[cfg(test)]
mod tests {
    use super::same_content;

    #[test]
    fn same_content_compares_bytes_not_only_length() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, c) = (dir.path().join("a"), dir.path().join("b"), dir.path().join("c"));
        std::fs::write(&a, b"dotsd-1").unwrap();
        std::fs::write(&b, b"dotsd-2").unwrap();
        std::fs::write(&c, b"dotsd-1").unwrap();
        assert!(!same_content(&a, &b), "mesmo tamanho, conteudo diferente");
        assert!(same_content(&a, &c));
        assert!(!same_content(&a, &dir.path().join("nao-existe")));
    }
}
