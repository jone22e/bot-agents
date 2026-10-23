//! Lista de modelos que cada CLI informa:
//! - Codex: cache local; se vazio, `codex app-server` (`model/list`).
//! - AGY: saida de `agy models` (uma linha `id<TAB>nome` por modelo).
//! O Claude Code nao lista modelos; a lista dele fica na interface.

use std::path::{Path, PathBuf};

use dots_core::EngineModel;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::detect::DetectOptions;
use crate::process::run_command;

/// Descricoes do catalogo do Codex vem em ingles; as conhecidas ficam em portugues.
fn describe_codex(d: &str) -> String {
    match d.trim() {
        "Frontier intelligence for the most demanding work." => "O mais capaz, para o trabalho mais exigente.",
        "Latest workhorse model for coding and everyday work." => "Modelo de trabalho mais recente, para codigo e tarefas do dia a dia.",
        "Workhorse model for coding and everyday work." => "Modelo de trabalho para codigo e tarefas do dia a dia.",
        "Previous generation workhorse model." => "Geracao anterior do modelo de trabalho.",
        "Older generation workhorse model." => "Geracao mais antiga do modelo de trabalho.",
        "Fast and affordable model for easier tasks." => "Rapido e economico, para tarefas mais simples.",
        "Fast and affordable agentic coding model." => "Rapido e economico, para codigo com agentes.",
        "Older coding model for complex work." => "Geracao anterior, para trabalho complexo em codigo.",
        "Older balanced model for straightforward work." => "Geracao anterior, equilibrado, para trabalho direto.",
        "Older fast and efficient model." => "Geracao anterior, rapido e eficiente.",
        "Legacy coding model." => "Modelo antigo de codigo.",
        other => other,
    }
    .to_string()
}

/// "GPT-6.1-Sol" -> "GPT-6.1 Sol", como o app do Codex mostra.
fn codex_label(display_name: &str) -> String {
    let rest = display_name.strip_prefix("GPT-").or_else(|| display_name.strip_prefix("gpt-"));
    if let Some(rest) = rest {
        let n = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
        if n > 0 && rest[n..].starts_with('-') {
            return format!("{}{} {}", &display_name[..4], &rest[..n], &rest[n + 1..]);
        }
    }
    display_name.to_string()
}

/// Caminho do catalogo do Codex: `$CODEX_HOME/models_cache.json` ou `~/.codex/models_cache.json`.
fn codex_cache_path(home: &Path) -> PathBuf {
    match std::env::var_os("CODEX_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("models_cache.json"),
        _ => home.join(".codex").join("models_cache.json"),
    }
}

/// Catalogo local do Codex, na ordem do app (`priority`), sem os ocultos.
fn codex_cached_models(home: &Path) -> Vec<EngineModel> {
    let Ok(raw) = std::fs::read_to_string(codex_cache_path(home)) else { return vec![] };
    parse_codex_cache(&raw)
}

/// Consulta o catalogo pelo protocolo do proprio Codex quando o cache nao esta disponivel.
async fn codex_app_server_models(exe: &Path, opts: &DetectOptions) -> Vec<EngineModel> {
    let Ok(path) = std::env::join_paths(&opts.dirs) else { return vec![] };
    let Ok(mut child) = Command::new(exe)
        .arg("app-server")
        .env("HOME", &opts.home)
        .env("PATH", path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn() else { return vec![] };
    let Some(mut input) = child.stdin.take() else { return vec![] };
    let Some(output) = child.stdout.take() else { return vec![] };
    let mut lines = BufReader::new(output).lines();
    let exchange = async {
        if input.write_all(b"{\"id\":1,\"method\":\"initialize\",\"params\":{\"clientInfo\":{\"name\":\"dots\",\"version\":\"0.0.0\"}}}\n").await.is_err() {
            return vec![];
        }
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            match value.get("id").and_then(|id| id.as_u64()) {
                Some(1) if value.get("result").is_some() => {
                    if input.write_all(b"{\"method\":\"initialized\",\"params\":{}}\n{\"id\":2,\"method\":\"model/list\",\"params\":{\"includeHidden\":false,\"limit\":1000}}\n").await.is_err() {
                        return vec![];
                    }
                }
                Some(1) => return vec![],
                Some(2) => return parse_codex_app_server_models(&value),
                _ => {}
            }
        }
        vec![]
    };
    let models = tokio::time::timeout(opts.timeout, exchange).await.unwrap_or_default();
    let _ = child.kill().await;
    models
}

pub async fn codex_models(exe: &Path, opts: &DetectOptions) -> Vec<EngineModel> {
    let cached = codex_cached_models(&opts.home);
    if cached.is_empty() { codex_app_server_models(exe, opts).await } else { cached }
}

fn parse_codex_app_server_models(value: &serde_json::Value) -> Vec<EngineModel> {
    value.get("result").and_then(|result| result.get("data")).and_then(|data| data.as_array())
        .into_iter().flatten()
        .filter(|model| model.get("hidden").and_then(|hidden| hidden.as_bool()) != Some(true))
        .filter_map(|model| {
            let id = model.get("model")?.as_str()?.trim();
            if id.is_empty() { return None }
            let display = model.get("displayName").and_then(|name| name.as_str()).unwrap_or(id);
            let description = model.get("description").and_then(|value| value.as_str()).map(describe_codex);
            Some(EngineModel { id: id.to_string(), label: codex_label(display), description })
        })
        .collect()
}

