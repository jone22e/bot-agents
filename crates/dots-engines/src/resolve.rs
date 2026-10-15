use std::path::{Path, PathBuf};

/// Diretorios onde CLIs costumam ser instalados no macOS. A ordem define a
/// prioridade. Nao depende do PATH do processo (apps GUI e launchd nao o herdam),
/// mas o inclui por ultimo.
pub fn candidate_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        home.join(".local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        home.join(".cargo/bin"),
        home.join(".bun/bin"),
        home.join(".volta/bin"),
    ];
    // nvm: versoes mais novas primeiro
    let nvm = home.join(".nvm/versions/node");
    if let Ok(entries) = std::fs::read_dir(&nvm) {
        let mut versions: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path().join("bin"))).collect();
        versions.sort();
        versions.reverse();
        dirs.extend(versions);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&path) {
            if !dirs.contains(&p) {
                dirs.push(p);
            }
        }
    }
    dirs
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Resolve o executavel: override explicito primeiro, depois os diretorios.
/// Retorna o caminho encontrado ou uma lista de problemas.
pub fn resolve_executable(name: &str, override_path: Option<&str>, dirs: &[PathBuf]) -> Result<PathBuf, Vec<String>> {
    let mut issues = vec![];
    if let Some(o) = override_path {
        let p = PathBuf::from(o);
        if is_executable(&p) {
            return Ok(p);
        }
        issues.push(format!("caminho configurado manualmente nao e executavel: {o}"));
    }
    for d in dirs {
        let p = d.join(name);
        if is_executable(&p) {
            return Ok(p);
        }
    }
    issues.push(format!("executavel `{name}` nao encontrado nos diretorios conhecidos"));
    Err(issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_and_missing_reports() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("codex");
        std::fs::write(&bin, "#!/bin/sh\necho x\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(resolve_executable("codex", None, &[dir.path().to_path_buf()]).unwrap(), bin);
        assert_eq!(resolve_executable("codex", Some(bin.to_str().unwrap()), &[]).unwrap(), bin);
        let err = resolve_executable("codex", Some("/nao/existe"), &[]).unwrap_err();
        assert_eq!(err.len(), 2);
    }
}
