use std::path::PathBuf;

use dots_core::RuntimeMode;

pub const HELP: &str = "dotsd - runtime local do DOTS

USO:
  dotsd [--data-dir <DIR>] [--mode launchd|detached|manual] [--stderr]
  dotsd mcp-gateway [--data-dir <DIR>]   Servidor MCP (stdio) usado pelos CLIs dos motores
  dotsd call <metodo> [<params-json>] [--data-dir <DIR>]   Chama um metodo IPC do runtime em execucao e imprime o JSON
  dotsd --version
  dotsd --help

OPCOES:
  --data-dir <DIR>   Diretorio de dados (padrao: $DOTS_DATA_DIR ou ~/Library/Application Support/DOTS)
  --mode <MODO>      Como o processo foi iniciado (informativo). Padrao: manual
  --stderr           Tambem escreve logs no stderr
  --version          Mostra a versao
  --help             Mostra esta ajuda";

#[derive(Debug)]
pub struct RunArgs {
    pub data_dir: Option<PathBuf>,
    pub mode: RuntimeMode,
    pub log_to_stderr: bool,
}

#[derive(Debug)]
pub enum Command {
    Run(RunArgs),
    /// `dotsd mcp-gateway`: servidor MCP por stdio para os CLIs dos motores.
    McpGateway { data_dir: Option<PathBuf> },
    /// `dotsd call <metodo> [params]`: chamada IPC avulsa (diagnostico e manutencao).
    Call { data_dir: Option<PathBuf>, method: String, params: String },
    Version,
    Help,
}

pub fn parse(args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut run = RunArgs { data_dir: None, mode: RuntimeMode::Manual, log_to_stderr: false };
    let mut it = args.peekable();
    if it.peek().map(|a| a == "call").unwrap_or(false) {
        it.next();
        let (mut data_dir, mut positional) = (None, vec![]);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--data-dir" => data_dir = Some(PathBuf::from(it.next().ok_or("--data-dir exige um valor")?)),
                other if other.starts_with("--data-dir=") => data_dir = Some(PathBuf::from(&other["--data-dir=".len()..])),
                other if other.starts_with("--") => return Err(format!("argumento desconhecido: {other}")),
                other => positional.push(other.to_string()),
            }
        }
        if data_dir.is_none() {
            data_dir = std::env::var_os("DOTS_DATA_DIR").map(PathBuf::from);
        }
        let mut pos = positional.into_iter();
        let method = pos.next().ok_or("call exige o nome do metodo")?;
        let params = pos.next().unwrap_or_else(|| "{}".to_string());
        return Ok(Command::Call { data_dir, method, params });
    }
    if it.peek().map(|a| a == "mcp-gateway").unwrap_or(false) {
        it.next();
        let mut data_dir = None;
        while let Some(a) = it.next() {
            match a.as_str() {
                "--data-dir" => data_dir = Some(PathBuf::from(it.next().ok_or("--data-dir exige um valor")?)),
                other if other.starts_with("--data-dir=") => data_dir = Some(PathBuf::from(&other["--data-dir=".len()..])),
                other => return Err(format!("argumento desconhecido: {other}")),
            }
        }
        if data_dir.is_none() {
            data_dir = std::env::var_os("DOTS_DATA_DIR").map(PathBuf::from);
        }
        return Ok(Command::McpGateway { data_dir });
    }
    while let Some(a) = it.next() {
        match a.as_str() {
            "--version" | "-V" => return Ok(Command::Version),
            "--help" | "-h" => return Ok(Command::Help),
            "--stderr" => run.log_to_stderr = true,
            "--data-dir" => {
                run.data_dir = Some(PathBuf::from(it.next().ok_or("--data-dir exige um valor")?));
            }
            "--mode" => {
                run.mode = match it.next().ok_or("--mode exige um valor")?.as_str() {
                    "launchd" => RuntimeMode::Launchd,
                    "detached" => RuntimeMode::Detached,
                    "manual" => RuntimeMode::Manual,
                    other => return Err(format!("modo invalido: {other}")),
                };
            }
            other if other.starts_with("--data-dir=") => {
                run.data_dir = Some(PathBuf::from(&other["--data-dir=".len()..]));
            }
            other => return Err(format!("argumento desconhecido: {other}")),
        }
    }
    if run.data_dir.is_none() {
        if let Some(env) = std::env::var_os("DOTS_DATA_DIR") {
            run.data_dir = Some(PathBuf::from(env));
        }
    }
    Ok(Command::Run(run))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_args() {
        let c = parse(["--data-dir", "/x", "--mode", "launchd", "--stderr"].iter().map(|s| s.to_string())).unwrap();
        match c {
            Command::Run(r) => {
                assert_eq!(r.data_dir.unwrap(), PathBuf::from("/x"));
                assert_eq!(r.mode, RuntimeMode::Launchd);
                assert!(r.log_to_stderr);
            }
            _ => panic!(),
        }
        assert!(matches!(parse(["--version".to_string()].into_iter()).unwrap(), Command::Version));
        match parse(["call", "dots.list", "{\"x\":1}", "--data-dir", "/d"].iter().map(|s| s.to_string())).unwrap() {
            Command::Call { data_dir, method, params } => {
                assert_eq!(data_dir.unwrap(), PathBuf::from("/d"));
                assert_eq!(method, "dots.list");
                assert_eq!(params, "{\"x\":1}");
            }
            _ => panic!(),
        }
        assert!(parse(["call".to_string()].into_iter()).is_err());
        assert!(parse(["--bogus".to_string()].into_iter()).is_err());
    }
}
