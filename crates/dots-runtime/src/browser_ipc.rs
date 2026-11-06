//! IPC do navegador por DOT: estado e transmissao ao vivo ("computador do DOT") para a interface.

use std::sync::Arc;

use dots_core::DotsResult;
use serde_json::{Value, json};

use crate::browser::{Browser, LaunchOptions, PageSnapshot};
use crate::personal_chrome::PersonalChrome;
use crate::service::RuntimeService;

pub enum AgentBrowser {
    Isolated(Arc<Browser>),
    Personal(Arc<PersonalChrome>),
}

impl AgentBrowser {
    pub async fn open(&self, url: &str) -> DotsResult<PageSnapshot> {
        match self { Self::Isolated(b) => b.open(url).await, Self::Personal(b) => b.open(url).await }
    }
    pub async fn read(&self) -> DotsResult<PageSnapshot> {
        match self { Self::Isolated(b) => b.read().await, Self::Personal(b) => b.read().await }
    }
    pub async fn click(&self, r: u64) -> DotsResult<PageSnapshot> {
        match self { Self::Isolated(b) => b.click(r).await, Self::Personal(b) => b.click(r).await }
    }
    pub async fn type_text(&self, r: u64, text: &str, clear: bool, submit: bool) -> DotsResult<PageSnapshot> {
        match self { Self::Isolated(b) => b.type_text(r, text, clear, submit).await, Self::Personal(b) => b.type_text(r, text, clear, submit).await }
    }
    pub async fn select(&self, r: u64, option: &str) -> DotsResult<PageSnapshot> {
        match self { Self::Isolated(b) => b.select(r, option).await, Self::Personal(b) => b.select(r, option).await }
    }
    pub async fn press(&self, key: &str) -> DotsResult<PageSnapshot> {
        match self { Self::Isolated(b) => b.press(key).await, Self::Personal(b) => b.press(key).await }
    }
    pub async fn current_url(&self) -> DotsResult<String> {
        match self { Self::Isolated(b) => b.current_url().await, Self::Personal(b) => b.current_url().await }
    }
    pub async fn describe(&self, r: u64) -> DotsResult<String> {
        match self { Self::Isolated(b) => b.describe(r).await, Self::Personal(b) => b.describe(r).await }
    }
    pub fn dialog(&self) -> Option<String> {
        match self { Self::Isolated(b) => b.dialog(), Self::Personal(_) => None }
    }
}

impl RuntimeService {
    pub async fn agent_browser_get(&self, dot_id: &str) -> DotsResult<AgentBrowser> {
        if self.browser_mode().await == "personal_chrome" {
            Ok(AgentBrowser::Personal(self.personal_chrome.get(dot_id).await))
        } else {
            Ok(AgentBrowser::Isolated(self.browsers.get(dot_id, &self.browser_options().await).await?))
        }
    }

