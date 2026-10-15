//! IPC do navegador por DOT: estado e transmissao ao vivo ("computador do DOT") para a interface.

use dots_core::DotsResult;
use serde_json::{Value, json};

use crate::browser::LaunchOptions;
use crate::service::RuntimeService;

impl RuntimeService {
    /// Opcoes de lancamento vindas das configuracoes (`browser.headless`, `browser.executable`).
    pub async fn browser_options(&self) -> LaunchOptions {
        let headless = self.storage.call(|st| st.settings().get("browser.headless")).await.ok().flatten().and_then(|s| s.value.as_bool()).unwrap_or(true);
        let executable = self.storage.call(|st| st.settings().get("browser.executable")).await.ok().flatten().and_then(|s| s.value.as_str().map(std::path::PathBuf::from));
        LaunchOptions { headless, executable }
    }

    /// `{running, watching, url, title, binary}` do navegador do DOT, sem inicia-lo.
    pub async fn browser_status(&self, dot_id: &str) -> Value {
        match self.browsers.running(dot_id).await {
            Some(b) => {
                let (url, title) = b.page_info().await.unwrap_or_default();
                json!({ "running": true, "watching": self.browsers.is_watched(dot_id), "url": url, "title": title, "binary": b.binary.label })
            }
            None => json!({ "running": false, "watching": false, "url": "", "title": "", "binary": Value::Null }),
        }
    }

    /// Comeca ou renova a transmissao; quadros chegam como evento `browser.frame {dot_id, data, width, height}`.
    pub async fn browser_watch(&self, dot_id: &str) -> DotsResult<Value> {
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
}
