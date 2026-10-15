//! Segredos (cookie de sessao e senha opcional) fora do SQLite.

use std::collections::HashMap;
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
