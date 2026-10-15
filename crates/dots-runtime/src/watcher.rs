//! Observador de arquivos e commits nas pastas autorizadas dos DOTs com gatilhos.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dots_core::automation::event_types as et;
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use serde_json::json;
use tokio::sync::mpsc;

use crate::service::RuntimeService;

const IGNORED_DIRS: &[&str] = &[".git", "node_modules", "target", ".dots", ".venv", "__pycache__", "dist", ".next"];

pub struct Watcher {
    debouncer: Option<Debouncer<notify::RecommendedWatcher, RecommendedCache>>,
    watched: HashSet<PathBuf>,
    tx: mpsc::Sender<Vec<PathBuf>>,
}

impl Watcher {
    pub fn new(tx: mpsc::Sender<Vec<PathBuf>>) -> Self {
        Self { debouncer: None, watched: HashSet::new(), tx }
    }

    /// Reconfigura o conjunto de pastas observadas (chamado quando gatilhos ou pastas mudam).
    pub fn set_paths(&mut self, paths: Vec<PathBuf>) {
        let wanted: HashSet<PathBuf> = paths.into_iter().filter(|p| p.is_dir()).collect();
        if wanted == self.watched {
            return;
        }
        self.debouncer = None;
        self.watched.clear();
        if wanted.is_empty() {
            tracing::info!("observador de arquivos desligado (sem gatilhos)");
            return;
        }
        let tx = self.tx.clone();
        let mut deb = match new_debouncer(Duration::from_secs(2), None, move |r: DebounceEventResult| {
            if let Ok(events) = r {
                let paths: Vec<PathBuf> = events.into_iter().flat_map(|e| e.paths.clone()).collect();
                if !paths.is_empty() {
                    let _ = tx.blocking_send(paths);
                }
            }
        }) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "falha ao criar observador de arquivos");
                return;
            }
        };
        for p in &wanted {
            match deb.watch(p, RecursiveMode::Recursive) {
                Ok(()) => tracing::info!(path = %p.display(), "observando pasta"),
                Err(e) => tracing::warn!(path = %p.display(), error = %e, "nao foi possivel observar"),
            }
        }
        self.debouncer = Some(deb);
        self.watched = wanted;
    }
}

fn is_ignored(path: &Path) -> bool {
    path.components().any(|c| IGNORED_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
}

fn git_head_sha(repo: &Path) -> Option<(String, String)> {
    let head = std::fs::read_to_string(repo.join(".git/HEAD")).ok()?;
    let head = head.trim();
    let sha = if let Some(r) = head.strip_prefix("ref: ") {
        std::fs::read_to_string(repo.join(".git").join(r.trim())).ok()?.trim().to_string()
    } else {
        head.to_string()
    };
    if sha.len() < 7 {
        return None;
    }
    let msg = std::process::Command::new("/usr/bin/git").args(["-C", &repo.to_string_lossy(), "log", "-1", "--pretty=%s"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    Some((sha, msg))
}

/// Encontra a raiz do repositorio git que contem `path` (dentro das pastas observadas).
fn git_root(path: &Path, roots: &HashSet<PathBuf>) -> Option<PathBuf> {
    let mut cur = Some(path);
    while let Some(p) = cur {
        if p.join(".git").is_dir() {
            return Some(p.to_path_buf());
        }
        if roots.contains(p) {
            break;
        }
        cur = p.parent();
    }
    None
}

/// Pastas a observar: as autorizadas de DOTs com gatilhos de arquivo/commit habilitados.
pub async fn compute_paths(service: &RuntimeService) -> Vec<PathBuf> {
    let result = service
        .storage
        .call(|st| {
            let mut dots: HashSet<String> = HashSet::new();
            for s in st.subscriptions().enabled_for_type(et::FILE_CHANGED)? {
                dots.insert(s.dot_id);
            }
            for s in st.subscriptions().enabled_for_type(et::GIT_COMMIT)? {
                dots.insert(s.dot_id);
            }
            let mut paths = vec![];
            for d in dots {
                for w in st.dots().workspaces(&d)? {
                    paths.push(PathBuf::from(w.path));
                }
            }
            Ok(paths)
        })
        .await;
    result.unwrap_or_default()
}

/// Loop: recebe lotes do debouncer, gera eventos `file.changed` e `git.commit.created`.
pub async fn run(service: Arc<RuntimeService>, mut rx: mpsc::Receiver<Vec<PathBuf>>, watcher: Arc<std::sync::Mutex<Watcher>>) {
    let mut last_sha: HashMap<PathBuf, String> = HashMap::new();
    loop {
        let batch = tokio::select! {
            b = rx.recv() => match b { Some(b) => b, None => break },
            _ = service.shutdown.clone().wait() => break,
        };
        let roots = watcher.lock().unwrap().watched.clone();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut repos: HashSet<PathBuf> = HashSet::new();
        for p in batch {
            if p.components().any(|c| c.as_os_str() == ".git") {
                if let Some(root) = git_root(&p, &roots) {
                    repos.insert(root);
                }
                continue;
            }
            if is_ignored(&p) || !seen.insert(p.clone()) {
                continue;
            }
            let meta = std::fs::metadata(&p).ok();
            let key = match &meta {
                Some(m) if m.is_file() => {
                    let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                    format!("file:{}:{}:{}", p.display(), mtime, m.len())
                }
                Some(_) => continue,
                None => format!("file:{}:deleted", p.display()),
            };
            let kind = if meta.is_none() { "deleted" } else { "modified" };
            let _ = service.emit_event(et::FILE_CHANGED, "watcher", json!({ "path": p.to_string_lossy(), "kind": kind }), None, Some(key), 0).await;
        }
        for repo in repos {
            if let Some((sha, msg)) = git_head_sha(&repo) {
                if last_sha.get(&repo) == Some(&sha) {
                    continue;
                }
                last_sha.insert(repo.clone(), sha.clone());
                let key = format!("git:{}:{sha}", repo.display());
                let _ = service.emit_event(et::GIT_COMMIT, "watcher", json!({ "path": repo.to_string_lossy(), "repo": repo.to_string_lossy(), "sha": sha, "message": msg }), None, Some(key), 0).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_internal_dirs() {
        assert!(is_ignored(Path::new("/w/node_modules/x.js")));
        assert!(is_ignored(Path::new("/w/.dots/execution.token")));
        assert!(!is_ignored(Path::new("/w/src/a.rs")));
    }
}
