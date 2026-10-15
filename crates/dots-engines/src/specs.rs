use dots_core::{EngineCapability, EngineKind};

/// Como verificar autenticacao de forma nao interativa.
#[derive(Debug, Clone, Copy)]
pub enum AuthProbe {
    /// `codex login status`: stdout contem "Logged in" ou "Not logged in".
    CodexLoginStatus,
    /// `claude auth status`: JSON com `loggedIn`.
    ClaudeAuthStatus,
    /// O CLI nao oferece comando; status permanece `Unknown`.
    None,
}

#[derive(Debug, Clone)]
pub struct EngineSpec {
    pub kind: EngineKind,
    pub executable: &'static str,
    pub version_args: &'static [&'static str],
    pub auth: AuthProbe,
    pub capabilities: &'static [(&'static str, &'static str, &'static str)],
    pub install_guidance: &'static str,
    pub auth_guidance: &'static str,
}

/// Capacidades verificadas em: codex-cli 0.162.1, claude 2.1.300, agy 1.3.5 (2026-10-15).
pub fn spec(kind: EngineKind) -> EngineSpec {
    match kind {
        EngineKind::Codex => EngineSpec {
            kind,
            executable: "codex",
            version_args: &["--version"],
            auth: AuthProbe::CodexLoginStatus,
            capabilities: &[
                ("non_interactive", "Execucao nao interativa", "codex exec"),
                ("json_stream", "Eventos JSONL no stdout", "codex exec --json"),
                ("working_dir", "Diretorio de trabalho", "codex exec -C <DIR>"),
                ("sandbox", "Politica de sandbox", "codex exec -s read-only|workspace-write"),
                ("no_approval", "Sem aprovacao interativa", "codex exec -a never"),
                ("resume", "Retomar sessao por id", "codex exec resume <id>"),
                ("last_message_file", "Ultima mensagem em arquivo", "codex exec -o <FILE>"),
                ("extra_dirs", "Diretorios adicionais", "codex exec --add-dir <DIR>"),
            ],
            install_guidance: "Instale o Codex CLI (npm i -g @openai/codex ou instalador oficial) e execute `codex login`.",
            auth_guidance: "Execute `codex login` em um terminal para autenticar.",
        },
        EngineKind::Claude => EngineSpec {
            kind,
            executable: "claude",
            version_args: &["--version"],
            auth: AuthProbe::ClaudeAuthStatus,
            capabilities: &[
                ("non_interactive", "Execucao nao interativa", "claude -p"),
                ("json_stream", "Streaming JSON", "claude -p --output-format stream-json"),
                ("permission_mode", "Modo de permissao", "claude --permission-mode <mode>"),
                ("tools_allowlist", "Lista de ferramentas", "claude --tools / --allowedTools"),
                ("extra_dirs", "Diretorios adicionais", "claude --add-dir <DIR>"),
                ("resume", "Retomar sessao por id", "claude --resume <id>"),
                ("session_id", "Definir id de sessao", "claude --session-id <uuid>"),
                ("budget", "Limite de custo", "claude --max-budget-usd <amount>"),
                ("system_prompt", "Prompt de sistema adicional", "claude --append-system-prompt"),
            ],
            install_guidance: "Instale o Claude Code (https://claude.com/claude-code) e execute `claude` para autenticar.",
            auth_guidance: "Execute `claude` em um terminal e conclua o login.",
        },
        EngineKind::Agy => EngineSpec {
            kind,
            executable: "agy",
            version_args: &["--version"],
            auth: AuthProbe::None,
            capabilities: &[
                ("non_interactive", "Execucao nao interativa", "agy --print"),
                ("json_stream", "Streaming JSON", "agy --output-format stream-json"),
                ("mode", "Modo de execucao", "agy --mode accept-edits|auto|plan"),
                ("sandbox", "Sandbox de terminal", "agy --sandbox"),
                ("extra_dirs", "Diretorios adicionais", "agy --add-dir <DIR>"),
                ("resume", "Retomar conversa por id", "agy --conversation <id>"),
                ("print_timeout", "Tempo limite no modo print", "agy --print-timeout"),
            ],
            install_guidance: "Instale o Antigravity CLI (agy) pelo aplicativo Antigravity ou instalador oficial.",
            auth_guidance: "O agy 1.3.5 nao expoe comando de verificacao de login; abra `agy` no terminal para conferir.",
        },
    }
}

pub fn capabilities(kind: EngineKind) -> Vec<EngineCapability> {
    spec(kind)
        .capabilities
        .iter()
        .map(|(id, label, evidence)| EngineCapability {
            id: id.to_string(),
            label: label.to_string(),
            evidence: evidence.to_string(),
        })
        .collect()
}
