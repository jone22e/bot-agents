use std::path::{Path, PathBuf};

use dots_core::{DotsError, DotsResult};
use serde::{Deserialize, Serialize};

/// Limite pratico de `sun_path` no macOS (104 bytes incluindo NUL).
const MAX_SOCKET_PATH_BYTES: usize = 100;

#[derive(Debug, Clone)]
pub struct SocketPaths {
    /// Diretorio (0700) que contem socket e token.
    pub run_dir: PathBuf,
    pub socket_path: PathBuf,
    pub token_path: PathBuf,
}

impl SocketPaths {
    /// Resolve os caminhos a partir do diretorio de dados. Se o caminho do
    /// socket exceder o limite do SO, usa `$TMPDIR/dots-<uid>/`.
    pub fn resolve(data_dir: &Path) -> Self {
        let preferred = data_dir.join("run");
        let sock = preferred.join("rt.sock");
        if sock.as_os_str().len() <= MAX_SOCKET_PATH_BYTES {
            return Self::in_dir(preferred);
        }
        let uid = unsafe { libc::getuid() };
        let tmp = std::env::var_os("TMPDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
        Self::in_dir(tmp.join(format!("dots-{uid}")))
    }

    fn in_dir(run_dir: PathBuf) -> Self {
        Self { socket_path: run_dir.join("rt.sock"), token_path: run_dir.join("token"), run_dir }
    }

    /// Cria o diretorio com permissao 0700.
    pub fn ensure_dir(&self) -> DotsResult<()> {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        if !self.run_dir.exists() {
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&self.run_dir)?;
        }
        std::fs::set_permissions(&self.run_dir, std::fs::Permissions::from_mode(0o700))?;
        Ok(())
    }

    /// Gera e grava um token novo (32 bytes, hex) com permissao 0600.
    pub fn write_new_token(&self) -> DotsResult<String> {
        use rand::RngCore;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let token = hex::encode(bytes);
        let _ = std::fs::remove_file(&self.token_path);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&self.token_path)?;
        f.write_all(token.as_bytes())?;
        Ok(token)
    }

    pub fn read_token(&self) -> DotsResult<String> {
        let t = std::fs::read_to_string(&self.token_path)
            .map_err(|e| DotsError::RuntimeUnavailable(format!("token do runtime indisponivel: {e}")))?;
        Ok(t.trim().to_string())
    }
}

/// Informacoes publicadas pelo runtime em `<data_dir>/runtime.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub pid: u32,
    pub version: String,
    pub proto: u32,
    pub socket_path: String,
    pub token_path: String,
    pub started_at: String,
    pub mode: String,
}

impl RuntimeInfo {
    pub fn file_path(data_dir: &Path) -> PathBuf {
        data_dir.join("runtime.json")
    }

    pub fn write(&self, data_dir: &Path) -> DotsResult<()> {
        let path = Self::file_path(data_dir);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn read(data_dir: &Path) -> DotsResult<Option<Self>> {
        let path = Self::file_path(data_dir);
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read(&path)?;
        Ok(Some(serde_json::from_slice(&raw)?))
    }

    pub fn remove(data_dir: &Path) {
        let _ = std::fs::remove_file(Self::file_path(data_dir));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_data_dir_falls_back_to_tmpdir() {
        let long = PathBuf::from("/".to_string() + &"a".repeat(150));
        let p = SocketPaths::resolve(&long);
        assert!(!p.socket_path.starts_with(&long));
        assert!(p.socket_path.as_os_str().len() <= MAX_SOCKET_PATH_BYTES);
    }

    #[test]
    fn short_data_dir_uses_run_subdir() {
        let p = SocketPaths::resolve(Path::new("/tmp/dots-test"));
        assert_eq!(p.socket_path, PathBuf::from("/tmp/dots-test/run/rt.sock"));
    }

    #[test]
    fn token_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = SocketPaths::resolve(dir.path());
        p.ensure_dir().unwrap();
        let t = p.write_new_token().unwrap();
        assert_eq!(t.len(), 64);
        assert_eq!(p.read_token().unwrap(), t);
        let mode = std::fs::metadata(&p.token_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let dmode = std::fs::metadata(&p.run_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(dmode, 0o700);
    }
}
