//! Consumo das assinaturas (copiado do ovrgit). Nada e enviado alem do que os proprios CLIs ja usam:
//! - Claude: o mesmo endpoint de uso da conta que o Claude Code consulta, com o login ja feito (token no
//!   Keychain do macOS ou em `~/.claude/.credentials.json`). O token nunca sai daqui. O token de acesso dura
//!   poucas horas e so o CLI o renova: quando vence, pedimos ao CLI que renove (`claude mcp list`).
//! - Codex: percentuais que o proprio CLI grava nos registros das sessoes (`~/.codex/sessions/**/*.jsonl`).
//! - Antigravity: `agy -p /usage --output-format json`, que segundo o CLI nao roda o agente nem gasta cota; a conta
//!   conectada vem de `~/.gemini/jetski-standalone-oauth-token`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dots_core::{EngineKind, ProviderUsage, Timestamp, UsageInfo, UsageWindow};
use tokio::sync::Mutex;

use crate::service::RuntimeService;

const CACHE: Duration = Duration::from_secs(5 * 60);
const RENEW_EVERY: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct UsageCache {
    claude: Mutex<Option<(Instant, Option<ProviderUsage>)>>,
    codex: Mutex<Option<(Instant, Option<ProviderUsage>)>>,
    agy: Mutex<Option<(Instant, Option<ProviderUsage>)>>,
    renewed_at: Mutex<Option<Instant>>,
}

/// Login do Claude Code: token, plano ("max", "pro"...), faixa ("20x") e validade do token (ms).
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeLogin {
    pub token: String,
    pub plan: Option<String>,
    pub tier: Option<String>,
    pub expires_at: Option<i64>,
}

impl ClaudeLogin {
    pub fn plan_label(&self) -> Option<String> {
        self.plan.as_ref().map(|p| match &self.tier {
            Some(t) => format!("{p} {t}"),
            None => p.clone(),
        })
    }
    fn expired(&self) -> bool {
        self.expires_at.map(|e| e - now_ms() < 60_000).unwrap_or(false)
    }
}

fn now_ms() -> i64 {
    dots_core::now().timestamp_millis()
}

/// Le `claudeAiOauth` do JSON de credenciais do Claude Code. So token e plano sao lidos.
pub fn parse_claude_credentials(json: &str) -> Option<ClaudeLogin> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let o = v.get("claudeAiOauth")?;
    let token = o.get("accessToken")?.as_str()?.to_string();
    if token.is_empty() {
        return None;
    }
    let tier = o.get("rateLimitTier").and_then(|t| t.as_str()).and_then(|t| {
        // "default_claude_max_20x" -> "20x"
        let last = t.rsplit('_').next()?;
        (last.ends_with('x') && last.len() > 1 && last[..last.len() - 1].chars().all(|c| c.is_ascii_digit())).then(|| last.to_string())
    });
    Some(ClaudeLogin {
        token,
        plan: o.get("subscriptionType").and_then(|p| p.as_str()).filter(|p| !p.is_empty()).map(String::from),
        tier,
        expires_at: o.get("expiresAt").and_then(|e| e.as_i64()),
    })
}

/// Resposta de `/api/oauth/usage`: janelas de 5 horas e 7 dias.
pub fn parse_claude_usage(body: &str) -> Option<(Option<UsageWindow>, Option<UsageWindow>)> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let win = |w: Option<&serde_json::Value>| -> Option<UsageWindow> {
        let w = w?;
        let pct = w.get("utilization")?.as_f64()?;
        let resets_at = w.get("resets_at").and_then(|r| r.as_str()).and_then(|r| chrono::DateTime::parse_from_rfc3339(r).ok()).map(|d| d.with_timezone(&chrono::Utc));
        Some(UsageWindow { pct: pct.clamp(0.0, 100.0), resets_at })
    };
    Some((win(v.get("five_hour")), win(v.get("seven_day"))))
}

