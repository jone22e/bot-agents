use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use dots_core::{AuthStatus, EngineKind};
use dots_engines::{DetectOptions, detect_all, detect_one};

fn fake(dir: &std::path::Path, name: &str, script: &str) {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn opts(dir: &std::path::Path) -> DetectOptions {
    DetectOptions {
        home: dir.to_path_buf(),
        dirs: vec![dir.to_path_buf(), "/usr/bin".into(), "/bin".into()],
        timeout: Duration::from_secs(5),
    }
}

#[tokio::test]
async fn detects_fake_codex_logged_in() {
    let dir = tempfile::tempdir().unwrap();
    fake(
        dir.path(),
        "codex",
        r#"case "$1" in --version) echo "codex-cli 0.162.1";; login) echo "Logged in using ChatGPT";; esac"#,
    );
    let d = detect_one(EngineKind::Codex, None, &opts(dir.path())).await;
    assert!(d.installed);
    assert_eq!(d.version.as_deref(), Some("0.162.1"));
    assert_eq!(d.auth_status, AuthStatus::Authenticated);
    assert_eq!(d.auth_detail.as_deref(), Some("Logged in using ChatGPT"));
    assert!(!d.capabilities.is_empty());
    assert!(d.issues.is_empty());
}

#[tokio::test]
async fn detects_fake_codex_logged_out() {
    let dir = tempfile::tempdir().unwrap();
    fake(dir.path(), "codex", r#"case "$1" in --version) echo "codex-cli 0.162.1";; login) echo "Not logged in"; exit 1;; esac"#);
    let d = detect_one(EngineKind::Codex, None, &opts(dir.path())).await;
    assert_eq!(d.auth_status, AuthStatus::Unauthenticated);
    assert!(d.guidance.is_some());
}

#[tokio::test]
async fn detects_fake_claude_json_auth() {
    let dir = tempfile::tempdir().unwrap();
    fake(
        dir.path(),
        "claude",
        r#"case "$1" in --version) echo "2.1.300 (Claude Code)";; auth) echo '{"loggedIn":true,"authMethod":"claude.ai","email":"x@y"}';; esac"#,
    );
    let d = detect_one(EngineKind::Claude, None, &opts(dir.path())).await;
    assert_eq!(d.version.as_deref(), Some("2.1.300"));
    assert_eq!(d.auth_status, AuthStatus::Authenticated);
    assert_eq!(d.auth_detail.as_deref(), Some("Autenticado via claude.ai (x@y)"));
}

#[tokio::test]
async fn agy_auth_is_unknown_by_design() {
    let dir = tempfile::tempdir().unwrap();
    fake(dir.path(), "agy", r#"echo "1.3.5""#);
    let d = detect_one(EngineKind::Agy, None, &opts(dir.path())).await;
    assert!(d.installed);
    assert_eq!(d.auth_status, AuthStatus::Unknown);
    assert!(d.guidance.is_some());
}

#[tokio::test]
async fn missing_engine_reports_guidance() {
    let dir = tempfile::tempdir().unwrap();
    let d = detect_one(EngineKind::Codex, None, &opts(dir.path())).await;
    assert!(!d.installed);
    assert!(d.executable_path.is_none());
    assert!(d.guidance.is_some());
    assert!(!d.issues.is_empty());
}

#[tokio::test]
async fn hanging_engine_times_out() {
    let dir = tempfile::tempdir().unwrap();
    fake(dir.path(), "claude", "sleep 30");
    let mut o = opts(dir.path());
    o.timeout = Duration::from_millis(300);
    let d = detect_one(EngineKind::Claude, None, &o).await;
    assert!(!d.installed);
    assert!(d.issues.iter().any(|i| i.contains("excedeu")));
}

#[tokio::test]
async fn detect_all_returns_three() {
    let dir = tempfile::tempdir().unwrap();
    let all = detect_all(&[], &opts(dir.path())).await;
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|d| !d.installed));
}
