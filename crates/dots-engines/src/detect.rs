use std::path::PathBuf;
use std::time::Duration;

use dots_core::{AuthStatus, EngineDetection, EngineKind};

use crate::process::run_command;
use crate::resolve::{candidate_dirs, resolve_executable};
use crate::specs::{AuthProbe, capabilities, spec};

#[derive(Debug, Clone)]
pub struct DetectOptions {
    pub home: PathBuf,
    /// Diretorios candidatos; se vazio, usa [`candidate_dirs`].
    pub dirs: Vec<PathBuf>,
    pub timeout: Duration,
}

impl Default for DetectOptions {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        Self { dirs: candidate_dirs(&home), home, timeout: Duration::from_secs(10) }
    }
}

/// Extrai a primeira sequencia `x.y.z` de um texto.
fn parse_version(text: &str) -> Option<String> {
    for token in text.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',') {
        let t = token.trim_start_matches('v');
        let parts: Vec<&str> = t.split('.').collect();
        if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
            return Some(t.to_string());
        }
    }
    None
}

pub async fn detect_one(kind: EngineKind, override_path: Option<&str>, opts: &DetectOptions) -> EngineDetection {
    let s = spec(kind);
    let mut det = EngineDetection {
        kind,
        display_name: kind.display_name().to_string(),
        installed: false,
        executable_path: None,
        version: None,
        auth_status: AuthStatus::Unknown,
        auth_detail: None,
        capabilities: vec![],
        models: vec![],
        issues: vec![],
        guidance: None,
        checked_at: dots_core::now(),
    };

    let exe = match resolve_executable(s.executable, override_path, &opts.dirs) {
        Ok(p) => p,
        Err(issues) => {
            det.issues = issues;
            det.guidance = Some(s.install_guidance.to_string());
            return det;
        }
    };
    det.executable_path = Some(exe.to_string_lossy().to_string());

    match run_command(&exe, s.version_args, &opts.home, &opts.dirs, opts.timeout).await {
        Ok(out) if out.exit_code == Some(0) => match parse_version(&out.stdout).or_else(|| parse_version(&out.stderr)) {
            Some(v) => {
                det.installed = true;
                det.version = Some(v);
            }
            None => det.issues.push(format!("nao foi possivel interpretar a versao: {}", out.stdout.trim())),
        },
        Ok(out) => det.issues.push(format!(
            "`{} {}` saiu com codigo {:?}: {}",
            s.executable,
            s.version_args.join(" "),
            out.exit_code,
            first_line(&out.stderr).or_else(|| first_line(&out.stdout)).unwrap_or_default()
        )),
        Err(e) => det.issues.push(e.message().to_string()),
    }
    if !det.installed {
        det.guidance = Some(s.install_guidance.to_string());
        return det;
    }
    det.capabilities = capabilities(kind);
    det.models = match kind {
        EngineKind::Codex => crate::models::codex_models(&exe, opts).await,
        EngineKind::Agy => crate::models::agy_models(&exe, opts).await,
        EngineKind::Claude => vec![],
    };

    match s.auth {
        AuthProbe::CodexLoginStatus => match run_command(&exe, &["login", "status"], &opts.home, &opts.dirs, opts.timeout).await {
            Ok(out) => {
                let text = format!("{}\n{}", out.stdout, out.stderr);
                let lower = text.to_lowercase();
                det.auth_detail = first_line(&text);
                if lower.contains("not logged in") {
                    det.auth_status = AuthStatus::Unauthenticated;
                } else if lower.contains("logged in") && out.exit_code == Some(0) {
                    det.auth_status = AuthStatus::Authenticated;
                } else {
                    det.auth_status = AuthStatus::Unknown;
                    det.issues.push("resposta inesperada de `codex login status`".into());
                }
            }
            Err(e) => det.issues.push(e.message().to_string()),
        },
        AuthProbe::ClaudeAuthStatus => match run_command(&exe, &["auth", "status"], &opts.home, &opts.dirs, opts.timeout).await {
            Ok(out) => match serde_json::from_str::<serde_json::Value>(out.stdout.trim()) {
                Ok(v) => match v.get("loggedIn").and_then(|b| b.as_bool()) {
                    Some(true) => {
                        det.auth_status = AuthStatus::Authenticated;
                        let method = v.get("authMethod").and_then(|m| m.as_str()).unwrap_or("?");
                        let email = v.get("email").and_then(|m| m.as_str());
                        det.auth_detail = Some(match email {
                            Some(e) => format!("Autenticado via {method} ({e})"),
                            None => format!("Autenticado via {method}"),
                        });
                    }
                    Some(false) => {
                        det.auth_status = AuthStatus::Unauthenticated;
                        det.auth_detail = Some("Nao autenticado".into());
                    }
                    None => det.issues.push("`claude auth status` sem campo loggedIn".into()),
                },
                Err(_) => {
                    det.issues.push("`claude auth status` nao retornou JSON".into());
                    det.auth_detail = first_line(&out.stdout).or_else(|| first_line(&out.stderr));
                }
            },
            Err(e) => det.issues.push(e.message().to_string()),
        },
        AuthProbe::None => {
            det.auth_status = AuthStatus::Unknown;
            det.auth_detail = Some("Este CLI nao oferece verificacao nao interativa de login".into());
        }
    }
    if det.auth_status == AuthStatus::Unauthenticated {
        det.guidance = Some(s.auth_guidance.to_string());
    } else if det.auth_status == AuthStatus::Unknown && matches!(s.auth, AuthProbe::None) {
        det.guidance = Some(s.auth_guidance.to_string());
    }
    det
}

/// Detecta os tres motores em paralelo. `overrides` = (kind, caminho manual).
pub async fn detect_all(overrides: &[(EngineKind, Option<String>)], opts: &DetectOptions) -> Vec<EngineDetection> {
    let futs = EngineKind::ALL.iter().map(|k| {
        let ov = overrides.iter().find(|(kk, _)| kk == k).and_then(|(_, p)| p.clone());
        async move { detect_one(*k, ov.as_deref(), opts).await }
    });
    futures_join(futs).await
}

async fn futures_join<I, F>(iter: I) -> Vec<EngineDetection>
where
    I: IntoIterator<Item = F>,
    F: std::future::Future<Output = EngineDetection>,
{
    let mut out = vec![];
    let futs: Vec<F> = iter.into_iter().collect();
    // Tres futures: join manual sem dependencia extra.
    let mut iter = futs.into_iter();
    let (a, b, c) = (iter.next(), iter.next(), iter.next());
    if let (Some(a), Some(b), Some(c)) = (a, b, c) {
        let (ra, rb, rc) = tokio::join!(a, b, c);
        out.extend([ra, rb, rc]);
    }
    out
}

fn first_line(s: &str) -> Option<String> {
    s.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("codex-cli 0.162.1").as_deref(), Some("0.162.1"));
        assert_eq!(parse_version("2.1.300 (Claude Code)").as_deref(), Some("2.1.300"));
        assert_eq!(parse_version("1.3.5\n").as_deref(), Some("1.3.5"));
        assert_eq!(parse_version("v24.15.0").as_deref(), Some("24.15.0"));
        assert_eq!(parse_version("sem versao"), None);
    }
}