/// Limites do ChatGPT gravados pelo Codex em uma linha `token_count` do registro da sessao.
pub fn parse_codex_line(line: &str, fallback_at: Timestamp) -> Option<ProviderUsage> {
    if !line.contains("rate_limits") {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let payload = v.get("payload")?;
    if payload.get("type").and_then(|t| t.as_str()) != Some("token_count") {
        return None;
    }
    let rl = payload.get("rate_limits")?;
    let at = v.get("timestamp").and_then(|t| t.as_str()).and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or(fallback_at);
    let wins: Vec<(f64, f64, Option<i64>)> = ["primary", "secondary"]
        .iter()
        .filter_map(|k| rl.get(k))
        .filter_map(|w| Some((w.get("used_percent")?.as_f64()?, w.get("window_minutes").and_then(|m| m.as_f64()).unwrap_or(0.0), w.get("resets_at").and_then(|r| r.as_i64()))))
        .collect();
    let pick = |test: &dyn Fn(f64) -> bool| -> Option<UsageWindow> {
        wins.iter().find(|(_, m, _)| test(*m)).map(|(pct, _, resets)| UsageWindow {
            pct: pct.clamp(0.0, 100.0),
            resets_at: resets.and_then(|s| chrono::DateTime::from_timestamp(s, 0)),
        })
    };
    Some(ProviderUsage {
        five_hour: pick(&|m| m > 0.0 && m <= 600.0),
        week: pick(&|m| m >= 7.0 * 24.0 * 60.0 - 60.0),
        plan: rl.get("plan_type").and_then(|p| p.as_str()).map(String::from),
        at,
        error: None,
    })
}

/// Email dentro do `id_token` (JWT) do Antigravity.
pub fn parse_agy_token(json: &str) -> Option<Option<String>> {
    use base64::Engine as _;
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v.get("token").is_none() && v.get("id_token").is_none() {
        return None;
    }
    let email = v
        .get("id_token")
        .and_then(|t| t.as_str())
        .and_then(|t| t.split('.').nth(1))
        .and_then(|p| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(p.trim_end_matches('=')).ok())
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|c| c.get("email").and_then(|e| e.as_str()).map(String::from));
    Some(email)
}

/// Saida de `agy -p /usage --output-format json`: a cota e separada por grupo de modelos (Gemini, Claude e GPT...),
/// cada um com janela semanal e de 5 horas. Para cada janela fica o grupo mais gasto. Retorna (5 horas, semana).
pub fn parse_agy_usage(body: &str) -> Option<(Option<UsageWindow>, Option<UsageWindow>)> {
    let v: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    let groups = v.get("command")?.get("data")?.get("groups")?.as_array()?;
    let mut five_hour: Option<UsageWindow> = None;
    let mut week: Option<UsageWindow> = None;
    for b in groups.iter().filter_map(|g| g.get("buckets")?.as_array()).flatten() {
        let Some(left) = b.get("remaining_fraction").and_then(|f| f.as_f64()) else { continue };
        let slot = match b.get("window").and_then(|w| w.as_str()) {
            Some("5h") => &mut five_hour,
            Some("weekly") => &mut week,
            _ => continue,
        };
        let pct = ((1.0 - left) * 100.0).clamp(0.0, 100.0);
        if slot.as_ref().map(|w| pct > w.pct).unwrap_or(true) {
            let resets_at = b.get("reset_time").and_then(|r| r.as_str()).and_then(|r| chrono::DateTime::parse_from_rfc3339(r).ok()).map(|d| d.with_timezone(&chrono::Utc));
            *slot = Some(UsageWindow { pct, resets_at });
        }
    }
    Some((five_hour, week))
}

