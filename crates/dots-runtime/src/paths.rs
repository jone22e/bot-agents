use std::path::{Path, PathBuf};

use dots_core::{DotsError, DotsResult};

/// Layout do diretorio de dados.
#[derive(Debug, Clone)]
pub struct DataLayout {
    pub root: PathBuf,
    pub db_file: PathBuf,
    pub backups_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub workspaces_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub browsers_dir: PathBuf,
    pub lock_file: PathBuf,
    pub pid_file: PathBuf,
}

pub fn default_data_dir() -> DotsResult<PathBuf> {
    // macOS: ~/Library/Application Support ; Linux: ~/.local/share ; Windows: %APPDATA%
    let base = dirs::data_dir()
        .or_else(|| home_dir().map(|h| h.join(".local/share")))
        .ok_or_else(|| DotsError::Io("nao foi possivel determinar o diretorio de dados".into()))?;
    Ok(base.join(dots_core::APP_NAME))
}

/// HOME com fallback em getpwuid (launchd pode nao definir HOME).
pub fn home_dir() -> Option<PathBuf> {
    if let Some(h) = dirs::home_dir() {
        return Some(h);
    }
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return None;
        }
        let dir = std::ffi::CStr::from_ptr((*pw).pw_dir);
        Some(PathBuf::from(dir.to_string_lossy().to_string()))
    }
}

impl DataLayout {
    pub fn prepare(root: Option<PathBuf>) -> DotsResult<Self> {
        let root = match root {
            Some(r) => r,
            None => default_data_dir()?,
        };
        let layout = Self::from_root(&root);
        for d in [&layout.root, &layout.backups_dir, &layout.logs_dir, &layout.workspaces_dir, &layout.bin_dir, &layout.browsers_dir] {
            std::fs::create_dir_all(d)?;
        }
        Ok(layout)
    }

    pub fn from_root(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            db_file: root.join("dots.db"),
            backups_dir: root.join("backups"),
            logs_dir: root.join("logs"),
            workspaces_dir: root.join("workspaces"),
            bin_dir: root.join("bin"),
            browsers_dir: root.join("browsers"),
            lock_file: root.join("runtime.lock"),
            pid_file: root.join("runtime.pid"),
        }
    }
}
