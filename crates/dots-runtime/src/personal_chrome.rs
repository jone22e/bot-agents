//! Ponte para uma extensao carregada no perfil pessoal do Chromium (Ubuntu).
//! O runtime nunca inicia Chromium nem le seus arquivos de perfil. A extensao
//! controla somente as abas que cria para os DOTs e responde via Native Messaging.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use dots_core::{DotsError, DotsResult};
use dots_ipc::server::EventSink;
use serde_json::{Value, json};
use tokio::sync::{Mutex, oneshot};

use crate::browser::PageSnapshot;

const HEARTBEAT_TTL: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(35);
const HOST_NAME: &str = "com.c2s.dots";

struct Bridge {
    events: EventSink,
    last_seen: StdMutex<Option<Instant>>,
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
}

impl Bridge {
    fn connected(&self) -> bool {
        self.last_seen.lock().unwrap().is_some_and(|at| at.elapsed() < HEARTBEAT_TTL)
    }

    fn heartbeat(&self) {
        *self.last_seen.lock().unwrap() = Some(Instant::now());
    }

    async fn result(&self, id: &str, value: Value) {
        if let Some(waiter) = self.pending.lock().await.remove(id) {
            let _ = waiter.send(value);
        }
    }

    async fn request(&self, dot_id: &str, op: &str, args: Value) -> DotsResult<Value> {
        if !self.connected() {
            return Err(DotsError::RuntimeUnavailable("extensao do Chromium desconectada; configure-a em Conectar aplicativos".into()));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        self.events.emit("browser.personal.command", json!({ "id": id, "dot_id": dot_id, "op": op, "args": args }));
        let result = tokio::time::timeout(REQUEST_TIMEOUT, rx).await;
        self.pending.lock().await.remove(&id);
        let value = result
            .map_err(|_| DotsError::Io(format!("extensao do Chromium nao respondeu a {op}")))?
            .map_err(|_| DotsError::Io("resposta da extensao do Chromium perdida".into()))?;
        if value["ok"] != true {
            return Err(DotsError::Io(value["error"].as_str().unwrap_or("erro na extensao do Chromium").into()));
        }
        Ok(value.get("value").cloned().unwrap_or(Value::Null))
    }
}

pub struct PersonalChrome {
    dot_id: String,
    bridge: Arc<Bridge>,
    op: Mutex<()>,
}

impl PersonalChrome {
    async fn call(&self, op: &str, args: Value) -> DotsResult<Value> {
        self.bridge.request(&self.dot_id, op, args).await
    }

    async fn page(&self, op: &str, args: Value) -> DotsResult<PageSnapshot> {
        let _lock = self.op.lock().await;
        let v = self.call(op, args).await?;
        Ok(PageSnapshot {
            url: v["url"].as_str().unwrap_or("").into(),
            title: v["title"].as_str().unwrap_or("").into(),
            loaded: v["loaded"].as_bool().unwrap_or(false),
            text: v["text"].as_str().unwrap_or("").into(),
        })
    }

    pub async fn open(&self, url: &str) -> DotsResult<PageSnapshot> {
        self.page("open", json!({"url":url,"focus":false})).await
    }

    pub async fn open_focused(&self, url: &str) -> DotsResult<PageSnapshot> {
        self.page("open", json!({"url":url,"focus":true})).await
    }

    pub async fn read(&self) -> DotsResult<PageSnapshot> {
        self.page("read", json!({})).await
    }

    pub async fn click(&self, r: u64) -> DotsResult<PageSnapshot> {
        self.page("click", json!({"ref":r})).await
    }

    pub async fn type_text(&self, r: u64, text: &str, clear: bool, submit: bool) -> DotsResult<PageSnapshot> {
        self.page("type", json!({"ref":r,"text":text,"clear":clear,"submit":submit})).await
    }

    pub async fn select(&self, r: u64, option: &str) -> DotsResult<PageSnapshot> {
        self.page("select", json!({"ref":r,"option":option})).await
    }

    pub async fn press(&self, key: &str) -> DotsResult<PageSnapshot> {
        self.page("press", json!({"key":key})).await
    }

    pub async fn describe(&self, r: u64) -> DotsResult<String> {
        let v = self.call("describe", json!({"ref":r})).await?;
        Ok(v.as_str().unwrap_or("elemento").into())
    }

    pub async fn page_info(&self) -> DotsResult<(String, String)> {
        let v = self.call("status", json!({})).await?;
        if v["running"] != true {
            return Err(DotsError::Io("nenhuma aba aberta para este DOT; use dots_browser_open".into()));
        }
        Ok((v["url"].as_str().unwrap_or("").into(), v["title"].as_str().unwrap_or("").into()))
    }

    pub async fn current_url(&self) -> DotsResult<String> {
        self.page_info().await.map(|(url, _)| url)
    }
}

pub struct PersonalChromePool {
    bridge: Arc<Bridge>,
    inner: Mutex<HashMap<String, Arc<PersonalChrome>>>,
}

impl PersonalChromePool {
    pub fn new(events: EventSink) -> Self {
        Self { bridge: Arc::new(Bridge { events, last_seen: StdMutex::new(None), pending: Mutex::new(HashMap::new()) }), inner: Mutex::new(HashMap::new()) }
    }

    pub fn connected(&self) -> bool {
        self.bridge.connected()
    }

    pub fn heartbeat(&self) {
        self.bridge.heartbeat();
    }

