//! Pausa e retomada de funcionarios pelo gerente: o gerente pode tirar de acao (e devolver) um funcionario da
//! propria equipe sem passar pelo usuario. Pausado, o funcionario nao recebe trabalho novo; o que ja estava na
//! fila espera ate ele ser retomado (pelo gerente ou pelo usuario).

use dots_core::{Dot, DotStatus, DotsResult, Team};
use serde_json::json;

use crate::policy::SharedContext;
use crate::service::RuntimeService;
use crate::tools::ToolOutput;

impl RuntimeService {
    /// Confere que quem chama gerencia uma equipe e que o alvo e funcionario dela. Devolve (equipe, funcionario)
    /// ou a mensagem de erro para o DOT.
    async fn employee_of_caller(&self, ctx: &SharedContext, who: &str) -> DotsResult<Result<(Team, Dot), String>> {
        let manager_id = ctx.dot_id.clone();
        let Some(team) = self.storage.call({ let m = manager_id.clone(); move |s| s.teams().managed_by(&m) }).await? else {
            return Ok(Err("voce nao gerencia nenhuma equipe; so o gerente pausa ou retoma funcionarios da propria equipe".into()));
        };
        let who_s = who.trim().to_string();
        if who_s.is_empty() {
            return Ok(Err("informe o funcionario (dot): nome ou id".into()));
        }
        let members = team.member_ids.clone();
        let target = self.storage.call(move |s| {
            let all = s.dots().list(true)?;
            Ok(all.into_iter().find(|d| members.contains(&d.id) && (d.id == who_s || d.name.eq_ignore_ascii_case(&who_s))))
        }).await?;
        let Some(target) = target else {
            return Ok(Err(format!("'{who}' nao e funcionario da equipe \"{}\"; use dots_list_team para ver quem esta nela", team.name)));
        };
        if target.id == manager_id {
            return Ok(Err("um gerente nao pausa nem retoma a si mesmo".into()));
        }
        Ok(Ok((team, target)))
    }

    /// `dots_pause_employee` / `dots_resume_employee`: muda o estado de um funcionario da equipe do gerente.
    pub async fn set_employee_status(&self, ctx: &SharedContext, who: &str, status: DotStatus, reason: &str) -> DotsResult<ToolOutput> {
        let (team, target) = match self.employee_of_caller(ctx, who).await? {
            Ok(x) => x,
            Err(msg) => return Ok(ToolOutput::err(msg)),
        };
        let verb = if status == DotStatus::Paused { "pausado" } else { "retomado" };
        if target.status == status {
            return Ok(ToolOutput::ok(format!("'{}' ja estava {verb}.", target.name)));
        }
        if target.status == DotStatus::Archived {
            return Ok(ToolOutput::err(format!("'{}' esta arquivado; so o usuario pode reativa-lo", target.name)));
        }
        let actor = format!("dot:{}", ctx.dot_id);
        let (tid, tname, reason) = (target.id.clone(), team.name.clone(), reason.trim().to_string());
        let updated = self.storage.call(move |s| {
            let d = s.dots().set_status(&actor, &tid, status)?;
            s.audit().record(&actor, if status == DotStatus::Paused { "dot.pause" } else { "dot.resume" }, "dot", Some(&tid), json!({ "dot_name": d.name, "team": tname, "reason": reason }))?;
            Ok(d)
        }).await?;
        self.events.emit("dots.changed", json!({ "id": updated.id, "action": status.as_str() }));
        tracing::info!(dot = %updated.id, manager = %ctx.dot_id, status = status.as_str(), "funcionario {verb} pelo gerente");
        Ok(ToolOutput::ok(if status == DotStatus::Paused {
            format!("'{}' foi pausado: nao recebe trabalho novo e o que estava na fila espera. Retome com dots_resume_employee quando quiser que ele volte.", updated.name)
        } else {
            format!("'{}' foi retomado e volta a receber trabalho; o que estava na fila dele segue.", updated.name)
        }))
    }
}
