//! Navegador por DOT: um Chrome/Chromium proprio, com perfil (`--user-data-dir`) isolado por DOT,
//! controlado pelo runtime via CDP (Chrome DevTools Protocol) sobre `--remote-debugging-pipe`.
//!
//! Sem dependencias externas: o CDP por pipe e JSON terminado em `\0` nos descritores 3 (entrada
//! do navegador) e 4 (saida). O runtime fala direto com o processo; nenhuma porta e aberta.

use std::collections::{HashMap, VecDeque};
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dots_core::{DotsError, DotsResult};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
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
    let mut out = Vec::new();
    #[cfg(target_os = "macos")]
    out.extend([
        BrowserBinary { path: PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"), label: "Google Chrome".into() },
        BrowserBinary { path: home.join("Applications/Google Chrome.app/Contents/MacOS/Google Chrome"), label: "Google Chrome".into() },
    ]);
    #[cfg(target_os = "linux")]
    out.extend([
        ("/usr/bin/google-chrome-stable", "Google Chrome"),
        ("/usr/bin/google-chrome", "Google Chrome"),
        ("/opt/google/chrome/chrome", "Google Chrome"),
        ("/usr/bin/chromium", "Chromium"),
        ("/usr/bin/chromium-browser", "Chromium"),
        ("/snap/bin/chromium", "Chromium"),
        ("/usr/bin/brave-browser", "Brave"),
        ("/usr/bin/microsoft-edge", "Microsoft Edge"),
    ].map(|(path, label)| BrowserBinary { path: PathBuf::from(path), label: label.into() }));
    out.extend(playwright_chromium(home));
    #[cfg(target_os = "macos")]
    {
        out.push(BrowserBinary { path: PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"), label: "Chromium".into() });
        out.push(BrowserBinary { path: PathBuf::from("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"), label: "Brave".into() });
        out.push(BrowserBinary { path: PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"), label: "Microsoft Edge".into() });
    }
    out
}

/// Chromium baixado pelo Playwright, o mais novo primeiro.
fn playwright_chromium(home: &Path) -> Vec<BrowserBinary> {
    #[cfg(target_os = "macos")]
    let base = home.join("Library/Caches/ms-playwright");
    #[cfg(target_os = "linux")]
    let base = home.join(".cache/ms-playwright");
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
            #[cfg(target_os = "macos")]
            let paths = ["chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing", "chrome-mac/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing", "chrome-mac/Chromium.app/Contents/MacOS/Chromium"];
            #[cfg(target_os = "linux")]
            let paths = ["chrome-linux/chrome", "chrome-linux64/chrome"];
            paths
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
/// Quantas linhas do stderr do navegador ficam guardadas para o diagnostico.
const STDERR_TAIL_LINES: usize = 40;

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
    /// Ultimas linhas do stderr do navegador: o motivo quando ele morre antes de responder.
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    /// Dialogo nativo aberto na pagina (`alert`/`confirm`/`prompt`): enquanto existir, a pagina nao roda JavaScript.
    dialog: Arc<Mutex<Option<String>>>,
}

/// O Chrome morreu porque a sandbox nao conseguiu criar namespaces (tipico de container Docker sem `SYS_ADMIN`).
/// Olha o stderr guardado na mensagem de erro do lancamento.
pub fn sandbox_unavailable(err: &str) -> bool {
    err.contains("Failed to move to new namespace") || err.contains("Zygote process exited prematurely") || err.contains("No usable sandbox")
}

impl Browser {
    /// Inicia o navegador com perfil proprio e abre o canal CDP.
    ///
    /// Primeiro com a sandbox do Chrome. Em containers (Docker sem `SYS_ADMIN`, seccomp padrao) o kernel nega criar
    /// namespaces, o zygote morre e o navegador aborta antes de responder; nesse caso, e so nele, relanca com `--no-sandbox`.
    pub async fn launch(binary: BrowserBinary, profile_dir: &Path, home: &Path, opts: &LaunchOptions) -> DotsResult<Self> {
        match Self::launch_with(binary.clone(), profile_dir, home, opts, false).await {
            Ok(b) => Ok(b),
            Err(e) if sandbox_unavailable(&e.to_string()) => {
                tracing::warn!(binary = %binary.path.display(), "sandbox do navegador indisponivel neste sistema (namespaces negados); iniciando sem sandbox");
                Self::launch_with(binary, profile_dir, home, opts, true).await
            }
            Err(e) => Err(e),
        }
    }

    async fn launch_with(binary: BrowserBinary, profile_dir: &Path, home: &Path, opts: &LaunchOptions, no_sandbox: bool) -> DotsResult<Self> {
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
        if no_sandbox {
            cmd.arg("--no-sandbox");
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
            .stderr(std::process::Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        // No Linux o navegador com janela precisa achar a sessao grafica (X11 ou Wayland) e o D-Bus do usuario.
        if cfg!(target_os = "linux") {
            for key in ["DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "XDG_RUNTIME_DIR", "XDG_SESSION_TYPE", "DBUS_SESSION_BUS_ADDRESS"] {
                if let Ok(v) = std::env::var(key) {
                    cmd.env(key, v);
                }
            }
        }
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
        let mut child = cmd.spawn().map_err(|e| DotsError::Io(format!("falha ao iniciar {}: {e}", binary.path.display())))?;
        unsafe {
            libc::close(to_child_r);
            libc::close(from_child_w);
        }
        let stderr_tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        if let Some(stderr) = child.stderr.take() {
            let tail = stderr_tail.clone();
            tokio::spawn(async move {
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut t = tail.lock().unwrap();
                    if t.len() >= STDERR_TAIL_LINES {
                        t.pop_front();
                    }
                    t.push_back(line);
                }
            });
        }
        let pid = child.id().ok_or_else(|| DotsError::Io("navegador sem pid".into()))?;
        let writer = pipe::Sender::from_owned_fd(unsafe { OwnedFd::from_raw_fd(to_child_w) })?;
        let reader = pipe::Receiver::from_owned_fd(unsafe { OwnedFd::from_raw_fd(from_child_r) })?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let (events, _) = broadcast::channel(64);
        let dialog = Arc::new(Mutex::new(None));
        tokio::spawn(read_loop(reader, pending.clone(), alive.clone(), events.clone(), dialog.clone(), pid));

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
            stderr_tail,
            dialog,
        };
        // Primeiro contato: confirma que o canal responde. Se o navegador morreu, diz por que (saida e stderr).
        let v = match b.call(None, "Browser.getVersion", json!({})).await {
            Ok(v) => v,
            Err(e) => {
                tokio::time::sleep(Duration::from_millis(300)).await;
                let status = b.child.lock().unwrap().as_mut().and_then(|c| c.try_wait().ok().flatten()).map(|st| st.to_string()).unwrap_or_else(|| "ainda em execucao".into());
                let tail = b.stderr_lines();
                tracing::warn!(binary = %b.binary.path.display(), %status, stderr = %tail, "navegador nao respondeu ao primeiro contato");
                return Err(DotsError::Io(format!("{e} ({}; {status}){}", b.binary.path.display(), if tail.is_empty() { String::new() } else { format!(": {tail}") })));
            }
        };
        tracing::info!(pid, binary = %b.binary.label, product = v.get("product").and_then(|p| p.as_str()).unwrap_or("?"), profile = %profile_dir.display(), no_sandbox, "navegador iniciado");
        Ok(b)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Ultimas linhas do stderr do navegador, numa linha so.
    pub fn stderr_lines(&self) -> String {
        self.stderr_tail.lock().unwrap().iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" | ")
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
        self.call(Some(&sid), "DOM.enable", json!({})).await?;
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
        match self.call(Some(&sid), "Page.startScreencast", serde_json::from_str(SCREENCAST_PARAMS).unwrap()).await {
            Ok(_) => Ok(()),
            // Painel reaberto antes do Chrome encerrar o screencast anterior: segue usando o que ja esta ativo.
            Err(e) if e.message().contains("already active") => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Define um cookie para `url` (login de um app conectado, ex.: sessao do Flexi) antes de navegar.
    pub async fn set_cookie(&self, name: &str, value: &str, url: &str) -> DotsResult<()> {
        let sid = self.page_session().await?;
        self.call(Some(&sid), "Network.setCookie", json!({ "name": name, "value": value, "url": url })).await?;
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
            if self.dialog().is_some() {
                return true;
            }
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
        if let Some(d) = self.dialog() {
            return Ok(PageSnapshot { url: self.current_url().await.unwrap_or_default(), title: String::new(), loaded: true, text: dialog_note(&d) });
        }
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

    /// Dialogo nativo aberto na pagina, como `confirm: "Deseja excluir?"`.
    pub fn dialog(&self) -> Option<String> {
        self.dialog.lock().unwrap().clone()
    }

    /// URL da aba de trabalho sem rodar JavaScript na pagina (funciona com dialogo aberto).
    pub async fn current_url(&self) -> DotsResult<String> {
        let targets = self.call(None, "Target.getTargets", json!({})).await?;
        Ok(targets
            .get("targetInfos")
            .and_then(|t| t.as_array())
            .and_then(|arr| arr.iter().find(|t| t.get("type").and_then(|x| x.as_str()) == Some("page")))
            .and_then(|t| t.get("url").and_then(|u| u.as_str()))
            .unwrap_or("")
            .to_string())
    }

    /// Papel e nome de um elemento da ultima leitura (`[ref=N]`), ex.: `button "Excluir anuncio"`.
    pub async fn describe(&self, backend: u64) -> DotsResult<String> {
        let sid = self.page_session().await?;
        let ax = self.call(Some(&sid), "Accessibility.getPartialAXTree", json!({ "backendNodeId": backend, "fetchRelatives": false })).await.map_err(|e| ref_error(backend, e))?;
        let node = ax.get("nodes").and_then(|n| n.as_array()).and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
        let role = node.pointer("/role/value").and_then(|r| r.as_str()).unwrap_or("elemento");
        let name = node.pointer("/name/value").and_then(|r| r.as_str()).unwrap_or("").trim();
        Ok(if name.is_empty() { role.to_string() } else { format!("{role} \"{}\"", name.chars().take(120).collect::<String>()) })
    }

    /// Releitura da pagina atual, sem navegar (conteudo que carregou depois, resultado de uma acao).
    pub async fn read(&self) -> DotsResult<PageSnapshot> {
        let _op = self.op.lock().await;
        if let Some(d) = self.dialog() {
            return Ok(PageSnapshot { url: self.current_url().await.unwrap_or_default(), title: String::new(), loaded: true, text: dialog_note(&d) });
        }
        let sid = self.page_session().await?;
        self.snapshot_in(&sid, true).await
    }

    /// Clica no centro do elemento `[ref=N]` como um mouse (rola ate ele antes) e devolve a pagina depois do clique.
    pub async fn click(&self, backend: u64) -> DotsResult<PageSnapshot> {
        let _op = self.op.lock().await;
        self.no_dialog()?;
        let sid = self.page_session().await?;
        let _ = self.call(Some(&sid), "DOM.scrollIntoViewIfNeeded", json!({ "backendNodeId": backend })).await;
        let point = match self.call(Some(&sid), "DOM.getContentQuads", json!({ "backendNodeId": backend })).await {
            Ok(q) => q.get("quads").and_then(|q| q.as_array()).and_then(|a| a.iter().find_map(quad_center)),
            Err(e) if is_stale(&e) => return Err(ref_error(backend, e)),
            Err(_) => None,
        };
        self.until_dialog(async {
            match point {
                Some((x, y)) => {
                    self.call(Some(&sid), "Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": x, "y": y })).await?;
                    self.call(Some(&sid), "Input.dispatchMouseEvent", json!({ "type": "mousePressed", "x": x, "y": y, "button": "left", "buttons": 1, "clickCount": 1 })).await?;
                    self.call(Some(&sid), "Input.dispatchMouseEvent", json!({ "type": "mouseReleased", "x": x, "y": y, "button": "left", "buttons": 0, "clickCount": 1 })).await?;
                }
                // Sem caixa visivel (elemento escondido ou de tamanho zero): clique pelo DOM.
                None => {
                    self.call_on(&sid, backend, "function(){ this.click(); return ''; }", Value::Null).await?;
                }
            }
            Ok(())
        })
        .await?;
        self.settle(&sid).await
    }

    /// Digita `text` no campo `[ref=N]` (foca antes). `clear` apaga o que havia; `submit` tecla Enter no fim.
    pub async fn type_text(&self, backend: u64, text: &str, clear: bool, submit: bool) -> DotsResult<PageSnapshot> {
        let _op = self.op.lock().await;
        self.no_dialog()?;
        let sid = self.page_session().await?;
        let _ = self.call(Some(&sid), "DOM.scrollIntoViewIfNeeded", json!({ "backendNodeId": backend })).await;
        self.call(Some(&sid), "DOM.focus", json!({ "backendNodeId": backend })).await.map_err(|e| ref_error(backend, e))?;
        if clear {
            self.call_on(&sid, backend, SELECT_ALL_JS, Value::Null).await?;
            if text.is_empty() {
                self.key(&sid, "Backspace").await?;
            }
        }
        if !text.is_empty() {
            self.call(Some(&sid), "Input.insertText", json!({ "text": text })).await?;
        }
        if submit {
            self.until_dialog(self.key(&sid, "Enter")).await?;
        }
        self.settle(&sid).await
    }

    /// Tecla na pagina (Enter, Escape, Tab, setas...). Com um dialogo nativo aberto: Enter aceita, Escape cancela.
    pub async fn press(&self, key: &str) -> DotsResult<PageSnapshot> {
        let _op = self.op.lock().await;
        if self.dialog().is_some() {
            let accept = match key {
                "Enter" => true,
                "Escape" => false,
                _ => return Err(DotsError::Validation("ha um dialogo do navegador aberto: use Enter (aceitar) ou Escape (cancelar)".into())),
            };
            // A sessao ja existe (o dialogo veio dela); page_session testaria com JavaScript, que esta parado.
            let sid = self.session.lock().await.clone().ok_or_else(|| DotsError::Io("aba do navegador indisponivel".into()))?;
            self.call(Some(&sid), "Page.handleJavaScriptDialog", json!({ "accept": accept })).await?;
            *self.dialog.lock().unwrap() = None;
            return self.settle(&sid).await;
        }
        if key_def(key).is_none() {
            return Err(DotsError::Validation(format!("tecla desconhecida: {key} (use {})", KEYS.iter().map(|k| k.0).collect::<Vec<_>>().join(", "))));
        }
        let sid = self.page_session().await?;
        self.until_dialog(self.key(&sid, key)).await?;
        self.settle(&sid).await
    }

    /// Escolhe uma opcao (pelo valor ou pelo texto) na lista `<select>` `[ref=N]`.
    pub async fn select(&self, backend: u64, option: &str) -> DotsResult<PageSnapshot> {
        let _op = self.op.lock().await;
        self.no_dialog()?;
        let sid = self.page_session().await?;
        let problem = self.call_on(&sid, backend, SELECT_JS, json!(option)).await?;
        if !problem.is_empty() {
            return Err(DotsError::Validation(problem));
        }
        self.settle(&sid).await
    }

    fn no_dialog(&self) -> DotsResult<()> {
        match self.dialog() {
            Some(d) => Err(DotsError::Validation(dialog_note(&d))),
            None => Ok(()),
        }
    }

    /// Roda a acao ate o fim ou ate abrir um dialogo nativo (o Chrome so responde a acao depois que o dialogo fecha).
    async fn until_dialog(&self, action: impl std::future::Future<Output = DotsResult<()>>) -> DotsResult<()> {
        tokio::pin!(action);
        loop {
            tokio::select! {
                r = &mut action => return r,
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if self.dialog().is_some() {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Depois de uma acao: espera a pagina reagir (e carregar, se navegou) e devolve a leitura nova.
    async fn settle(&self, sid: &str) -> DotsResult<PageSnapshot> {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let loaded = self.wait_loaded(sid).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        self.snapshot_in(sid, loaded).await
    }

    async fn key(&self, sid: &str, key: &str) -> DotsResult<()> {
        let (name, code, vk, text) = key_def(key).ok_or_else(|| DotsError::Validation(format!("tecla desconhecida: {key}")))?;
        let mut down = json!({ "type": if text.is_some() { "keyDown" } else { "rawKeyDown" }, "key": name, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk });
        if let Some(t) = text {
            down["text"] = json!(t);
            down["unmodifiedText"] = json!(t);
        }
        self.call(Some(sid), "Input.dispatchKeyEvent", down).await?;
        self.call(Some(sid), "Input.dispatchKeyEvent", json!({ "type": "keyUp", "key": name, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk })).await?;
        Ok(())
    }

    /// Executa `function` com `this` = elemento `[ref=N]`; devolve o texto que a funcao retornar.
    async fn call_on(&self, sid: &str, backend: u64, function: &str, arg: Value) -> DotsResult<String> {
        let node = self.call(Some(sid), "DOM.resolveNode", json!({ "backendNodeId": backend })).await.map_err(|e| ref_error(backend, e))?;
        let object_id = node.pointer("/object/objectId").and_then(|o| o.as_str()).ok_or_else(|| ref_error(backend, DotsError::Io("sem objectId".into())))?;
        let args = if arg.is_null() { json!([]) } else { json!([{ "value": arg }]) };
        let r = self.call(Some(sid), "Runtime.callFunctionOn", json!({ "functionDeclaration": function, "objectId": object_id, "arguments": args, "returnByValue": true })).await?;
        if let Some(ex) = r.get("exceptionDetails") {
            return Err(DotsError::Io(format!("erro na pagina: {}", ex.pointer("/exception/description").and_then(|d| d.as_str()).unwrap_or("excecao"))));
        }
        Ok(r.pointer("/result/value").and_then(|v| v.as_str()).unwrap_or("").to_string())
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

/// Seleciona o conteudo do campo (o texto digitado em seguida substitui o anterior).
const SELECT_ALL_JS: &str = "function(){ if (typeof this.select === 'function') { this.select(); } else { const r = document.createRange(); r.selectNodeContents(this); const s = getSelection(); s.removeAllRanges(); s.addRange(r); } return ''; }";
/// Escolhe a opcao de um `<select>` pelo valor ou pelo texto, pelo setter nativo (funciona com React) e avisa a pagina.
const SELECT_JS: &str = "function(v){ if (this.tagName !== 'SELECT') return 'o elemento nao e uma lista de opcoes (select): clique nele e depois na opcao'; const norm = (s) => (s || '').trim().toLowerCase(); const opts = [...this.options]; const o = opts.find((x) => x.value === v) || opts.find((x) => norm(x.textContent) === norm(v)); if (!o) return 'opcao nao encontrada; opcoes: ' + opts.map((x) => x.textContent.trim()).join(' | '); Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(this, o.value); this.dispatchEvent(new Event('input', { bubbles: true })); this.dispatchEvent(new Event('change', { bubbles: true })); return ''; }";

/// Teclas aceitas: (nome, code, keyCode, texto inserido).
const KEYS: &[(&str, &str, i64, Option<&str>)] = &[
    ("Enter", "Enter", 13, Some("\r")),
    ("Escape", "Escape", 27, None),
    ("Tab", "Tab", 9, None),
    ("Backspace", "Backspace", 8, None),
    ("Delete", "Delete", 46, None),
    ("Space", "Space", 32, Some(" ")),
    ("ArrowUp", "ArrowUp", 38, None),
    ("ArrowDown", "ArrowDown", 40, None),
    ("ArrowLeft", "ArrowLeft", 37, None),
    ("ArrowRight", "ArrowRight", 39, None),
    ("PageUp", "PageUp", 33, None),
    ("PageDown", "PageDown", 34, None),
    ("Home", "Home", 36, None),
    ("End", "End", 35, None),
];

fn key_def(key: &str) -> Option<(&'static str, &'static str, i64, Option<&'static str>)> {
    KEYS.iter().find(|k| k.0.eq_ignore_ascii_case(key)).map(|k| (if k.0 == "Space" { " " } else { k.0 }, k.1, k.2, k.3))
}

/// Centro de um quad (`[x1,y1,...,x4,y4]`, coordenadas da tela da aba); None se nao tiver area.
fn quad_center(q: &Value) -> Option<(f64, f64)> {
    let p: Vec<f64> = q.as_array()?.iter().filter_map(|v| v.as_f64()).collect();
    if p.len() != 8 {
        return None;
    }
    let (xs, ys) = ([p[0], p[2], p[4], p[6]], [p[1], p[3], p[5], p[7]]);
    let w = xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min);
    let h = ys.iter().cloned().fold(f64::MIN, f64::max) - ys.iter().cloned().fold(f64::MAX, f64::min);
    if w < 1.0 || h < 1.0 {
        return None;
    }
    Some((xs.iter().sum::<f64>() / 4.0, ys.iter().sum::<f64>() / 4.0))
}

fn is_stale(e: &DotsError) -> bool {
    let m = e.message().to_ascii_lowercase();
    m.contains("node with given id") || m.contains("no node") || m.contains("does not belong to the document")
}

/// Erro de um `ref` que nao existe mais (a pagina mudou desde a ultima leitura).
fn ref_error(backend: u64, e: DotsError) -> DotsError {
    if is_stale(&e) {
        DotsError::Validation(format!("o elemento [ref={backend}] nao existe mais na pagina: leia a pagina de novo (dots_browser_read) e use um ref atual"))
    } else {
        e
    }
}

/// Texto devolvido enquanto um dialogo nativo esta aberto.
fn dialog_note(d: &str) -> String {
    format!("Dialogo do navegador aberto ({d}). A pagina fica parada ate responder: dots_browser_press com Enter aceita (OK) e com Escape cancela.")
}

/// Le mensagens `\0`-terminadas: respostas vao aos chamadores, eventos aos assinantes.
async fn read_loop(mut reader: pipe::Receiver, pending: Pending, alive: Arc<AtomicBool>, events: broadcast::Sender<Value>, dialog: Arc<Mutex<Option<String>>>, pid: u32) {
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
                        match v.get("method").and_then(|m| m.as_str()) {
                            Some("Page.javascriptDialogOpening") => {
                                let kind = v.pointer("/params/type").and_then(|t| t.as_str()).unwrap_or("alert");
                                let msg = v.pointer("/params/message").and_then(|t| t.as_str()).unwrap_or("");
                                *dialog.lock().unwrap() = Some(format!("{kind}: \"{}\"", msg.chars().take(300).collect::<String>()));
                            }
                            Some("Page.javascriptDialogClosed") => *dialog.lock().unwrap() = None,
                            _ => {}
                        }
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
    #[test]
    fn sandbox_unavailable_reconhece_a_falha_de_namespace_do_container() {
        let erro = "navegador encerrou durante Browser.getVersion (/usr/bin/google-chrome; signal: 6 (SIGABRT) (core dumped)): Failed to move to new namespace: PID namespaces supported, Network namespace supported, but failed: errno = Operation not permitted | [38943:38943:1023/113922.773721:FATAL:content/browser/zygote_host/zygote_host_impl_linux.cc(228)] Zygote process exited prematurely with exit code 1";
        assert!(super::sandbox_unavailable(erro));
        assert!(!super::sandbox_unavailable("tempo esgotado em Browser.getVersion (/usr/bin/google-chrome; ainda em execucao)"));
        assert!(!super::sandbox_unavailable("navegador encerrou durante Browser.getVersion (/usr/bin/google-chrome; exit status: 1): Failed to connect to the bus: Could not parse server address"));
    }

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

    #[cfg(target_os = "macos")]
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

    #[cfg(target_os = "linux")]
    #[test]
    fn candidates_include_linux_chrome_and_playwright() {
        let home = tempfile::tempdir().unwrap();
        let pw = home.path().join(".cache/ms-playwright/chromium-1243/chrome-linux/chrome");
        std::fs::create_dir_all(pw.parent().unwrap()).unwrap();
        let c = candidates(home.path());
        assert_eq!(c[0].path, PathBuf::from("/usr/bin/google-chrome-stable"));
        assert!(c.iter().any(|b| b.path == PathBuf::from("/usr/bin/chromium")));
        assert!(c.iter().any(|b| b.path == pw));
    }

    /// `[ref=N]` da primeira linha da leitura que contem `needle`.
    fn ref_of(text: &str, needle: &str) -> u64 {
        let line = text.lines().find(|l| l.contains(needle)).unwrap_or_else(|| panic!("{needle} nao esta na pagina:\n{text}"));
        line.split("[ref=").nth(1).and_then(|r| r.split(']').next()).and_then(|n| n.parse().ok()).unwrap()
    }

    /// Prova real das acoes: clique, digitacao com Enter, lista de opcoes, dialogo `confirm` e ref vencido.
    #[tokio::test]
    async fn real_browser_clicks_types_selects_and_answers_dialogs() {
        let home = crate::paths::home_dir().unwrap();
        let Some(binary) = detect_binary(&home, None) else {
            eprintln!("sem navegador na maquina; teste ignorado");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let b = Browser::launch(binary, &dir.path().join("dot-acoes"), &home, &LaunchOptions { headless: true, executable: None }).await.unwrap();
        let html = "data:text/html,<title>Acoes</title>\
            <button onclick=\"document.getElementById('out').textContent='clicou'\">Excluir anuncio</button><p id=out>nada</p>\
            <input aria-label='Busca' onkeydown=\"if(event.key==='Enter'){document.title='enviou '+this.value}\">\
            <select aria-label='Status' onchange=\"document.getElementById('sel').textContent='status '+this.value\"><option value='0'>Ativo</option><option value='11'>Finalizado</option></select><p id=sel>-</p>\
            <button onclick=\"document.getElementById('out').textContent=confirm('Deseja excluir?')?'confirmado':'desistiu'\">Confirmar exclusao</button>";
        let page = b.open(html).await.unwrap();

        let after = b.click(ref_of(&page.text, "button \"Excluir anuncio\"")).await.unwrap();
        assert!(after.text.contains("clicou"), "{}", after.text);
        assert_eq!(b.describe(ref_of(&page.text, "button \"Excluir anuncio\"")).await.unwrap(), "button \"Excluir anuncio\"");

        let busca = ref_of(&page.text, "textbox \"Busca\"");
        b.type_text(busca, "rascunho", false, false).await.unwrap();
        let typed = b.type_text(busca, "kit 20", true, true).await.unwrap();
        assert_eq!(typed.title, "enviou kit 20", "clear substitui o texto e submit tecla Enter");

        let chosen = b.select(ref_of(&page.text, "combobox \"Status\""), "Finalizado").await.unwrap();
        assert!(chosen.text.contains("status 11"), "{}", chosen.text);
        let wrong = b.select(ref_of(&page.text, "combobox \"Status\""), "Pausado").await.unwrap_err();
        assert!(wrong.message().contains("Ativo | Finalizado"), "{}", wrong.message());

        let confirm = ref_of(&page.text, "button \"Confirmar exclusao\"");
        let asked = b.click(confirm).await.unwrap();
        assert!(asked.text.contains("Deseja excluir?") && b.dialog().is_some(), "{}", asked.text);
        assert!(b.click(confirm).await.is_err(), "com dialogo aberto, so dots_browser_press");
        let cancelled = b.press("Escape").await.unwrap();
        assert!(cancelled.text.contains("desistiu"), "{}", cancelled.text);
        b.click(confirm).await.unwrap();
        let accepted = b.press("Enter").await.unwrap();
        assert!(accepted.text.contains("confirmado") && b.dialog().is_none(), "{}", accepted.text);

        b.open("data:text/html,<title>Outra</title><p>x</p>").await.unwrap();
        let stale = b.click(confirm).await.unwrap_err();
        assert!(stale.message().contains("nao existe mais"), "{}", stale.message());
        assert!(b.press("F13").await.is_err());
        b.close().await;
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
