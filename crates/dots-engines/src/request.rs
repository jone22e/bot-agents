use std::path::PathBuf;
use std::time::Duration;

use dots_core::EngineKind;

/// Como o CLI deve iniciar o servidor MCP do DOTS (gateway de ferramentas).
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub server_name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Pedido de execucao normalizado, independente do motor.
#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub execution_id: String,
    pub engine: EngineKind,
    pub executable: PathBuf,
    /// Modelo a passar ao CLI; `None` deixa o padrao do proprio CLI.
    pub model: Option<String>,
    pub workspace: PathBuf,
    /// Diretorios adicionais autorizados (leitura/escrita conforme politica do CLI).
    pub extra_dirs: Vec<PathBuf>,
    pub prompt: String,
    /// Identidade do DOT, regras e memorias relevantes.
    pub system_instructions: String,
    /// Id de sessao do motor para retomar (thread/session/conversation).
    pub resume_session: Option<String>,
    /// Id de sessao a fixar na criacao (Claude aceita `--session-id`).
    pub new_session_id: Option<String>,
    /// Permite ferramentas nativas de escrita do CLI (sandbox do proprio CLI).
    pub native_write: bool,
    pub network_allowed: bool,
    pub gateway: Option<GatewayConfig>,
    pub timeout: Duration,
    pub max_turns: Option<u32>,
    pub max_budget_usd: Option<f64>,
    pub home: PathBuf,
    pub path_dirs: Vec<PathBuf>,
    /// Variaveis extras (ex.: DOTS_EXECUTION_TOKEN).
    pub env: Vec<(String, String)>,
}

/// Comando pronto para spawn.
#[derive(Debug, Clone)]
pub struct PreparedCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    /// Conteudo enviado ao stdin (prompt) e depois fechado.
    pub stdin: Option<String>,
}

impl ExecutionRequest {
    /// Prompt completo para motores sem flag de system prompt.
    pub fn prompt_with_instructions(&self) -> String {
        if self.system_instructions.trim().is_empty() {
            return self.prompt.clone();
        }
        format!(
            "<dot_instructions>\n{}\n</dot_instructions>\n\n{}",
            self.system_instructions.trim(),
            self.prompt
        )
    }
}
