//! Versao instalada de cada CLI (claude, codex, agy) e a mais recente publicada (copiado do ovrgit):
//! - Claude e Codex: a versao `latest` do pacote no npm (a mesma que o instalador de cada um usa).
//! - Antigravity: a raiz do servidor de atualizacoes do proprio CLI responde "Stable Version: x.y.z".
//! Para atualizar, roda o comando `update` do proprio CLI. Para instalar, o instalador oficial de cada um
//! (o mesmo do site: executavel proprio em ~/.local/bin, sem Node nem Homebrew).

use std::path::Path;
use std::time::{Duration, Instant};

use dots_core::{CliUpdateInfo, CliUpdates, DotsError, DotsResult, EngineKind};
use tokio::sync::Mutex;

use crate::service::RuntimeService;

const CACHE: Duration = Duration::from_secs(6 * 3600);

#[derive(Default)]
pub struct UpdatesCache {
    value: Mutex<Option<(Instant, CliUpdates)>>,
}

/// Primeiro "x.y.z" do texto (ex.: "2.1.283 (Claude Code)", "codex-cli 0.158.0", "Stable Version: 1.3.6").
pub fn version_of(text: &str) -> Option<String> {
    for token in text.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',' || c == '"') {
        let t = token.trim_start_matches('v');
        let parts: Vec<&str> = t.split('.').collect();
        if parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
            return Some(t.to_string());
        }
    }
    None
}

/// a > b, comparando parte a parte.
pub fn newer(a: &str, b: &str) -> bool {
    let num = |s: &str| s.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (pa, pb) = (num(a), num(b));
    for i in 0..3 {
        let (x, y) = (pa.get(i).copied().unwrap_or(0), pb.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn npm_package(kind: EngineKind) -> Option<&'static str> {
    match kind {
        EngineKind::Claude => Some("@anthropic-ai/claude-code"),
        EngineKind::Codex => Some("@openai/codex"),
        EngineKind::Agy => None,
    }
}

const AGY_UPDATER: &str = "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app";

/// Instaladores oficiais (script para macOS/Linux).
fn installer(kind: EngineKind) -> &'static str {
    match kind {
        EngineKind::Claude => "https://claude.ai/install.sh",
        EngineKind::Codex => "https://chatgpt.com/codex/install.sh",
        EngineKind::Agy => "https://antigravity.google/cli/install.sh",
    }
}

async fn latest(kind: EngineKind) -> Option<String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(8)).build().ok()?;
    match npm_package(kind) {
        Some(pkg) => {
            let v: serde_json::Value = client.get(format!("https://registry.npmjs.org/{pkg}/latest")).send().await.ok()?.error_for_status().ok()?.json().await.ok()?;
            version_of(v.get("version")?.as_str()?)
        }
        None => version_of(&client.get(AGY_UPDATER).send().await.ok()?.error_for_status().ok()?.text().await.ok()?),
    }
}

impl RuntimeService {
    async fn cli_exe(&self, kind: EngineKind) -> Option<std::path::PathBuf> {
        let cfg = self.storage.call(|s| s.engines().list()).await.ok().and_then(|l| l.into_iter().find(|c| c.kind == kind));
        dots_engines::resolve_executable(kind.executable_name(), cfg.as_ref().and_then(|c| c.executable_override.as_deref()), &self.path_dirs).ok()
    }

    async fn installed_version(&self, kind: EngineKind) -> Option<String> {
        let exe = self.cli_exe(kind).await?;
        let out = dots_engines::run_command(&exe, &["--version"], &self.home, &self.path_dirs, Duration::from_secs(15)).await.ok()?;
        version_of(&out.stdout).or_else(|| version_of(&out.stderr))
    }

    async fn check(&self, kind: EngineKind) -> Option<CliUpdateInfo> {
        let current = self.installed_version(kind).await?; // nao instalado: nada a mostrar
        let latest = latest(kind).await;
        let available = latest.as_deref().map(|l| newer(l, &current)).unwrap_or(false);
        Some(CliUpdateInfo { current, latest, available })
    }

    pub async fn cli_updates(&self, force: bool) -> CliUpdates {
        if !force {
            if let Some((at, v)) = self.cli_updates.value.lock().await.as_ref() {
                if at.elapsed() < CACHE {
                    return v.clone();
                }
            }
        }
        let (claude, codex, agy) = tokio::join!(self.check(EngineKind::Claude), self.check(EngineKind::Codex), self.check(EngineKind::Agy));
        let v = CliUpdates { claude, codex, agy };
        *self.cli_updates.value.lock().await = Some((Instant::now(), v.clone()));
        v
    }

    async fn remember(&self, kind: EngineKind, after: Option<CliUpdateInfo>) {
        let mut g = self.cli_updates.value.lock().await;
        match g.as_mut() {
            Some((_, v)) => v.set(kind, after),
            None => {
                let mut v = CliUpdates { claude: None, codex: None, agy: None };
                v.set(kind, after);
                *g = Some((Instant::now(), v));
            }
        }
    }

    fn last_line(out: &dots_engines::CommandOutput) -> Option<String> {
        let text = if out.stderr.trim().is_empty() { &out.stdout } else { &out.stderr };
        text.lines().map(str::trim).filter(|l| !l.is_empty()).last().map(String::from)
    }

    /// Roda o `update` do proprio CLI e devolve a situacao depois dele.
    pub async fn cli_update(&self, kind: EngineKind) -> DotsResult<Option<CliUpdateInfo>> {
        let exe = self.cli_exe(kind).await.ok_or_else(|| DotsError::NotFound("CLI nao encontrado.".into()))?;
        let out = dots_engines::run_command(&exe, &["update"], &self.home, &self.path_dirs, Duration::from_secs(5 * 60)).await?;
        let after = self.check(kind).await;
        self.remember(kind, after.clone()).await;
        let _ = self.detect_engines(Some(kind)).await;
        if out.exit_code != Some(0) && after.as_ref().map(|a| a.available).unwrap_or(false) {
            return Err(DotsError::Engine(Self::last_line(&out).unwrap_or_else(|| format!("A atualizacao terminou com codigo {:?}.", out.exit_code))));
        }
        Ok(after)
    }

    /// Instala o CLI pelo instalador oficial, sem perguntas, e devolve a situacao depois dele.
    pub async fn cli_install(&self, kind: EngineKind) -> DotsResult<Option<CliUpdateInfo>> {
        let script = format!("CODEX_NON_INTERACTIVE=1 curl -fsSL {} | bash", installer(kind));
        let out = dots_engines::run_command(Path::new("/bin/bash"), &["-c", &script], &self.home, &self.path_dirs, Duration::from_secs(10 * 60)).await?;
        let after = self.check(kind).await;
        self.remember(kind, after.clone()).await;
        let _ = self.detect_engines(Some(kind)).await;
        if after.is_none() {
            return Err(DotsError::Engine(Self::last_line(&out).unwrap_or_else(|| format!("A instalacao terminou com codigo {:?}.", out.exit_code))));
        }
        Ok(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(version_of("2.1.283 (Claude Code)").as_deref(), Some("2.1.283"));
        assert_eq!(version_of("codex-cli 0.158.0").as_deref(), Some("0.158.0"));
        assert_eq!(version_of("Stable Version: 1.3.6\n").as_deref(), Some("1.3.6"));
        assert_eq!(version_of("sem"), None);
        assert!(newer("2.1.301", "2.1.300"));
        assert!(newer("0.163.0", "0.162.1"));
        assert!(!newer("1.3.5", "1.3.5"));
        assert!(!newer("1.3.5", "1.10.0"));
    }
}