pub fn parse_codex_cache(raw: &str) -> Vec<EngineModel> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else { return vec![] };
    let Some(models) = v.get("models").and_then(|m| m.as_array()) else { return vec![] };
    let mut rows: Vec<(i64, EngineModel)> = models
        .iter()
        .filter_map(|m| {
            let slug = m.get("slug")?.as_str()?;
            if m.get("visibility").and_then(|x| x.as_str()) == Some("hide") {
                return None;
            }
            let display = m.get("display_name").and_then(|x| x.as_str()).unwrap_or(slug);
            let description = m.get("description").and_then(|x| x.as_str()).map(describe_codex);
            let priority = m.get("priority").and_then(|x| x.as_i64()).unwrap_or(99);
            Some((priority, EngineModel { id: slug.to_string(), label: codex_label(display), description }))
        })
        .collect();
    rows.sort_by_key(|(p, _)| *p);
    rows.into_iter().map(|(_, m)| m).collect()
}

/// `agy models` consulta o servico; linhas sem TAB (ex.: "Fetching available models...") sao ignoradas.
pub async fn agy_models(exe: &Path, opts: &DetectOptions) -> Vec<EngineModel> {
    match run_command(exe, &["models"], &opts.home, &opts.dirs, opts.timeout).await {
        Ok(out) if out.exit_code == Some(0) => parse_agy_models(&out.stdout),
        _ => vec![],
    }
}

pub fn parse_agy_models(stdout: &str) -> Vec<EngineModel> {
    stdout
        .lines()
        .filter_map(|line| {
            let (id, label) = line.trim().split_once('\t')?;
            let id = id.trim();
            if id.is_empty() || id.contains(char::is_whitespace) {
                return None;
            }
            Some(EngineModel { id: id.to_string(), label: label.trim().to_string(), description: None })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_cache_is_sorted_translated_and_without_hidden() {
        let raw = r#"{"models":[
          {"slug":"gpt-6-astra","display_name":"GPT-6-Astra","description":"Frontier intelligence for the most demanding work.","visibility":"list","priority":2},
          {"slug":"gpt-6.1-sol","display_name":"GPT-6.1-Sol","description":"Latest workhorse model for coding and everyday work.","visibility":"list","priority":1},
          {"slug":"gpt-reserve","display_name":"GPT-Reserve","visibility":"hide","priority":4},
          {"slug":"codex-auto-review","display_name":"Codex Auto Review","visibility":"hide","priority":43},
          {"slug":"sem-prioridade","display_name":"Sem Prioridade","description":"Something new.","visibility":"list"}
        ]}"#;
        let list = parse_codex_cache(raw);
        assert_eq!(list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["gpt-6.1-sol", "gpt-6-astra", "sem-prioridade"]);
        assert_eq!(list[0].label, "GPT-6.1 Sol");
        assert_eq!(list[0].description.as_deref(), Some("Modelo de trabalho mais recente, para codigo e tarefas do dia a dia."));
        assert_eq!(list[1].label, "GPT-6 Astra");
        assert_eq!(list[2].description.as_deref(), Some("Something new."));
        assert!(parse_codex_cache("nao e json").is_empty());
        assert!(parse_codex_cache("{}").is_empty());
    }

    #[test]
    fn codex_label_keeps_names_without_variant() {
        assert_eq!(codex_label("GPT-Reserve"), "GPT-Reserve");
        assert_eq!(codex_label("Codex Auto Review"), "Codex Auto Review");
        assert_eq!(codex_label("GPT-5.6-Terra"), "GPT-5.6 Terra");
    }

    #[tokio::test]
    async fn codex_lists_models_without_a_cache_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("codex");
        std::fs::write(&exe, "#!/bin/sh\nread -r line || exit 1\nprintf '%s\\n' '{\"id\":1,\"result\":{}}'\nread -r line || exit 1\nread -r line || exit 1\nprintf '%s\\n' '{\"id\":2,\"result\":{\"data\":[{\"model\":\"gpt-5.6-terra\",\"displayName\":\"GPT-5.6-Terra\",\"description\":\"Modelo disponivel.\",\"hidden\":false},{\"model\":\"hidden\",\"hidden\":true}]}}'\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let opts = DetectOptions { home: dir.path().to_path_buf(), dirs: vec![dir.path().to_path_buf()], timeout: std::time::Duration::from_secs(2) };
        let models = codex_models(&exe, &opts).await;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-5.6-terra");
        assert_eq!(models[0].label, "GPT-5.6 Terra");
    }

    #[test]
    fn agy_output_ignores_status_lines() {
        let out = "Fetching available models...\ngemini-3.8-flash-high\tGemini 3.8 Flash (High)\nclaude-opus-5-5-low\tClaude Opus 5.5 (Low)\n\n";
        let list = parse_agy_models(out);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "gemini-3.8-flash-high");
        assert_eq!(list[0].label, "Gemini 3.8 Flash (High)");
        assert_eq!(list[1].label, "Claude Opus 5.5 (Low)");
        assert!(parse_agy_models("erro: faca login").is_empty());
    }
}
