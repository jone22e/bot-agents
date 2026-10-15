//! Lista de modelos que cada CLI informa (mesma origem usada pelo ovrgit):
//! - Codex: `~/.codex/models_cache.json`, que o proprio CLI baixa e mantem.
//! - AGY: saida de `agy models` (uma linha `id<TAB>nome` por modelo).
//! O Claude Code nao lista modelos; a lista dele fica na interface.

use std::path::{Path, PathBuf};

use dots_core::EngineModel;

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

/// Catalogo do Codex, na ordem do app (`priority`), sem os ocultos (`visibility: "hide"`).
pub fn codex_models(home: &Path) -> Vec<EngineModel> {
    let Ok(raw) = std::fs::read_to_string(codex_cache_path(home)) else { return vec![] };
    parse_codex_cache(&raw)
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
