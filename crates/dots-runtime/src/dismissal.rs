//! Demissao: o gerente demite um funcionario da equipe por um motivo. O registro (motivo + licao) fica
//! permanente e entra nas instrucoes de toda a equipe; o DOT demitido e excluido (memoria, conversa e trabalhos
//! juntos) e o gerente contrata um substituto com `dots_hire_employee`.

use dots_core::automation::event_types as et;
use dots_core::{Dot, DotsResult, TaskStatus, Team};
use dots_storage::repos::{DismissalInput, TaskFilter};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::policy::SharedContext;
use crate::service::RuntimeService;
use crate::tools::ToolOutput;

#[derive(Deserialize, Default)]
struct ListParams {
    #[serde(default)]
    team_id: Option<String>,
    #[serde(default)]
    dot_id: Option<String>,
    #[serde(default)]
    manager_dot_id: Option<String>,
    #[serde(default)]
    limit: u32,
}

impl RuntimeService {
    /// Confere, antes de pedir aprovacao, que quem chama gerencia uma equipe e que o alvo e funcionario dela.
    /// Devolve (equipe, funcionario) ou a mensagem de erro para o DOT.
    pub async fn dismissal_precheck(&self, ctx: &SharedContext, who: &str, reason: &str) -> DotsResult<Result<(Team, Dot), String>> {
        if reason.trim().is_empty() {
            return Ok(Err("informe o motivo da demissao".into()));
        }
        let manager_id = ctx.dot_id.clone();
        let Some(team) = self.storage.call({ let m = manager_id.clone(); move |s| s.teams().managed_by(&m) }).await? else {
            return Ok(Err("voce nao gerencia nenhuma equipe; so o gerente demite funcionarios da propria equipe".into()));
        };
        let who_s = who.trim().to_string();
        let members = team.member_ids.clone();
        let target = self.storage.call(move |s| {
            let all = s.dots().list(true)?;
            Ok(all.into_iter().find(|d| members.contains(&d.id) && (d.id == who_s || d.name.eq_ignore_ascii_case(&who_s))))
        }).await?;
        let Some(target) = target else {
            return Ok(Err(format!("'{who}' nao e funcionario da equipe \"{}\"; use dots_list_team para ver quem esta nela", team.name)));
        };
        if target.id == manager_id {
            return Ok(Err("um gerente nao demite a si mesmo".into()));
        }
        Ok(Ok((team, target)))
    }

