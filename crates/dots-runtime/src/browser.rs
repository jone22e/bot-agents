//! Navegador por DOT: um Chrome/Chromium proprio, com perfil (`--user-data-dir`) isolado por DOT,
//! controlado pelo runtime via CDP (Chrome DevTools Protocol) sobre `--remote-debugging-pipe`.
//!
//! Sem dependencias externas: o CDP por pipe e JSON terminado em `\0` nos descritores 3 (entrada
//! do navegador) e 4 (saida). O runtime fala direto com o processo; nenhuma porta e aberta.

use std::collections::HashMap;
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dots_core::{DotsError, DotsResult};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::unix::pipe;
use tokio::sync::{broadcast, oneshot};

const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_SNAPSHOT_NODES: usize = 1500;
/// Quadros do screencast: JPEG reduzido, suficiente para acompanhar o trabalho.
const SCREENCAST_PARAMS: &str = r#"{"format":"jpeg","quality":55,"maxWidth":1280,"maxHeight":900,"everyNthFrame":1}"#;

/// Binario de navegador encontrado na maquina.
#[derive(Debug, Clone)]
pub struct BrowserBinary {
    pub path: PathBuf,
    pub label: String,
}

/// Candidatos em ordem de preferencia: Chrome instalado, Chromium do cache do Playwright, outros Chromium.
pub fn candidates(home: &Path) -> Vec<BrowserBinary> {
    let mut out = vec![
        BrowserBinary { path: PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"), label: "Google Chrome".into() },
        BrowserBinary { path: home.join("Applications/Google Chrome.app/Contents/MacOS/Google Chrome"), label: "Google Chrome".into() },
    ];
    out.extend(playwright_chromium(home));
    out.push(BrowserBinary { path: PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"), label: "Chromium".into() });
    out.push(BrowserBinary { path: PathBuf::from("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"), label: "Brave".into() });
    out.push(BrowserBinary { path: PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"), label: "Microsoft Edge".into() });
    out
}

/// Chromium baixado pelo Playwright (`~/Library/Caches/ms-playwright/chromium-<n>/...`), o mais novo primeiro.
fn playwright_chromium(home: &Path) -> Vec<BrowserBinary> {
    let base = home.join("Library/Caches/ms-playwright");
    let Ok(rd) = std::fs::read_dir(&base) else { return vec![] };
    let mut builds: Vec<(u64, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let n = name.strip_prefix("chromium-")?.parse::<u64>().ok()?;
            Some((n, e.path()))
        })
        .collect();
    builds.sort_by(|a, b| b.0.cmp(&a.0));
    builds
        .into_iter()
        .flat_map(|(n, dir)| {
            [
                "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
                "chrome-mac/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
                "chrome-mac/Chromium.app/Contents/MacOS/Chromium",
            ]
            .into_iter()
            .map(move |rel| BrowserBinary { path: dir.join(rel), label: format!("Chromium (Playwright {n})") })
        })
        .collect()
}

/// Primeiro candidato executavel. `preferred` (configuracao) tem prioridade.
pub fn detect_binary(home: &Path, preferred: Option<&Path>) -> Option<BrowserBinary> {
    if let Some(p) = preferred {
        if is_executable(p) {
            return Some(BrowserBinary { path: p.to_path_buf(), label: p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default() });
        }
    }
    candidates(home).into_iter().find(|c| is_executable(&c.path))
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Opcoes de lancamento.
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub headless: bool,
    pub executable: Option<PathBuf>,
}

/// Resultado de `open`: pagina carregada e sua representacao em texto.
#[derive(Debug, Clone)]
pub struct PageSnapshot {
    pub url: String,
    pub title: String,
    pub loaded: bool,
    pub text: String,
}

impl PageSnapshot {
    pub fn render(&self) -> String {
        let mut s = format!("Pagina: {}\nURL: {}\n", self.title, self.url);
        if !self.loaded {
            s.push_str("(a pagina nao terminou de carregar no tempo limite; conteudo parcial)\n");
        }
        s.push('\n');
        s.push_str(&self.text);
        s
    }
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

/// Um processo de navegador vivo, ligado a um DOT.
pub struct Browser {
    pub binary: BrowserBinary,
    pub pid: u32,
    child: Mutex<Option<tokio::process::Child>>,
    writer: tokio::sync::Mutex<pipe::Sender>,
    pending: Pending,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    /// Eventos CDP (`{method, params, sessionId}`) para quem estiver assistindo.
    events: broadcast::Sender<Value>,
    /// Screencast ligado: ao recriar a sessao da aba, religa.
    screencast: AtomicBool,
    /// Sessao CDP da aba de trabalho (uma por navegador neste spike).
    session: tokio::sync::Mutex<Option<String>>,
    /// Serializa operacoes de pagina.
    op: tokio::sync::Mutex<()>,
}

impl Browser {
    /// Inicia o navegador com perfil proprio e abre o canal CDP.
    pub async fn launch(binary: BrowserBinary, profile_dir: &Path, home: &Path, opts: &LaunchOptions) -> DotsResult<Self> {
        std::fs::create_dir_all(profile_dir)?;
        let (to_child_r, to_child_w) = raw_pipe()?;
        let (from_child_r, from_child_w) = raw_pipe()?;

        let mut cmd = tokio::process::Command::new(&binary.path);
        cmd.arg("--remote-debugging-pipe")
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-sync")
            .arg("--disable-background-networking")
            .arg("--disable-component-update")
            .arg("--disable-features=Translate,MediaRouter")
            .arg("--hide-scrollbars")
            .arg("--window-size=1280,900")
            .arg("--lang=pt-BR");
        if opts.headless {
            cmd.arg("--headless");
        }
        cmd.arg("about:blank");
        cmd.env_clear()
            .env("HOME", home)
            .env("USER", current_username())
            .env("LOGNAME", current_username())
            .env("TMPDIR", std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into()))
            .env("LANG", "pt_BR.UTF-8")
            .env("PATH", "/usr/bin:/bin")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .kill_on_drop(true);
        // No filho: as pontas do pipe viram os descritores 3 e 4 (somente chamadas async-signal-safe).
        unsafe {
            cmd.pre_exec(move || {
                let r = libc::fcntl(to_child_r, libc::F_DUPFD, 10);
                let w = libc::fcntl(from_child_w, libc::F_DUPFD, 10);
                if r < 0 || w < 0 || libc::dup2(r, 3) < 0 || libc::dup2(w, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd.spawn().map_err(|e| DotsError::Io(format!("falha ao iniciar {}: {e}", binary.path.display())))?;
        unsafe {
            libc::close(to_child_r);
            libc::close(from_child_w);
        }
        let pid = child.id().ok_or_else(|| DotsError::Io("navegador sem pid".into()))?;
        let writer = pipe::Sender::from_owned_fd(unsafe { OwnedFd::from_raw_fd(to_child_w) })?;
        let reader = pipe::Receiver::from_owned_fd(unsafe { OwnedFd::from_raw_fd(from_child_r) })?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let (events, _) = broadcast::channel(64);
        tokio::spawn(read_loop(reader, pending.clone(), alive.clone(), events.clone(), pid));

        let b = Self {
            binary,
            pid,
            child: Mutex::new(Some(child)),
            writer: tokio::sync::Mutex::new(writer),
            pending,
            next_id: AtomicU64::new(1),
            alive,
            events,
            screencast: AtomicBool::new(false),
            session: tokio::sync::Mutex::new(None),
            op: tokio::sync::Mutex::new(()),
        };
        // Primeiro contato: confirma que o canal responde.
        let v = b.call(None, "Browser.getVersion", json!({})).await?;
        tracing::info!(pid, binary = %b.binary.label, product = v.get("product").and_then(|p| p.as_str()).unwrap_or("?"), profile = %profile_dir.display(), "navegador iniciado");
        Ok(b)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Chamada CDP (`sessionId` opcional) com timeout.
    pub async fn call(&self, session: Option<&str>, method: &str, params: Value) -> DotsResult<Value> {
        if !self.is_alive() {
            return Err(DotsError::Io("navegador encerrado".into()));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = Value::String(s.to_string());
        }
        let mut bytes = msg.to_string().into_bytes();
        bytes.push(0);
        {
            let mut w = self.writer.lock().await;
            if let Err(e) = w.write_all(&bytes).await {
                self.pending.lock().unwrap().remove(&id);
                return Err(DotsError::Io(format!("escrita CDP ({method}): {e}")));
            }
        }
        let resp = match tokio::time::timeout(CALL_TIMEOUT, rx).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => return Err(DotsError::Io(format!("navegador encerrou durante {method}"))),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                return Err(DotsError::Io(format!("tempo esgotado em {method}")));
            }
        };
        if let Some(err) = resp.get("error") {
            let m = err.get("message").and_then(|m| m.as_str()).unwrap_or("erro CDP");
            return Err(DotsError::Io(format!("{method}: {m}")));
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Sessao da aba de trabalho (cria a aba na primeira vez).
    async fn page_session(&self) -> DotsResult<String> {
        let mut guard = self.session.lock().await;
        if let Some(s) = guard.as_ref() {
            // Sessao ainda valida?
            if self.call(Some(s), "Runtime.evaluate", json!({ "expression": "1", "returnByValue": true })).await.is_ok() {
                return Ok(s.clone());
            }
            *guard = None;
        }
        let targets = self.call(None, "Target.getTargets", json!({})).await?;
        let existing = targets
            .get("targetInfos")
            .and_then(|t| t.as_array())
            .and_then(|arr| arr.iter().find(|t| t.get("type").and_then(|x| x.as_str()) == Some("page")))
            .and_then(|t| t.get("targetId").and_then(|x| x.as_str()).map(String::from));
        let target_id = match existing {
            Some(t) => t,
            None => {
                let created = self.call(None, "Target.createTarget", json!({ "url": "about:blank" })).await?;
                created.get("targetId").and_then(|x| x.as_str()).map(String::from).ok_or_else(|| DotsError::Io("Target.createTarget sem targetId".into()))?
            }
        };
        let attached = self.call(None, "Target.attachToTarget", json!({ "targetId": target_id, "flatten": true })).await?;
        let sid = attached.get("sessionId").and_then(|x| x.as_str()).map(String::from).ok_or_else(|| DotsError::Io("Target.attachToTarget sem sessionId".into()))?;
        self.call(Some(&sid), "Page.enable", json!({})).await?;
        self.call(Some(&sid), "Runtime.enable", json!({})).await?;
        if self.screencast.load(Ordering::SeqCst) {
            let _ = self.call(Some(&sid), "Page.startScreencast", serde_json::from_str(SCREENCAST_PARAMS).unwrap()).await;
        }
        *guard = Some(sid.clone());
        Ok(sid)
    }

    /// Assina os eventos CDP (quadros do screencast chegam como `Page.screencastFrame`).
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    /// Liga o screencast da aba de trabalho (cria a aba se preciso).
    pub async fn start_screencast(&self) -> DotsResult<()> {
        self.screencast.store(true, Ordering::SeqCst);
        let sid = self.page_session().await?;
        self.call(Some(&sid), "Page.startScreencast", serde_json::from_str(SCREENCAST_PARAMS).unwrap()).await?;
        Ok(())
    }

    pub async fn stop_screencast(&self) {
        self.screencast.store(false, Ordering::SeqCst);
        let sid = self.session.lock().await.clone();
        if let Some(sid) = sid {
            let _ = self.call(Some(&sid), "Page.stopScreencast", json!({})).await;
        }
    }

    /// Confirma o recebimento de um quadro (o Chrome so manda o proximo depois disso).
    pub async fn ack_frame(&self, cdp_session: &str, frame_session: i64) {
        let _ = self.call(Some(cdp_session), "Page.screencastFrameAck", json!({ "sessionId": frame_session })).await;
    }

    /// URL e titulo atuais da aba de trabalho (sem navegar).
    pub async fn page_info(&self) -> DotsResult<(String, String)> {
        let sid = self.page_session().await?;
        let meta = self
            .call(Some(&sid), "Runtime.evaluate", json!({ "expression": "JSON.stringify({t: document.title, u: location.href})", "returnByValue": true }))
            .await?;
        let meta: Value = meta.pointer("/result/value").and_then(|x| x.as_str()).and_then(|s| serde_json::from_str(s).ok()).unwrap_or(json!({}));
        Ok((meta.get("u").and_then(|x| x.as_str()).unwrap_or("").to_string(), meta.get("t").and_then(|x| x.as_str()).unwrap_or("").to_string()))
    }

    /// Navega ate `url`, espera o carregamento e devolve a pagina em texto (arvore de acessibilidade).
    pub async fn open(&self, url: &str) -> DotsResult<PageSnapshot> {
        let _op = self.op.lock().await;
        let sid = self.page_session().await?;
        let nav = self.call(Some(&sid), "Page.navigate", json!({ "url": url })).await?;
        if let Some(err) = nav.get("errorText").and_then(|e| e.as_str()) {
            return Err(DotsError::Io(format!("navegacao falhou: {err}")));
        }
        let loaded = self.wait_loaded(&sid).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        self.snapshot_in(&sid, loaded).await
    }

    async fn wait_loaded(&self, sid: &str) -> bool {
        let start = tokio::time::Instant::now();
        while start.elapsed() < LOAD_TIMEOUT {
            let r = self.call(Some(sid), "Runtime.evaluate", json!({ "expression": "document.readyState", "returnByValue": true })).await;
            if let Ok(v) = r {
                if v.pointer("/result/value").and_then(|x| x.as_str()) == Some("complete") {
                    return true;
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        false
    }

    async fn snapshot_in(&self, sid: &str, loaded: bool) -> DotsResult<PageSnapshot> {
        let meta = self
            .call(Some(sid), "Runtime.evaluate", json!({ "expression": "JSON.stringify({t: document.title, u: location.href})", "returnByValue": true }))
            .await?;
        let meta: Value = meta.pointer("/result/value").and_then(|x| x.as_str()).and_then(|s| serde_json::from_str(s).ok()).unwrap_or(json!({}));
        let title = meta.get("t").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let url = meta.get("u").and_then(|x| x.as_str()).unwrap_or("").to_string();

        let mut text = match self.call(Some(sid), "Accessibility.getFullAXTree", json!({})).await {
            Ok(tree) => render_ax_tree(&tree),
            Err(e) => {
                tracing::warn!(error = %e, "arvore de acessibilidade indisponivel; usando texto da pagina");
                String::new()
            }
        };
        if text.trim().is_empty() {
            let body = self
                .call(Some(sid), "Runtime.evaluate", json!({ "expression": "document.body ? document.body.innerText : ''", "returnByValue": true }))
                .await?;
            text = body.pointer("/result/value").and_then(|x| x.as_str()).unwrap_or("").to_string();
        }
        Ok(PageSnapshot { url, title, loaded, text })
    }

    /// Encerra o processo (SIGTERM no grupo; SIGKILL se nao sair em 3 s).
    pub async fn close(&self) {
        self.alive.store(false, Ordering::SeqCst);
        unsafe { libc::killpg(self.pid as i32, libc::SIGTERM) };
        let child = self.child.lock().unwrap().take();
        if let Some(mut c) = child {
            if tokio::time::timeout(Duration::from_secs(3), c.wait()).await.is_err() {
                unsafe { libc::killpg(self.pid as i32, libc::SIGKILL) };
                let _ = c.wait().await;
            }
        }
        tracing::info!(pid = self.pid, "navegador encerrado");
    }
}

/// Le mensagens `\0`-terminadas: respostas vao aos chamadores, eventos aos assinantes.
async fn read_loop(mut reader: pipe::Receiver, pending: Pending, alive: Arc<AtomicBool>, events: broadcast::Sender<Value>, pid: u32) {
    let mut buf = Vec::with_capacity(64 * 1024);
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                while let Some(pos) = buf.iter().position(|b| *b == 0) {
                    let msg: Vec<u8> = buf.drain(..=pos).collect();
                    let Ok(v) = serde_json::from_slice::<Value>(&msg[..msg.len() - 1]) else { continue };
                    if let Some(id) = v.get("id").and_then(|i| i.as_u64()) {
                        if let Some(tx) = pending.lock().unwrap().remove(&id) {
                            let _ = tx.send(v);
                        }
                    } else {
                        tracing::trace!(pid, method = v.get("method").and_then(|m| m.as_str()).unwrap_or("?"), "evento CDP");
                        let _ = events.send(v);
                    }
                }
            }
        }
    }
    alive.store(false, Ordering::SeqCst);
    pending.lock().unwrap().clear();
    tracing::info!(pid, "canal CDP fechado");
}

/// Converte a arvore de acessibilidade em linhas `- role "nome" [ref=<backendDOMNodeId>] (props)`.
pub fn render_ax_tree(tree: &Value) -> String {
    let Some(nodes) = tree.get("nodes").and_then(|n| n.as_array()) else { return String::new() };
    let by_id: HashMap<&str, &Value> = nodes.iter().filter_map(|n| n.get("nodeId").and_then(|i| i.as_str()).map(|i| (i, n))).collect();
    let root = nodes.iter().find(|n| n.get("parentId").is_none()).or_else(|| nodes.first());
    let Some(root) = root else { return String::new() };
    let mut out = Vec::new();
    let mut count = 0usize;
    let mut stack: Vec<(&Value, usize)> = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        if count >= MAX_SNAPSHOT_NODES {
            out.push("…[arvore truncada]".to_string());
            break;
        }
        let ignored = node.get("ignored").and_then(|i| i.as_bool()).unwrap_or(false);
        let role = node.pointer("/role/value").and_then(|r| r.as_str()).unwrap_or("");
        let name = node.pointer("/name/value").and_then(|r| r.as_str()).unwrap_or("").trim();
        let mut child_depth = depth;
        if !ignored && is_relevant(role, name) {
            let mut line = format!("{}- {role}", "  ".repeat(depth));
            if !name.is_empty() {
                line.push_str(&format!(" \"{}\"", name.chars().take(200).collect::<String>()));
            }
            if let Some(b) = node.get("backendDOMNodeId").and_then(|b| b.as_u64()) {
                line.push_str(&format!(" [ref={b}]"));
            }
            if let Some(props) = node.get("properties").and_then(|p| p.as_array()) {
                for p in props {
                    let pname = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    if matches!(pname, "url" | "checked" | "expanded" | "selected" | "disabled" | "required" | "level") {
                        let pv = p.pointer("/value/value").map(|v| v.to_string().trim_matches('"').to_string()).unwrap_or_default();
                        if pv != "false" && !pv.is_empty() {
                            line.push_str(&format!(" {pname}={pv}"));
                        }
                    }
                }
            }
            if let Some(v) = node.pointer("/value/value").and_then(|v| v.as_str()) {
                if !v.is_empty() && role != "StaticText" {
                    line.push_str(&format!(" value=\"{}\"", v.chars().take(100).collect::<String>()));
                }
            }
            out.push(line);
            count += 1;
            child_depth = depth + 1;
        }
        if let Some(children) = node.get("childIds").and_then(|c| c.as_array()) {
            for c in children.iter().rev() {
                if let Some(child) = c.as_str().and_then(|id| by_id.get(id)) {
                    stack.push((child, child_depth));
                }
            }
        }
    }
    out.join("\n")
}

fn is_relevant(role: &str, name: &str) -> bool {
    match role {
        "" | "none" | "generic" | "InlineTextBox" | "LineBreak" | "presentation" | "LayoutTableCell" | "LayoutTableRow" | "LayoutTable" => false,
        "link" | "button" | "textbox" | "searchbox" | "combobox" | "checkbox" | "radio" | "heading" | "menuitem" | "tab" | "option" | "slider" | "spinbutton" | "switch" | "image" | "img" => true,
        _ => !name.is_empty(),
    }
}

fn raw_pipe() -> std::io::Result<(i32, i32)> {
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    for fd in fds {
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    Ok((fds[0], fds[1]))
}

fn current_username() -> String {
    if let Ok(u) = std::env::var("USER") {
        if !u.is_empty() {
            return u;
        }
    }
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return "user".into();
        }
        std::ffi::CStr::from_ptr((*pw).pw_name).to_string_lossy().to_string()
    }
}

/// Um quadro do screencast (JPEG em base64) e o tamanho da pagina.
#[derive(Debug, Clone)]
pub struct Frame {
    pub data: String,
    pub width: f64,
    pub height: f64,
}

/// Um navegador por DOT, criado sob demanda e mantido vivo ate o encerramento do runtime.
pub struct BrowserPool {
    home: PathBuf,
    browsers_dir: PathBuf,
    inner: tokio::sync::Mutex<HashMap<String, Arc<Browser>>>,
    /// Quem esta assistindo (dot_id -> prazo do ultimo `watch`); a tarefa de quadros para quando o prazo vence.
    watchers: Arc<Mutex<HashMap<String, tokio::time::Instant>>>,
}

/// Quanto tempo um `watch` vale sem renovacao (a interface renova a cada poucos segundos).
pub const WATCH_TTL: Duration = Duration::from_secs(15);

impl BrowserPool {
    pub fn new(home: PathBuf, browsers_dir: PathBuf) -> Self {
        Self { home, browsers_dir, inner: tokio::sync::Mutex::new(HashMap::new()), watchers: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Navegador do DOT (inicia se nao existir ou se o anterior morreu). Perfil em `<browsers>/<dot_id>`.
    pub async fn get(&self, dot_id: &str, opts: &LaunchOptions) -> DotsResult<Arc<Browser>> {
        let mut map = self.inner.lock().await;
        if let Some(b) = map.get(dot_id) {
            if b.is_alive() {
                return Ok(b.clone());
            }
            map.remove(dot_id);
        }
        let binary = detect_binary(&self.home, opts.executable.as_deref())
            .ok_or_else(|| DotsError::NotFound("navegador (Google Chrome ou Chromium)".into()))?;
        let profile = self.browsers_dir.join(dot_id);
        let b = Arc::new(Browser::launch(binary, &profile, &self.home, opts).await?);
        map.insert(dot_id.to_string(), b.clone());
        Ok(b)
    }

    /// Navegador do DOT se estiver vivo (nao inicia).
    pub async fn running(&self, dot_id: &str) -> Option<Arc<Browser>> {
        self.inner.lock().await.get(dot_id).filter(|b| b.is_alive()).cloned()
    }

    pub async fn close(&self, dot_id: &str) {
        self.watchers.lock().unwrap().remove(dot_id);
        if let Some(b) = self.inner.lock().await.remove(dot_id) {
            b.close().await;
        }
    }

    /// Comeca (ou renova) a transmissao de quadros do navegador do DOT para `on_frame`.
    /// Retorna `true` se uma transmissao nova foi iniciada.
    pub async fn watch(&self, dot_id: &str, opts: &LaunchOptions, on_frame: impl Fn(Frame) + Send + Sync + 'static) -> DotsResult<bool> {
        let renewed = {
            let mut w = self.watchers.lock().unwrap();
            let existed = w.contains_key(dot_id);
            w.insert(dot_id.to_string(), tokio::time::Instant::now() + WATCH_TTL);
            existed
        };
        if renewed {
            return Ok(false);
        }
        let browser = match self.get(dot_id, opts).await {
            Ok(b) => b,
            Err(e) => {
                self.watchers.lock().unwrap().remove(dot_id);
                return Err(e);
            }
        };
        let mut rx = browser.subscribe();
        if let Err(e) = browser.start_screencast().await {
            self.watchers.lock().unwrap().remove(dot_id);
            return Err(e);
        }
        let watchers = self.watchers.clone();
        let id = dot_id.to_string();
        tokio::spawn(async move {
            loop {
                let expired = watchers.lock().unwrap().get(&id).map(|t| *t < tokio::time::Instant::now()).unwrap_or(true);
                if expired || !browser.is_alive() {
                    break;
                }
                match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
                    Ok(Ok(ev)) => {
                        if ev.get("method").and_then(|m| m.as_str()) != Some("Page.screencastFrame") {
                            continue;
                        }
                        let cdp_session = ev.get("sessionId").and_then(|s| s.as_str()).unwrap_or("").to_string();
                        let p = ev.get("params").cloned().unwrap_or(Value::Null);
                        if let Some(fs) = p.get("sessionId").and_then(|s| s.as_i64()) {
                            browser.ack_frame(&cdp_session, fs).await;
                        }
                        on_frame(Frame {
                            data: p.get("data").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                            width: p.pointer("/metadata/deviceWidth").and_then(|v| v.as_f64()).unwrap_or(0.0),
                            height: p.pointer("/metadata/deviceHeight").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        });
                    }
                    Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                    Ok(Err(_)) => break,
                    Err(_) => continue,
                }
            }
            watchers.lock().unwrap().remove(&id);
            browser.stop_screencast().await;
            tracing::info!(dot = %id, "transmissao do navegador encerrada");
        });
        Ok(true)
    }

    /// Para de assistir (a tarefa de quadros encerra no proximo ciclo).
    pub fn unwatch(&self, dot_id: &str) {
        self.watchers.lock().unwrap().remove(dot_id);
    }

    pub fn is_watched(&self, dot_id: &str) -> bool {
        self.watchers.lock().unwrap().get(dot_id).map(|t| *t >= tokio::time::Instant::now()).unwrap_or(false)
    }

    pub async fn shutdown_all(&self) {
        let all: Vec<Arc<Browser>> = self.inner.lock().await.drain().map(|(_, b)| b).collect();
        for b in all {
            b.close().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_ax_tree_filters_and_indents() {
        let tree = json!({ "nodes": [
            { "nodeId": "1", "role": { "value": "RootWebArea" }, "name": { "value": "Titulo" }, "childIds": ["2", "5"], "backendDOMNodeId": 1 },
            { "nodeId": "2", "parentId": "1", "role": { "value": "generic" }, "ignored": false, "childIds": ["3", "4"] },
            { "nodeId": "3", "parentId": "2", "role": { "value": "heading" }, "name": { "value": "Ola" }, "backendDOMNodeId": 7, "properties": [{ "name": "level", "value": { "type": "integer", "value": 1 } }] },
            { "nodeId": "4", "parentId": "2", "role": { "value": "link" }, "name": { "value": "Entrar" }, "backendDOMNodeId": 9, "properties": [{ "name": "url", "value": { "type": "string", "value": "https://x/login" } }] },
            { "nodeId": "5", "parentId": "1", "role": { "value": "StaticText" }, "name": { "value": "" }, "ignored": true }
        ]});
        let s = render_ax_tree(&tree);
        assert_eq!(s, "- RootWebArea \"Titulo\" [ref=1]\n  - heading \"Ola\" [ref=7] level=1\n  - link \"Entrar\" [ref=9] url=https://x/login");
    }

    #[test]
    fn candidates_start_with_chrome_and_include_playwright_when_present() {
        let home = tempfile::tempdir().unwrap();
        let pw = home.path().join("Library/Caches/ms-playwright");
        std::fs::create_dir_all(pw.join("chromium-1187")).unwrap();
        std::fs::create_dir_all(pw.join("chromium-1243")).unwrap();
        let c = candidates(home.path());
        assert!(c[0].path.ends_with("Google Chrome.app/Contents/MacOS/Google Chrome"));
        let labels: Vec<&str> = c.iter().map(|b| b.label.as_str()).collect();
        let i1243 = labels.iter().position(|l| *l == "Chromium (Playwright 1243)").unwrap();
        let i1187 = labels.iter().position(|l| *l == "Chromium (Playwright 1187)").unwrap();
        assert!(i1243 < i1187, "o build mais novo vem primeiro");
    }

    /// Prova real: inicia o navegador detectado na maquina, abre uma pagina e le o conteudo.
    /// Ignorado automaticamente onde nao ha Chrome/Chromium.
    #[tokio::test]
    async fn launches_real_browser_with_isolated_profile() {
        let home = crate::paths::home_dir().unwrap();
        let Some(binary) = detect_binary(&home, None) else {
            eprintln!("sem navegador na maquina; teste ignorado");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("dot-a");
        let b = Browser::launch(binary, &profile, &home, &LaunchOptions { headless: true, executable: None }).await.unwrap();
        assert!(b.is_alive());
        let page = b.open("data:text/html,<title>Spike DOTS</title><h1>Ola DOT</h1><a href='https://example.com/x'>Seguir</a><input aria-label='Busca'>").await.unwrap();
        assert_eq!(page.title, "Spike DOTS");
        assert!(page.loaded);
        assert!(page.text.contains("heading \"Ola DOT\""), "{}", page.text);
        assert!(page.text.contains("link \"Seguir\""), "{}", page.text);
        assert!(page.text.contains("textbox \"Busca\""), "{}", page.text);
        // Segunda navegacao reutiliza a aba.
        let page2 = b.open("data:text/html,<title>Dois</title><p>segunda</p>").await.unwrap();
        assert_eq!(page2.title, "Dois");
        assert!(profile.join("Default").exists() || profile.read_dir().unwrap().next().is_some(), "perfil criado no diretorio do DOT");
        // Screencast: ao menos um quadro JPEG chega apos ligar.
        let mut rx = b.subscribe();
        b.start_screencast().await.unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let ev = rx.recv().await.unwrap();
                if ev.get("method").and_then(|m| m.as_str()) == Some("Page.screencastFrame") {
                    break ev;
                }
            }
        })
        .await
        .expect("quadro do screencast");
        let data = frame.pointer("/params/data").and_then(|d| d.as_str()).unwrap();
        assert!(data.starts_with("/9j/"), "JPEG em base64");
        assert!(frame.pointer("/params/metadata/deviceWidth").and_then(|v| v.as_f64()).unwrap() > 0.0);
        b.stop_screencast().await;
        let (url, title) = b.page_info().await.unwrap();
        assert_eq!(title, "Dois");
        assert!(url.starts_with("data:"));
        b.close().await;
        assert!(!b.is_alive());
    }
}
