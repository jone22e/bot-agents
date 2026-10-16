//! Atualizacao automatica pelos releases do GitHub (mesmo desenho do ovrgit, `src/main/updater.ts`): checa 10 s
//! depois de abrir, a cada 30 minutos e quando a janela volta ao primeiro plano (so se a ultima tentativa foi ha
//! mais de 10 minutos); baixa em segundo plano e instala quando o usuario pede ("Reiniciar e atualizar") ou ao
//! fechar o app.
//!
//! Com uma versao ja baixada, as checagens continuam: se sair outra mais nova, ela e baixada por cima e e a que
//! se instala. Nessas checagens a versao pronta fica na tela como esta; so um download novo muda o estado.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK: Duration = Duration::from_secs(10);
const INTERVAL: Duration = Duration::from_secs(30 * 60);
/** Ao focar a janela, so checa de novo passado este tempo desde a ultima tentativa. */
const FOCUS_MIN_GAP: Duration = Duration::from_secs(10 * 60);
pub const EVENT: &str = "update://state";

#[derive(Clone, Serialize)]
pub struct UpdateState {
    /// idle | checking | available | downloading | ready | error | unsupported
    pub status: &'static str,
    /// Versao instalada.
    pub current: String,
    /// Versao nova encontrada.
    pub version: Option<String>,
    pub percent: Option<u8>,
    pub error: Option<String>,
    /// Ultima checagem que terminou sem versao nova (ms desde 1970).
    pub checked_at: Option<i64>,
    /// "Atualizar automaticamente" ligado.
    pub auto: bool,
}

#[derive(Serialize, Deserialize)]
struct Prefs {
    auto: bool,
}

pub struct Updater {
    state: Mutex<UpdateState>,
    /// Versao encontrada com o automatico desligado (espera o "Baixar").
    available: Mutex<Option<Update>>,
    /// Versao baixada, pronta para instalar.
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    last_attempt: Mutex<Option<Instant>>,
}

impl Updater {
    pub fn new(app: &AppHandle) -> Self {
        let status = if tauri::is_dev() { "unsupported" } else { "idle" };
        let auto = read_prefs(app).map(|p| p.auto).unwrap_or(true);
        Self {
            state: Mutex::new(UpdateState { status, current: app.package_info().version.to_string(), version: None, percent: None, error: None, checked_at: None, auto }),
            available: Mutex::new(None),
            ready: Mutex::new(None),
            last_attempt: Mutex::new(None),
        }
    }

    pub fn snapshot(&self) -> UpdateState {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, app: &AppHandle, f: impl FnOnce(&mut UpdateState)) {
        let s = {
            let mut s = self.state.lock().unwrap();
            f(&mut s);
            s.clone()
        };
        let _ = app.emit(EVENT, s);
    }

    fn is_ready(&self) -> bool {
        self.state.lock().unwrap().status == "ready"
    }
}

fn prefs_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("updater.json"))
}

fn read_prefs(app: &AppHandle) -> Option<Prefs> {
    serde_json::from_slice(&std::fs::read(prefs_path(app)?).ok()?).ok()
}

/// Mensagem curta para o usuario; o erro completo traz detalhes de HTTP.
fn friendly(e: &tauri_plugin_updater::Error) -> String {
    let text = e.to_string();
    if text.contains("error sending request") || text.contains("dns") || text.contains("timed out") || text.contains("connect") {
        return "Sem conexão com o servidor de atualizações.".into();
    }
    if text.contains("404") || text.contains("Could not fetch a valid release JSON") {
        return "Nenhuma versão publicada foi encontrada.".into();
    }
    text.lines().next().unwrap_or_default().chars().take(200).collect()
}

