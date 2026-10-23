//! Segredos (cookie de sessao e senha opcional) fora do SQLite.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, key: &str);
}

/// Keychain do macOS (servico `com.c2s.dots`, conta = chave).
pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new(service: &str) -> Self {
        Self { service: service.to_string() }
    }
    fn entry(&self, key: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(&self.service, key).map_err(|e| e.to_string())
    }
}

impl SecretStore for KeychainStore {
    fn get(&self, key: &str) -> Option<String> {
        self.entry(key).ok()?.get_password().ok()
    }
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.entry(key)?.set_password(value).map_err(|e| e.to_string())
    }
    fn delete(&self, key: &str) {
        if let Ok(e) = self.entry(key) {
            let _ = e.delete_credential();
        }
    }
}

/// Em memoria (testes e ambientes sem Keychain).
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Option<String> {
        self.map.lock().unwrap().get(key).cloned()
    }
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.map.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) {
        self.map.lock().unwrap().remove(key);
    }
}

/// Arquivo JSON `{chave: valor}` com permissao 0600, na pasta de dados do DOTS. E o cofre de reserva quando o
/// chaveiro do sistema nao funciona (ex.: Docker, cujo seccomp padrao bloqueia `keyctl`/`add_key`).
/// Mesma protecao do banco SQLite e do perfil do navegador, que ja guardam o cookie de sessao em disco.
pub struct FileStore {
    path: PathBuf,
    map: Mutex<HashMap<String, String>>,
}

impl FileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let map = std::fs::read_to_string(&path)
            .ok()
            .and_then(|txt| serde_json::from_str::<HashMap<String, String>>(&txt).ok())
            .unwrap_or_default();
        Self { path, map: Mutex::new(map) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn persist(&self, map: &HashMap<String, String>) -> Result<(), String> {
        let txt = serde_json::to_string(map).map_err(|e| e.to_string())?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
            use std::io::Write;
            f.write_all(txt.as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

impl SecretStore for FileStore {
    fn get(&self, key: &str) -> Option<String> {
        self.map.lock().unwrap().get(key).cloned()
    }
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let mut map = self.map.lock().unwrap();
        map.insert(key.into(), value.into());
        self.persist(&map)
    }
    fn delete(&self, key: &str) {
        let mut map = self.map.lock().unwrap();
        if map.remove(key).is_some() {
            if let Err(e) = self.persist(&map) {
                tracing::warn!(error = %e, path = %self.path.display(), "nao foi possivel gravar o cofre em arquivo");
            }
        }
    }
}

/// Chaveiro do sistema com reserva em arquivo: le de qualquer um dos dois; grava no chaveiro e, se ele falhar,
/// no arquivo. Assim o cookie do Flexi sobrevive a um restart mesmo onde o chaveiro nao existe.
pub struct FallbackStore {
    primary: Box<dyn SecretStore>,
    reserve: Box<dyn SecretStore>,
}

impl FallbackStore {
    pub fn new(primary: impl SecretStore + 'static, reserve: impl SecretStore + 'static) -> Self {
        Self { primary: Box::new(primary), reserve: Box::new(reserve) }
    }
}

impl SecretStore for FallbackStore {
    fn get(&self, key: &str) -> Option<String> {
        self.primary.get(key).or_else(|| self.reserve.get(key))
    }
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        match self.primary.set(key, value) {
            Ok(()) => {
                self.reserve.delete(key);
                Ok(())
            }
            Err(e) => {
                tracing::warn!(error = %e, "chaveiro do sistema indisponivel; segredo guardado no cofre em arquivo");
                self.reserve.set(key, value)
            }
        }
    }
    fn delete(&self, key: &str) {
        self.primary.delete(key);
        self.reserve.delete(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Broken;
    impl SecretStore for Broken {
        fn get(&self, _: &str) -> Option<String> {
            None
        }
        fn set(&self, _: &str, _: &str) -> Result<(), String> {
            Err("keyctl: Operation not permitted".into())
        }
        fn delete(&self, _: &str) {}
    }

    #[test]
    fn file_store_persiste_com_permissao_0600_e_recarrega() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        let store = FileStore::new(&path);
        store.set("flexi.cookie.abc", "valor-1").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let again = FileStore::new(&path);
        assert_eq!(again.get("flexi.cookie.abc").as_deref(), Some("valor-1"));
        again.delete("flexi.cookie.abc");
        assert_eq!(FileStore::new(&path).get("flexi.cookie.abc"), None);
    }

    #[test]
    fn fallback_usa_o_arquivo_quando_o_chaveiro_falha_e_o_chaveiro_quando_funciona() {
        let dir = tempfile::tempdir().unwrap();
        let quebrado = FallbackStore::new(Broken, FileStore::new(dir.path().join("s.json")));
        quebrado.set("k", "v").unwrap();
        assert_eq!(quebrado.get("k").as_deref(), Some("v"));
        assert_eq!(FileStore::new(dir.path().join("s.json")).get("k").as_deref(), Some("v"), "ficou no arquivo");

        let ok = FallbackStore::new(MemoryStore::default(), FileStore::new(dir.path().join("s.json")));
        ok.set("k", "v2").unwrap();
        assert_eq!(ok.get("k").as_deref(), Some("v2"));
        assert_eq!(FileStore::new(dir.path().join("s.json")).get("k"), None, "chaveiro funcionou: arquivo nao guarda copia");
        ok.delete("k");
        assert_eq!(ok.get("k"), None);
    }
}