/// Ultimo registro de limites nos arquivos de sessao do Codex (os 30 mais recentes dos ultimos 14 dias).
pub fn scan_codex_sessions(home: &Path) -> Option<ProviderUsage> {
    use std::io::{Read, Seek, SeekFrom};
    let root = home.join(".codex/sessions");
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = vec![];
    let now = dots_core::now();
    for d in 0..14 {
        let dt = now - chrono::Duration::days(d);
        let dir = root.join(dt.format("%Y/%m/%d").to_string());
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "jsonl").unwrap_or(false) {
                if let Ok(m) = e.metadata() {
                    files.push((p, m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)));
                }
            }
        }
    }
    files.sort_by(|a, b| b.1.cmp(&a.1));
    for (file, mtime) in files.into_iter().take(30) {
        let Ok(mut f) = std::fs::File::open(&file) else { continue };
        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
        let len = size.min(512 * 1024);
        if f.seek(SeekFrom::Start(size - len)).is_err() {
            continue;
        }
        let mut buf = Vec::with_capacity(len as usize);
        if f.read_to_end(&mut buf).is_err() {
            continue;
        }
        let text = String::from_utf8_lossy(&buf);
        let at: Timestamp = chrono::DateTime::<chrono::Utc>::from(mtime);
        for line in text.lines().rev() {
            if let Some(u) = parse_codex_line(line, at) {
                return Some(u);
            }
        }
    }
    None
}