    /// `dots_dismiss_employee`: so o gerente da equipe demite, e so funcionarios dela (ja aprovado pelo usuario).
    pub async fn dismiss_employee(&self, ctx: &SharedContext, who: &str, reason: &str, lesson: &str) -> DotsResult<ToolOutput> {
        let reason = reason.trim();
        let manager_id = ctx.dot_id.clone();
        let (team, target) = match self.dismissal_precheck(ctx, who, reason).await? {
            Ok(x) => x,
            Err(msg) => return Ok(ToolOutput::err(msg)),
        };
        let lesson = if lesson.trim().is_empty() { reason.to_string() } else { lesson.trim().to_string() };

        // Trabalhos em andamento do demitido param (em cascata, com os que ele delegou) antes da exclusao; o
        // encerramento devolve ao gerente o resultado das delegacoes que estavam com ele.
        let filter = TaskFilter { dot_id: Some(target.id.clone()), status: Some(vec![TaskStatus::Queued, TaskStatus::Running, TaskStatus::WaitingApproval, TaskStatus::WaitingUser, TaskStatus::WaitingExternal, TaskStatus::Paused]), limit: 500, ..Default::default() };
        let active = self.storage.call({ let f = filter.clone(); move |s| s.tasks().list(&f) }).await?;
        for t in &active {
            if let Err(e) = self.cancel_task(&t.id).await {
                tracing::warn!(task = %t.id, error = %e, "demissao: falha ao cancelar trabalho do demitido");
            }
        }
        if !active.is_empty() {
            let start = std::time::Instant::now();
            while start.elapsed() < std::time::Duration::from_secs(8) {
                let left = self.storage.call({ let f = filter.clone(); move |s| s.tasks().list(&f) }).await?;
                if left.is_empty() { break; }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }

        // O registro nasce antes da exclusao (o vinculo com o DOT vira nulo depois; nome, motivo e licao ficam).
        // A exclusao leva memoria, conversa, trabalhos e o lugar na equipe junto (em cascata).
        let (did, dname, mid, mname, tid, tname, r, l) = (target.id.clone(), target.name.clone(), manager_id.clone(), ctx.dot_name.clone(), team.id.clone(), team.name.clone(), reason.to_string(), lesson.clone());
        let (dismissal, wiped, team) = self.storage.call(move |s| {
            let wiped = s.memories().delete_all_for_dot(&did)?;
            let d = s.dismissals().create(DismissalInput { team_id: Some(&tid), team_name: &tname, manager_dot_id: &mid, manager_name: &mname, dot_id: &did, dot_name: &dname, reason: &r, lesson: &l, memories_wiped: wiped })?;
            s.dots().delete(&format!("dot:{mid}"), &did)?;
            s.audit().record(&format!("dot:{mid}"), "dot.dismiss", "dot", Some(&did), json!({ "dot_name": dname, "manager": mname, "team": tname, "reason": r, "memories_wiped": wiped, "deleted": true }))?;
            Ok((d, wiped, s.teams().get(&tid)?))
        }).await?;
        self.browsers.close(&target.id).await;
        let _ = std::fs::remove_dir_all(self.layout.browsers_dir.join(&target.id));
        self.sync_team_public(&team).await?;

        let _ = self.emit_event(et::DOT_DISMISSED, &format!("dot:{manager_id}"), json!({ "dismissal_id": dismissal.id, "dot_id": target.id, "dot_name": target.name, "manager_dot_id": manager_id, "team_id": team.id, "reason": reason, "lesson": lesson }), None, Some(format!("dismissal:{}", dismissal.id)), 0).await;
        self.events.emit("memory.changed", json!({ "dot_id": target.id }));
        self.events.emit("teams.changed", json!({ "id": team.id, "action": "dismissal" }));
        self.events.emit("dots.changed", json!({ "id": target.id, "action": "delete" }));
        tracing::info!(dot = %target.id, manager = %manager_id, wiped, "funcionario demitido e excluido pelo gerente");
        let limit = self.max_employees_per_manager().await;
        Ok(ToolOutput::ok(format!(
            "'{}' foi demitido e excluido (com {} memoria(s) e a conversa dele). A equipe \"{}\" ficou com {} de {} funcionario(s). Agora contrate um substituto com dots_hire_employee (nome novo e a funcao) e delegue a ele de novo o trabalho, com instrucoes que evitem o erro. Motivo e licao ficam registrados e entram nas instrucoes de todos os funcionarios da equipe, inclusive do novo.",
            target.name, wiped, team.name, team.member_ids.len(), limit
        )))
    }

    pub async fn handle_dismissals(&self, method: &str, params: Value) -> DotsResult<Option<Value>> {
        use crate::service::parse_params as p;
        Ok(Some(match method {
            "dismissals.list" => {
                let ListParams { team_id, dot_id, manager_dot_id, limit } = p(params).unwrap_or_default();
                json!(self.storage.call(move |s| match (team_id, dot_id, manager_dot_id) {
                    (Some(t), _, _) => s.dismissals().for_team(&t, limit),
                    (_, Some(d), _) => s.dismissals().for_dot(&d, limit),
                    (_, _, Some(m)) => s.dismissals().by_manager(&m, limit),
                    _ => s.dismissals().recent(limit),
                }).await?)
            }
            _ => return Ok(None),
        }))
    }
}
