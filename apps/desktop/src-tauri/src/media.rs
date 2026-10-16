//! Midia do compositor da conversa: ditado (audio -> texto pelo ajudante nativo `dots-transcribe`, neste Mac)
//! e miniatura de imagens anexadas. Copiado do ovrgit (src/main/transcribe.ts e src/shared/transcribe.ts).

use std::path::PathBuf;
use std::time::Duration;

use base64::Engine as _;
use dots_core::DotsError;

/// A primeira vez num idioma baixa o modelo de fala do sistema: por isso o prazo folgado.
const TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// WAV de 16 kHz mono: 40 MB da uns 20 minutos de fala.
const MAX_WAV_BYTES: usize = 40 * 1024 * 1024;
/// Idioma do ditado.
const LOCALE: &str = "pt-BR";
/// Miniatura so para imagens ate este tamanho.
const PREVIEW_MAX_BYTES: u64 = 25 * 1024 * 1024;

/// Sidecar: o Tauri coloca `dots-transcribe` ao lado do executavel principal.
fn transcriber() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.parent()?.join("dots-transcribe");
    p.is_file().then_some(p)
}

/// Le a saida do ajudante (uma linha JSON por vez): o texto transcrito ou o erro em linguagem de usuario.
pub fn read_transcribe_output(stdout: &str) -> Result<String, String> {
    let mut parts: Vec<String> = Vec::new();
    let mut done = false;
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        match v["type"].as_str() {
            Some("text") => {
                if let Some(t) = v["text"].as_str().filter(|t| !t.is_empty()) {
                    parts.push(t.to_string());
                }
            }
            Some("done") => done = true,
            Some("error") => {
                return Err(match v["code"].as_str().unwrap_or("") {
                    "unsupported-os" => "A transcricao de audio precisa do macOS 26 ou mais novo.".into(),
                    "unsupported-locale" => "Este Mac nao tem o reconhecimento de fala em portugues.".into(),
                    "unreadable-audio" => "Nao deu para ler o audio gravado.".into(),
                    _ => match v["detail"].as_str() {
                        Some(d) if !d.is_empty() => format!("A transcricao falhou: {}", d.chars().take(200).collect::<String>()),
                        _ => "A transcricao falhou.".into(),
                    },
                });
            }
            _ => {}
        }
    }
    if !done {
        return Err("A transcricao parou antes de terminar.".into());
    }
    // cada trecho ja vem com a pontuacao e os espacos dele
    Ok(parts.join("").split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Transcreve o WAV gravado no compositor (corpo bruto do invoke) em texto, neste computador.
#[tauri::command]
pub async fn transcribe_audio(request: tauri::ipc::Request<'_>) -> Result<String, DotsError> {
    let tauri::ipc::InvokeBody::Raw(wav) = request.body() else {
        return Err(DotsError::Validation("audio ausente".into()));
    };
    let bin = transcriber().ok_or_else(|| DotsError::Internal("Transcritor de audio nao encontrado neste app.".into()))?;
    if wav.is_empty() {
        return Err(DotsError::Validation("A gravacao ficou vazia.".into()));
    }
    if wav.len() > MAX_WAV_BYTES {
        return Err(DotsError::Validation("Gravacao longa demais para transcrever.".into()));
    }
    let dir = tempfile_dir()?;
    let file = dir.join("audio.wav");
    let result = async {
        tokio::fs::write(&file, wav).await.map_err(|e| DotsError::Io(e.to_string()))?;
        let child = tokio::process::Command::new(&bin)
            .arg("--input")
            .arg(&file)
            .arg("--locale")
            .arg(LOCALE)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| DotsError::Io(format!("executar transcritor: {e}")))?;
        let out = tokio::time::timeout(TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| DotsError::Internal("A transcricao demorou demais.".into()))?
            .map_err(|e| DotsError::Io(e.to_string()))?;
        let text = read_transcribe_output(&String::from_utf8_lossy(&out.stdout)).map_err(DotsError::Internal)?;
        if text.is_empty() {
            return Err(DotsError::Validation("Nao ouvi nenhuma fala na gravacao.".into()));
        }
        Ok(text)
    }
    .await;
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn tempfile_dir() -> Result<PathBuf, DotsError> {
    let dir = std::env::temp_dir().join(format!("dots-ditado-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    std::fs::create_dir_all(&dir).map_err(|e| DotsError::Io(e.to_string()))?;
    Ok(dir)
}

/// Miniatura de uma imagem anexada (data URL); `None` se nao for imagem ou for grande demais.
#[tauri::command]
pub async fn image_preview(path: String) -> Result<Option<String>, DotsError> {
    let p = PathBuf::from(&path);
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Ok(None),
    };
    let Ok(meta) = tokio::fs::metadata(&p).await else { return Ok(None) };
    if !meta.is_file() || meta.len() > PREVIEW_MAX_BYTES {
        return Ok(None);
    }
    let bytes = tokio::fs::read(&p).await.map_err(|e| DotsError::Io(e.to_string()))?;
    Ok(Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))))
}

#[cfg(test)]
mod tests {
    use super::read_transcribe_output;

    #[test]
    fn joins_text_and_requires_done() {
        let out = "{\"type\":\"status\",\"state\":\"preparing\"}\n{\"type\":\"text\",\"text\":\"Ola, \"}\n{\"type\":\"text\",\"text\":\"tudo  bem?\"}\n{\"type\":\"done\"}\n";
        assert_eq!(read_transcribe_output(out).unwrap(), "Ola, tudo bem?");
        assert!(read_transcribe_output("{\"type\":\"text\",\"text\":\"x\"}").is_err());
        assert_eq!(read_transcribe_output("{\"type\":\"error\",\"code\":\"unsupported-os\"}").unwrap_err(), "A transcricao de audio precisa do macOS 26 ou mais novo.");
    }
}