async fn run(exe: &Path, args: &[&str], cwd: Option<&Path>, timeout: Duration) -> Option<String> {
    let mut cmd = tokio::process::Command::new(exe);
    cmd.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    let out = tokio::time::timeout(timeout, cmd.output()).await.ok()?.ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl RuntimeService {
    pub async fn usage(&self, force: bool) -> UsageInfo {
        let (claude, codex, agy) = tokio::join!(self.claude_usage(force), self.codex_usage(force), self.agy_usage(force));
        UsageInfo { claude, codex, agy }
    }

    async fn claude_login(&self) -> Option<ClaudeLogin> {
        if cfg!(target_os = "macos") {
            if let Some(out) = run(Path::new("/usr/bin/security"), &["find-generic-password", "-s", "Claude Code-credentials", "-w"], None, Duration::from_secs(5)).await {
                if let Some(l) = parse_claude_credentials(&out) {
                    return Some(l);
                }
            }
        }
        std::fs::read_to_string(self.home.join(".claude/.credentials.json")).ok().and_then(|s| parse_claude_credentials(&s))
    }

    /// Faz o CLI renovar o token de acesso. `mcp list` e o comando mais barato que passa pela renovacao e nao
    /// gasta inferencia; roda na pasta temporaria para nao subir os MCPs de um projeto. No maximo uma vez por minuto.
    async fn renew_claude_token(&self) {
        {
            let mut last = self.usage.renewed_at.lock().await;
            if last.map(|t| t.elapsed() < RENEW_EVERY).unwrap_or(false) {
                return;
            }
            *last = Some(Instant::now());
        }
        let cfg = self.storage.call(|s| s.engines().list()).await.ok().and_then(|l| l.into_iter().find(|c| c.kind == EngineKind::Claude));
        let Ok(exe) = dots_engines::resolve_executable("claude", cfg.as_ref().and_then(|c| c.executable_override.as_deref()), &self.path_dirs) else { return };
        let _ = dots_engines::run_command(&exe, &["mcp", "list"], &self.home, &self.path_dirs, Duration::from_secs(45)).await;
    }

    async fn fetch_claude_usage(&self, login: &ClaudeLogin) -> Result<ProviderUsage, Option<String>> {
        let plan = login.plan_label();
        let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|e| Some(e.to_string()))?;
        let res = client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("Authorization", format!("Bearer {}", login.token))
            .header("anthropic-beta", "oauth-2025-04-20")
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| Some(if e.is_timeout() { "Sem resposta do Claude.".to_string() } else { e.to_string() }))?;
        let status = res.status().as_u16();
        if status == 401 || status == 403 {
            return Err(None); // nao autorizado: token vencido
        }
        if !(200..300).contains(&status) {
            return Ok(ProviderUsage { five_hour: None, week: None, plan, at: dots_core::now(), error: Some(format!("Claude respondeu {status}")) });
        }
        let body = res.text().await.map_err(|e| Some(e.to_string()))?;
        let (five_hour, week) = parse_claude_usage(&body).ok_or_else(|| Some("resposta do Claude fora do formato esperado".to_string()))?;
        Ok(ProviderUsage { five_hour, week, plan, at: dots_core::now(), error: None })
    }

    async fn claude_usage(&self, force: bool) -> Option<ProviderUsage> {
        if !force {
            if let Some((at, v)) = self.usage.claude.lock().await.as_ref() {
                if at.elapsed() < CACHE {
                    return v.clone();
                }
            }
        }
        let value = self.claude_usage_fresh().await;
        *self.usage.claude.lock().await = Some((Instant::now(), value.clone()));
        value
    }

    async fn claude_usage_fresh(&self) -> Option<ProviderUsage> {
        let mut login = self.claude_login().await?;
        let mut renewed = false;
        if login.expired() {
            self.renew_claude_token().await;
            renewed = true;
            login = self.claude_login().await.unwrap_or(login);
        }
        let mut r = self.fetch_claude_usage(&login).await;
        if matches!(r, Err(None)) && !renewed {
            self.renew_claude_token().await;
            login = self.claude_login().await.unwrap_or(login);
            r = self.fetch_claude_usage(&login).await;
        }
        Some(match r {
            Ok(u) => u,
            Err(Some(e)) => ProviderUsage { five_hour: None, week: None, plan: login.plan_label(), at: dots_core::now(), error: Some(e) },
            Err(None) => ProviderUsage { five_hour: None, week: None, plan: login.plan_label(), at: dots_core::now(), error: Some("Login do Claude expirado: rode \"claude\" no terminal para renovar.".into()) },
        })
    }

    async fn codex_usage(&self, force: bool) -> Option<ProviderUsage> {
        if !force {
            if let Some((at, v)) = self.usage.codex.lock().await.as_ref() {
                if at.elapsed() < CACHE {
                    return v.clone();
                }
            }
        }
        let home = self.home.clone();
        let value = tokio::task::spawn_blocking(move || scan_codex_sessions(&home)).await.ok().flatten();
        *self.usage.codex.lock().await = Some((Instant::now(), value.clone()));
        value
    }

    async fn agy_usage(&self, force: bool) -> Option<ProviderUsage> {
        if !force {
            if let Some((at, v)) = self.usage.agy.lock().await.as_ref() {
                if at.elapsed() < CACHE {
                    return v.clone();
                }
            }
        }
        let value = self.agy_usage_fresh().await;
        *self.usage.agy.lock().await = Some((Instant::now(), value.clone()));
        value
    }

    /// Antigravity: conta conectada e limites do `/usage` do proprio CLI.
    async fn agy_usage_fresh(&self) -> Option<ProviderUsage> {
        let cfg = self.storage.call(|s| s.engines().list()).await.ok().and_then(|l| l.into_iter().find(|c| c.kind == EngineKind::Agy));
        let exe = dots_engines::resolve_executable("agy", cfg.as_ref().and_then(|c| c.executable_override.as_deref()), &self.path_dirs).ok()?;
        let logged = std::fs::read_to_string(self.home.join(".gemini/jetski-standalone-oauth-token")).ok().and_then(|s| parse_agy_token(&s));
        let Some(email) = logged else {
            return Some(ProviderUsage { five_hour: None, week: None, plan: None, at: dots_core::now(), error: Some("Nao conectado: rode \"agy\" no terminal.".into()) });
        };
        let out = dots_engines::run_command(&exe, &["-p", "/usage", "--output-format", "json"], &self.home, &self.path_dirs, Duration::from_secs(30)).await;
        Some(match out.as_ref().ok().and_then(|o| parse_agy_usage(&o.stdout)) {
            Some((five_hour, week)) => ProviderUsage { five_hour, week, plan: email, at: dots_core::now(), error: None },
            None => ProviderUsage { five_hour: None, week: None, plan: email, at: dots_core::now(), error: Some(match out {
                Err(e) => e.to_string(),
                Ok(_) => "resposta do agy fora do formato esperado".into(),
            }) },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_credentials_and_usage_parse() {
        let l = parse_claude_credentials(r#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":1,"subscriptionType":"max","rateLimitTier":"default_claude_max_20x"}}"#).unwrap();
        assert_eq!(l.plan_label().as_deref(), Some("max 20x"));
        assert!(l.expired());
        assert!(parse_claude_credentials(r#"{"claudeAiOauth":{"accessToken":""}}"#).is_none());
        let (f, w) = parse_claude_usage(r#"{"five_hour":{"utilization":76.4,"resets_at":"2026-10-15T20:00:00Z"},"seven_day":{"utilization":149,"resets_at":null}}"#).unwrap();
        assert_eq!(f.as_ref().unwrap().pct, 76.4);
        assert!(f.unwrap().resets_at.is_some());
        assert_eq!(w.as_ref().unwrap().pct, 100.0);
        assert!(w.unwrap().resets_at.is_none());
    }

    #[test]
    fn codex_line_parse_picks_windows() {
        let line = r#"{"timestamp":"2026-10-15T12:00:00Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":6,"window_minutes":300,"resets_at":1760600000},"secondary":{"used_percent":6.5,"window_minutes":10080,"resets_at":1761000000},"plan_type":"pro"}}}"#;
        let u = parse_codex_line(line, dots_core::now()).unwrap();
        assert_eq!(u.five_hour.as_ref().unwrap().pct, 6.0);
        assert_eq!(u.week.as_ref().unwrap().pct, 6.5);
        assert_eq!(u.plan.as_deref(), Some("pro"));
        assert_eq!(u.at.to_rfc3339(), "2026-10-15T12:00:00+00:00");
        assert!(parse_codex_line(r#"{"payload":{"type":"agent_message"}}"#, dots_core::now()).is_none());
    }

    #[test]
    fn codex_sessions_scan_finds_latest_file() {
        let dir = tempfile::tempdir().unwrap();
        let day = dots_core::now().format("%Y/%m/%d").to_string();
        let d = dir.path().join(".codex/sessions").join(day);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.jsonl"), "{\"x\":1}\n{\"timestamp\":\"2026-10-15T12:00:00Z\",\"payload\":{\"type\":\"token_count\",\"rate_limits\":{\"primary\":{\"used_percent\":40,\"window_minutes\":300}}}}\n").unwrap();
        let u = scan_codex_sessions(dir.path()).unwrap();
        assert_eq!(u.five_hour.unwrap().pct, 40.0);
        assert!(u.week.is_none());
    }

    #[test]
    fn agy_token_email() {
        use base64::Engine as _;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"email":"x@y.com"}"#);
        let json = format!(r#"{{"id_token":"h.{payload}.s","token":{{}}}}"#);
        assert_eq!(parse_agy_token(&json).unwrap().as_deref(), Some("x@y.com"));
        assert!(parse_agy_token(r#"{}"#).is_none());
    }

    #[test]
    fn agy_usage_picks_most_used_group() {
        let body = r#"{"status":"SUCCESS","command":{"name":"usage","data":{"groups":[
            {"name":"Gemini Models","buckets":[{"window":"weekly","remaining_fraction":0.9,"reset_time":"2026-10-23T13:14:19Z"},{"window":"5h","remaining_fraction":1,"reset_time":"2026-10-16T18:14:19Z"}]},
            {"name":"Claude and GPT models","buckets":[{"window":"weekly","remaining_fraction":0.6,"reset_time":"2026-10-22T10:00:00Z"},{"window":"5h","remaining_fraction":0.75,"reset_time":"2026-10-16T17:00:00Z"}]}]}}}"#;
        let (f, w) = parse_agy_usage(body).unwrap();
        let (f, w) = (f.unwrap(), w.unwrap());
        assert_eq!(f.pct.round(), 25.0);
        assert_eq!(f.resets_at.unwrap().to_rfc3339(), "2026-10-16T17:00:00+00:00");
        assert_eq!(w.pct.round(), 40.0);
        assert_eq!(w.resets_at.unwrap().to_rfc3339(), "2026-10-22T10:00:00+00:00");
        assert!(parse_agy_usage("Gemini Models\tWeekly").is_none());
    }
}