    pub async fn result(&self, id: &str, value: Value) {
        self.bridge.result(id, value).await;
    }

    pub async fn get(&self, dot_id: &str) -> Arc<PersonalChrome> {
        let mut map = self.inner.lock().await;
        map.entry(dot_id.into()).or_insert_with(|| Arc::new(PersonalChrome { dot_id: dot_id.into(), bridge: self.bridge.clone(), op: Mutex::new(()) })).clone()
    }

    pub async fn running(&self, dot_id: &str) -> Option<Arc<PersonalChrome>> {
        if !self.connected() { return None; }
        let browser = self.get(dot_id).await;
        browser.page_info().await.ok().map(|_| browser)
    }

    pub async fn remove(&self, dot_id: &str) {
        self.inner.lock().await.remove(dot_id);
    }
}

fn shell_quote(value: &Path) -> String {
    format!("'{}'", value.to_string_lossy().replace('\'', "'\\''"))
}

fn valid_extension_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| (b'a'..=b'p').contains(&c))
}

/// Exporta a extensao para o usuario carregar sem compactacao. Com o ID informado
/// pelo Chromium, registra o host Native Messaging somente para essa extensao.
pub fn prepare(root: &Path, home: &Path, dotsd: &Path, extension_id: Option<&str>) -> DotsResult<PathBuf> {
    if !cfg!(target_os = "linux") {
        return Err(DotsError::Validation("Chromium pessoal requer Ubuntu/Linux".into()));
    }
    prepare_files(root, home, dotsd, extension_id)
}

fn prepare_files(root: &Path, home: &Path, dotsd: &Path, extension_id: Option<&str>) -> DotsResult<PathBuf> {
    let extension_dir = root.join("chromium-extension");
    std::fs::create_dir_all(&extension_dir)?;
    std::fs::write(extension_dir.join("manifest.json"), include_str!("../../../apps/chromium-extension/manifest.json"))?;
    std::fs::write(extension_dir.join("background.js"), include_str!("../../../apps/chromium-extension/background.js"))?;
    if let Some(id) = extension_id {
        if !valid_extension_id(id) {
            return Err(DotsError::Validation("ID da extensao deve ter 32 letras de a ate p".into()));
        }
        let wrapper = root.join("bin/chromium-host.sh");
        std::fs::create_dir_all(wrapper.parent().unwrap())?;
        std::fs::write(&wrapper, format!("#!/bin/sh\nexec {} chromium-host --data-dir {}\n", shell_quote(dotsd), shell_quote(root)))?;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700))?;
        let hosts = home.join(".config/chromium/NativeMessagingHosts");
        std::fs::create_dir_all(&hosts)?;
        let manifest = json!({ "name": HOST_NAME, "description": "DOTS Chromium bridge", "path": wrapper, "type": "stdio", "allowed_origins": [format!("chrome-extension://{id}/")] });
        std::fs::write(hosts.join(format!("{HOST_NAME}.json")), serde_json::to_vec_pretty(&manifest)?)?;
    }
    Ok(extension_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_extension_id_and_quotes_paths() {
        assert!(valid_extension_id("abcdefghijklmnopabcdefghijklmnop"));
        assert!(!valid_extension_id("abcdefghijklmnopabcdefghijklmnopx"));
        assert!(!valid_extension_id("0123456789abcdef0123456789abcdef"));
        assert_eq!(shell_quote(Path::new("/tmp/a'b")), "'/tmp/a'\\''b'");
    }

    #[test]
    fn exports_extension_and_native_host_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let home = temp.path().join("home");
        let id = "abcdefghijklmnopabcdefghijklmnop";
        let extension = prepare_files(&root, &home, Path::new("/opt/dots/bin/dotsd"), Some(id)).unwrap();
        assert!(extension.join("manifest.json").exists());
        assert!(extension.join("background.js").exists());
        let host: Value = serde_json::from_slice(&std::fs::read(home.join(".config/chromium/NativeMessagingHosts/com.c2s.dots.json")).unwrap()).unwrap();
        assert_eq!(host["allowed_origins"][0], format!("chrome-extension://{id}/"));
        assert_eq!(host["path"], root.join("bin/chromium-host.sh").to_string_lossy().as_ref());
        let wrapper = std::fs::read_to_string(root.join("bin/chromium-host.sh")).unwrap();
        assert!(wrapper.contains("chromium-host --data-dir"));
    }

    #[tokio::test]
    async fn command_and_result_use_the_bridge() {
        let dir = tempfile::tempdir().unwrap();
        let server = dots_ipc::IpcServer::bind(dots_ipc::SocketPaths::resolve(dir.path()), dots_ipc::ServerConfig::default()).unwrap();
        let mut events = server.event_sink().subscribe();
        let pool = PersonalChromePool::new(server.event_sink());
        pool.heartbeat();
        let browser = pool.get("dot-1").await;
        let task = tokio::spawn(async move { browser.open("https://example.test/").await });
        let command = events.recv().await.unwrap();
        assert_eq!(command.event, "browser.personal.command");
        assert_eq!(command.payload["op"], "open");
        assert_eq!(command.payload["dot_id"], "dot-1");
        let id = command.payload["id"].as_str().unwrap();
        pool.result(id, json!({"ok":true,"value":{"url":"https://example.test/","title":"Teste","loaded":true,"text":"ok"}})).await;
        let page = task.await.unwrap().unwrap();
        assert_eq!(page.title, "Teste");
        assert_eq!(page.url, "https://example.test/");
    }
}