/// `a` e mais nova que `b`? Compara os numeros separados por ponto; "v" na frente e sufixos ("-beta.1") sao ignorados.
fn is_newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| v.trim_start_matches(['v', 'V']).split('-').next().unwrap_or("").split('.').map(|n| n.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let (x, y) = (pa.get(i).copied().unwrap_or(0), pb.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// Agenda as checagens automaticas (chamado no setup do app).
pub fn start(app: &AppHandle) {
    if tauri::is_dev() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if app.state::<Updater>().snapshot().auto {
                check(&app).await;
            }
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

/// Checagem oportunista (janela focada): so com o automatico ligado e passado o intervalo minimo.
pub fn on_focus(app: &AppHandle) {
    let u = app.state::<Updater>();
    if tauri::is_dev() || !u.snapshot().auto {
        return;
    }
    if u.last_attempt.lock().unwrap().is_some_and(|t| t.elapsed() < FOCUS_MIN_GAP) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { check(&app).await });
}

pub async fn check(app: &AppHandle) -> UpdateState {
    let u = app.state::<Updater>();
    let snap = u.snapshot();
    // em desenvolvimento nao ha pacote instalado para substituir; nao interrompe um download em andamento
    if matches!(snap.status, "unsupported" | "checking" | "downloading") {
        return snap;
    }
    *u.last_attempt.lock().unwrap() = Some(Instant::now());
    if !u.is_ready() {
        u.set(app, |s| {
            s.status = "checking";
            s.error = None;
        });
    }
    let found = match app.updater() {
        Ok(up) => up.check().await,
        Err(e) => Err(e),
    };
    match found {
        Ok(Some(update)) => {
            let ready_version = u.ready.lock().unwrap().as_ref().map(|(r, _)| r.version.clone());
            // ja ha uma versao pronta: so uma mais nova interessa (substitui a baixada)
            if ready_version.is_some_and(|rv| !is_newer(&update.version, &rv)) {
                return u.snapshot();
            }
            if snap.auto {
                download_update(app, update).await;
            } else {
                let v = update.version.clone();
                *u.available.lock().unwrap() = Some(update);
                if !u.is_ready() {
                    u.set(app, |s| {
                        s.status = "available";
                        s.version = Some(v);
                    });
                }
            }
        }
        Ok(None) => {
            if !u.is_ready() {
                u.set(app, |s| {
                    s.status = "idle";
                    s.version = None;
                    s.checked_at = Some(now_ms());
                });
            }
        }
        Err(e) => {
            if !u.is_ready() {
                let msg = friendly(&e);
                u.set(app, |s| {
                    s.status = "error";
                    s.error = Some(msg);
                });
            }
        }
    }
    u.snapshot()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

async fn download_update(app: &AppHandle, update: Update) {
    let u = app.state::<Updater>();
    let version = update.version.clone();
    u.set(app, |s| {
        s.status = "downloading";
        s.version = Some(version.clone());
        s.percent = Some(0);
        s.error = None;
    });
    let mut got: u64 = 0;
    let mut last_pct: u8 = 0;
    let res = update
        .download(
            |chunk, total| {
                got += chunk as u64;
                if let Some(t) = total.filter(|t| *t > 0) {
                    let pct = ((got * 100) / t).min(100) as u8;
                    if pct != last_pct {
                        last_pct = pct;
                        app.state::<Updater>().set(app, |s| s.percent = Some(pct));
                    }
                }
            },
            || {},
        )
        .await;
    match res {
        Ok(bytes) => {
            *u.ready.lock().unwrap() = Some((update, bytes));
            *u.available.lock().unwrap() = None;
            u.set(app, |s| {
                s.status = "ready";
                s.version = Some(version);
                s.percent = Some(100);
            });
        }
        Err(e) => {
            let msg = friendly(&e);
            let had_ready = u.ready.lock().unwrap().is_some();
            u.set(app, |s| {
                if had_ready {
                    // a versao baixada antes continua valendo
                    s.status = "ready";
                    s.version = u.ready.lock().unwrap().as_ref().map(|(r, _)| r.version.clone());
                } else {
                    s.status = "error";
                    s.error = Some(msg);
                }
            });
        }
    }
}

/// Instala a versao baixada (substitui o app em /Applications). Retorna se havia algo a instalar.
fn install_ready(app: &AppHandle) -> Result<bool, String> {
    let ready = app.state::<Updater>().ready.lock().unwrap().take();
    let Some((update, bytes)) = ready else { return Ok(false) };
    update.install(bytes).map_err(|e| e.to_string())?;
    tracing::info!(version = %update.version, "atualizacao instalada");
    Ok(true)
}

/// Ao fechar o app: instala a versao pronta, se houver (a proxima abertura ja e a nova).
pub fn install_on_exit(app: &AppHandle) {
    if let Err(e) = install_ready(app) {
        tracing::warn!(error = %e, "falha ao instalar a atualizacao ao fechar");
    }
}

#[tauri::command]
pub fn update_state(app: AppHandle) -> UpdateState {
    app.state::<Updater>().snapshot()
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> UpdateState {
    check(&app).await
}

#[tauri::command]
pub async fn update_download(app: AppHandle) -> UpdateState {
    let pending = app.state::<Updater>().available.lock().unwrap().take();
    if let Some(update) = pending {
        download_update(&app, update).await;
    }
    app.state::<Updater>().snapshot()
}

/// Fecha o app e abre a versao nova.
#[tauri::command]
pub fn update_install(app: AppHandle) -> Result<(), String> {
    if install_ready(&app)? {
        app.restart();
    }
    Ok(())
}

#[tauri::command]
pub fn update_set_auto(app: AppHandle, auto: bool) -> UpdateState {
    if let Some(p) = prefs_path(&app) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&p, serde_json::to_vec(&Prefs { auto }).unwrap_or_default());
    }
    let u = app.state::<Updater>();
    u.set(&app, |s| s.auto = auto);
    if auto {
        on_focus(&app);
    }
    u.snapshot()
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_versions_by_number() {
        assert!(is_newer("0.4.10", "0.4.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("0.4.0-beta.1", "0.4.0"));
        assert!(!is_newer("0.3.9", "0.4.0"));
    }
}
