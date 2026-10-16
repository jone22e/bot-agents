//! Integracao global do AGY: o CLI nao aceita servidor MCP por execucao, entao o DOTS
//! registra `agy mcp add dots ...` e regras `mcp(dots/<tool>)` em `permissions.allow`
//! do `~/.gemini/antigravity-cli/settings.json`. Sempre com consentimento explicito e reversivel.

use std::path::{Path, PathBuf};
use std::time::Duration;

use dots_core::{DotsError, DotsResult};
use serde::Serialize;
use serde_json::{Value, json};

use crate::tools::tool_definitions;

#[derive(Debug, Clone, Serialize)]
pub struct AgySetupStatus {
    pub settings_path: String,
    pub server_registered: bool,
    pub server_command: Option<String>,
    pub rules_present: Vec<String>,
    pub rules_missing: Vec<String>,
    pub configured: bool,
}

fn settings_path(home: &Path) -> PathBuf {
    home.join(".gemini/antigravity-cli/settings.json")
}

fn expected_rules() -> Vec<String> {
    tool_definitions()
        .into_iter()
        .chain(dots_flexi::tool_definitions())
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .filter(|n| n != "permission_prompt")
        .map(|n| format!("mcp(dots/{n})"))
        .collect()
}

fn read_settings(path: &Path) -> DotsResult<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&raw).map_err(|e| DotsError::Engine(format!("settings.json do AGY invalido: {e}")))
}

pub async fn status(agy: &Path, home: &Path, path_dirs: &[PathBuf]) -> DotsResult<AgySetupStatus> {
    let out = dots_engines::run_command(agy, &["mcp", "list"], home, path_dirs, Duration::from_secs(15)).await?;
    let text = format!("{}\n{}", out.stdout, out.stderr);
    let server_line = text.lines().find(|l| l.trim_start().starts_with("dots") || l.contains(" dots ") || l.contains("\"dots\"") || l.starts_with("dots:"));
    let server_registered = server_line.is_some();
    let sp = settings_path(home);
    let settings = read_settings(&sp).unwrap_or(json!({}));
    let allow: Vec<String> = settings
        .get("permissions")
        .and_then(|p| p.get("allow"))
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let expected = expected_rules();
    let present: Vec<String> = expected.iter().filter(|r| allow.contains(r)).cloned().collect();
    let missing: Vec<String> = expected.iter().filter(|r| !allow.contains(r)).cloned().collect();
    Ok(AgySetupStatus {
        settings_path: sp.to_string_lossy().into(),
        server_registered,
        server_command: server_line.map(|l| l.trim().to_string()),
        configured: server_registered && missing.is_empty(),
        rules_present: present,
        rules_missing: missing,
    })
}

/// Registra o servidor e adiciona as regras (idempotente). Faz backup do settings.json.
pub async fn apply(agy: &Path, home: &Path, path_dirs: &[PathBuf], dotsd: &Path, data_dir: &Path) -> DotsResult<AgySetupStatus> {
    let data = data_dir.to_string_lossy().to_string();
    let env_arg = format!("DOTS_DATA_DIR={data}");
    let dotsd_s = dotsd.to_string_lossy().to_string();
    let args = ["mcp", "add", "-e", env_arg.as_str(), "dots", dotsd_s.as_str(), "mcp-gateway"];
    let out = dots_engines::run_command(agy, &args, home, path_dirs, Duration::from_secs(30)).await?;
    if out.exit_code != Some(0) {
        return Err(DotsError::Engine(format!("agy mcp add falhou: {}{}", out.stdout.trim(), out.stderr.trim())));
    }
    let sp = settings_path(home);
    let mut settings = read_settings(&sp)?;
    if sp.exists() {
        let backup = sp.with_extension(format!("json.dots-backup-{}", dots_core::now().format("%Y%m%dT%H%M%S")));
        std::fs::copy(&sp, &backup)?;
    }
    if !settings.is_object() {
        settings = json!({});
    }
    let obj = settings.as_object_mut().unwrap();
    let perms = obj.entry("permissions").or_insert(json!({}));
    if !perms.is_object() {
        *perms = json!({});
    }
    let allow = perms.as_object_mut().unwrap().entry("allow").or_insert(json!([]));
    if !allow.is_array() {
        *allow = json!([]);
    }
    let arr = allow.as_array_mut().unwrap();
    for rule in expected_rules() {
        if !arr.iter().any(|x| x.as_str() == Some(&rule)) {
            arr.push(Value::String(rule));
        }
    }
    if let Some(parent) = sp.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&sp, serde_json::to_string_pretty(&settings)?)?;
    status(agy, home, path_dirs).await
}

/// Remove o servidor e as regras adicionadas pelo DOTS.
pub async fn remove(agy: &Path, home: &Path, path_dirs: &[PathBuf]) -> DotsResult<AgySetupStatus> {
    let _ = dots_engines::run_command(agy, &["mcp", "remove", "dots"], home, path_dirs, Duration::from_secs(30)).await;
    let sp = settings_path(home);
    if sp.exists() {
        let mut settings = read_settings(&sp)?;
        if let Some(arr) = settings.get_mut("permissions").and_then(|p| p.get_mut("allow")).and_then(|a| a.as_array_mut()) {
            arr.retain(|x| !x.as_str().map(|s| s.starts_with("mcp(dots/")).unwrap_or(false));
        }
        // Nao deixa chaves vazias criadas pelo DOTS.
        let empty_allow = settings.get("permissions").and_then(|p| p.get("allow")).and_then(|a| a.as_array()).map(|a| a.is_empty()).unwrap_or(false);
        if empty_allow {
            if let Some(p) = settings.get_mut("permissions").and_then(|p| p.as_object_mut()) {
                p.remove("allow");
            }
        }
        let empty_perms = settings.get("permissions").and_then(|p| p.as_object()).map(|o| o.is_empty()).unwrap_or(false);
        if empty_perms {
            if let Some(o) = settings.as_object_mut() {
                o.remove("permissions");
            }
        }
        std::fs::write(&sp, serde_json::to_string_pretty(&settings)?)?;
    }
    status(agy, home, path_dirs).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_rules_cover_all_tools_except_permission_prompt() {
        let r = expected_rules();
        assert!(r.contains(&"mcp(dots/dots_write_file)".to_string()));
        assert!(r.contains(&"mcp(dots/flexi_ads_status)".to_string()));
        assert!(r.contains(&"mcp(dots/flexi_publicar_anuncio)".to_string()));
        assert!(!r.iter().any(|x| x.contains("permission_prompt")));
    }
}