    pub async fn agent_browser_running(&self, dot_id: &str) -> Option<AgentBrowser> {
        if self.browser_mode().await == "personal_chrome" {
            self.personal_chrome.running(dot_id).await.map(AgentBrowser::Personal)
        } else {
            self.browsers.running(dot_id).await.map(AgentBrowser::Isolated)
        }
    }
    pub async fn browser_mode(&self) -> &'static str {
        let personal = self.storage.call(|st| st.settings().get("browser.mode")).await.ok().flatten().is_some_and(|s| s.value == "personal_chrome");
        if personal { "personal_chrome" } else { "isolated" }
    }

    /// Opcoes de lancamento vindas das configuracoes (`browser.headless`, `browser.executable`).
    pub async fn browser_options(&self) -> LaunchOptions {
        let headless = self.storage.call(|st| st.settings().get("browser.headless")).await.ok().flatten().and_then(|s| s.value.as_bool()).unwrap_or(true);
        let executable = self.storage.call(|st| st.settings().get("browser.executable")).await.ok().flatten().and_then(|s| s.value.as_str().map(std::path::PathBuf::from));
        LaunchOptions { headless, executable }
    }

    /// `{running, watching, url, title, binary}` do navegador do DOT, sem inicia-lo.
    pub async fn browser_status(&self, dot_id: &str) -> Value {
        if self.browser_mode().await == "personal_chrome" {
            return match self.personal_chrome.running(dot_id).await {
                Some(b) => {
                    let (url, title) = b.page_info().await.unwrap_or_default();
                    json!({ "mode": "personal_chrome", "running": true, "watching": false, "url": url, "title": title, "binary": "Chromium pessoal", "connected": true })
                }
                None => json!({ "mode": "personal_chrome", "running": false, "watching": false, "url": "", "title": "", "binary": "Chromium pessoal", "connected": self.personal_chrome.connected() }),
            };
        }
        match self.browsers.running(dot_id).await {
            Some(b) => {
                let (url, title) = b.page_info().await.unwrap_or_default();
                json!({ "mode": "isolated", "running": true, "watching": self.browsers.is_watched(dot_id), "url": url, "title": title, "binary": b.binary.label })
            }
            None => json!({ "mode": "isolated", "running": false, "watching": false, "url": "", "title": "", "binary": Value::Null }),
        }
    }

    /// Comeca ou renova a transmissao; quadros chegam como evento `browser.frame {dot_id, data, width, height}`.
    pub async fn browser_watch(&self, dot_id: &str) -> DotsResult<Value> {
        if self.browser_mode().await == "personal_chrome" {
            let mut status = self.browser_status(dot_id).await;
            status["started"] = Value::Bool(false);
            return Ok(status);
        }
        let opts = self.browser_options().await;
        let events = self.events.clone();
        let id = dot_id.to_string();
        let started = self
            .browsers
            .watch(dot_id, &opts, move |f| {
                events.emit("browser.frame", json!({ "dot_id": id, "data": f.data, "width": f.width, "height": f.height }));
            })
            .await?;
        let mut status = self.browser_status(dot_id).await;
        status["started"] = Value::Bool(started);
        Ok(status)
    }

    pub fn browser_unwatch(&self, dot_id: &str) -> Value {
        self.browsers.unwatch(dot_id);
        json!({ "ok": true })
    }

    /// Abre uma URL digitada pelo usuario na aba deste DOT dentro do Chromium pessoal.
    pub async fn browser_open_personal(&self, dot_id: &str, url: &str) -> DotsResult<Value> {
        if self.browser_mode().await != "personal_chrome" {
            return Err(dots_core::DotsError::Validation("selecione Chromium pessoal em Conectar aplicativos".into()));
        }
        if !(url.starts_with("https://") || url.starts_with("http://")) || crate::policy::url_host(url).is_empty() {
            return Err(dots_core::DotsError::Validation("informe uma URL completa http ou https".into()));
        }
        let id = dot_id.to_string();
        self.storage.call(move |st| st.dots().get(&id).map(|_| ())).await?;
        let page = self.personal_chrome.get(dot_id).await.open_focused(url).await?;
        self.events.emit("browser.personal_opened", json!({"dot_id":dot_id,"url":page.url}));
        Ok(self.browser_status(dot_id).await)
    }

    pub async fn browser_personal_setup(&self, extension_id: Option<&str>) -> DotsResult<Value> {
        let saved = self.storage.call(|st| st.settings().get("browser.extension_id")).await?.and_then(|s| s.value.as_str().map(str::to_string));
        let id = extension_id.or(saved.as_deref());
        let path = crate::personal_chrome::prepare(&self.layout.root, &self.home, &self.dotsd_path, id)?;
        if let Some(id) = extension_id {
            let value = json!(id);
            self.storage.call(move |st| st.settings().set("browser.extension_id", &value).map(|_| ())).await?;
        }
        Ok(json!({"path":path,"extension_id":id,"connected":self.personal_chrome.connected()}))
    }
}
