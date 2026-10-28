//! Contratacao: o gerente cria um funcionario novo (nome e funcao escolhidos por ele) na propria equipe.
//! O gerente nunca executa o trabalho; sem funcionario para a tarefa, ele contrata. O limite de funcionarios
//! por gerente e uma configuracao do usuario (`teams.max_employees_per_manager`).

use dots_core::{DotCreate, DotRole, DotsError, DotsResult, TeamCreate, ToolPolicyUpdate};
use serde_json::json;

use crate::policy::SharedContext;
use crate::flexi_team::subspecialty_template_id;
use crate::service::RuntimeService;
use crate::tools::ToolOutput;

pub const MAX_EMPLOYEES_SETTING: &str = "teams.max_employees_per_manager";
pub const DEFAULT_MAX_EMPLOYEES: u32 = 5;
/// Teto do limite configuravel (protege contra um valor absurdo gravado a mao).
const MAX_EMPLOYEES_CEILING: u32 = 50;

impl RuntimeService {
    /// Limite de funcionarios por gerente (configuracao do usuario; padrao 5).
    pub async fn max_employees_per_manager(&self) -> u32 {
        self.storage
            .call(|s| s.settings().get(MAX_EMPLOYEES_SETTING))
            .await
            .ok()
            .flatten()
            .and_then(|s| s.value.as_u64())
            .map(|v| (v as u32).clamp(1, MAX_EMPLOYEES_CEILING))
            .unwrap_or(DEFAULT_MAX_EMPLOYEES)
    }

    /// `dots_hire_employee`: so gerente (supervisor) contrata, dentro do limite. O funcionario nasce na equipe do
    /// gerente com o mesmo assistente e modelo, as mesmas pastas, o mesmo app conectado e as mesmas permissoes do
    /// gerente (nunca mais que ele), sem poder delegar.
    pub async fn hire_employee(&self, ctx: &SharedContext, name: &str, role: &str, instructions: &str, subspecialty: &str) -> DotsResult<ToolOutput> {
        let (name, role, instructions, subspecialty) = (name.trim().to_string(), role.trim().to_string(), instructions.trim().to_string(), subspecialty.trim().to_string());
        if name.is_empty() || role.is_empty() {
            return Ok(ToolOutput::err("informe o nome do funcionario (name) e a funcao dele (role)".into()));
        }
        let manager = self.storage.call({ let d = ctx.dot_id.clone(); move |s| s.dots().get(&d) }).await?;
        let team = self.storage.call({ let d = manager.id.clone(); move |s| s.teams().managed_by(&d) }).await?;
        if team.is_none() && manager.role != DotRole::Supervisor {
            return Ok(ToolOutput::err("so um gerente contrata funcionarios".into()));
        }
        let linked_sub = if subspecialty.is_empty() {
            None
        } else {
            let integration = self.storage.call({ let d = manager.id.clone(); move |s| s.apps().integration(&d, crate::flexi::KIND) }).await?
                .ok_or_else(|| DotsError::Validation("gerente sem conexao Flexi".into()))?;
            let manager_template = integration.role_template.as_deref()
                .ok_or_else(|| DotsError::Validation("gerente sem especialidade configurada no Flexi".into()))?;
            let templates = self.flexi_fetch_templates(Some(&integration.connection_id)).await?;
            let sub = templates.iter().find(|template| template.id == manager_template)
                .and_then(|template| template.subspecialties.iter().find(|sub| sub.id == subspecialty))
                .ok_or_else(|| DotsError::Validation(format!("subespecialidade {subspecialty} nao encontrada para este gerente")))?;
            Some((subspecialty_template_id(manager_template, &sub.id), sub.instructions.clone()))
        };
        let limit = self.max_employees_per_manager().await;
        if let Some(t) = &team {
            if t.member_ids.len() as u32 >= limit {
                return Ok(ToolOutput::err(format!(
                    "limite de {limit} funcionario(s) por gerente atingido na equipe \"{}\". Delegue a quem ja esta na equipe, demita alguem (dots_dismiss_employee) ou peca ao usuario para aumentar o limite em Configuracoes.",
                    t.name
                )));
            }
        }
        let taken = self.storage.call({ let n = name.clone(); move |s| Ok(s.dots().list(true)?.into_iter().any(|d| d.name.eq_ignore_ascii_case(&n))) }).await?;
        if taken {
            return Ok(ToolOutput::err(format!("ja existe um DOT chamado '{name}'; escolha outro nome")));
        }

        // Gerente sem equipe (supervisor avulso): a equipe nasce com ele.
        let team = match team {
            Some(t) => t,
            None => {
                let input = TeamCreate { name: format!("Equipe de {}", manager.name), description: String::new(), manager_dot_id: Some(manager.id.clone()), member_ids: vec![] };
                let t = self.storage.call(move |s| s.teams().create(input)).await?;
                self.sync_team_public(&t).await?;
                t
            }
        };

        let input = DotCreate {
            name: name.clone(),
            description: role.clone(),
            objective: role.clone(),
            instructions: linked_sub.as_ref().map(|(_, text)| text.clone())
                .unwrap_or_else(|| if instructions.is_empty() { role.clone() } else { instructions.clone() }),
            primary_engine: manager.primary_engine,
            model: manager.model.clone(),
            workspace_path: None,
            limits: Some(manager.limits.clone()),
            role: Some(DotRole::Worker),
        };
        let ws_root = self.layout.workspaces_dir.clone();
        let (mid, tid, linked_template) = (manager.id.clone(), team.id.clone(), linked_sub.map(|(id, _)| id));
        let actor = format!("dot:{}", manager.id);
        let (hired, team) = self
            .storage
            .call(move |s| {
                let created = s.dots().create(&actor, input, None)?;
                let ws = ws_root.join(&created.id);
                std::fs::create_dir_all(&ws)?;
                let hired = s.dots().update(&actor, &created.id, dots_core::DotUpdate { workspace_path: Some(Some(ws.to_string_lossy().to_string())), ..Default::default() })?;
                // Mesmas permissoes do gerente, sem delegacao.
                let p = s.policies().get(&mid)?;
                s.policies().update(&hired.id, ToolPolicyUpdate {
                    auto_approve_level: Some(p.auto_approve_level),
                    allowed_commands: Some(p.allowed_commands),
                    allow_network: Some(p.allow_network),
                    max_file_bytes: Some(p.max_file_bytes),
                    engine_native_write: Some(p.engine_native_write),
                    can_delegate_to: Some(vec![]),
                    ..Default::default()
                })?;
                // Mesmas pastas extras do gerente.
                for w in s.dots().workspaces(&mid)?.into_iter().filter(|w| !w.is_primary) {
                    s.dots().add_workspace(&actor, &hired.id, &w.path, w.can_read, w.can_write)?;
                }
                // Mesmo app conectado (ex.: Flexi), na mesma area de trabalho.
                if let Some(i) = s.apps().integration(&mid, crate::flexi::KIND)? {
                    s.apps().set_integration(&hired.id, crate::flexi::KIND, Some(&i.connection_id), linked_template.as_deref(), i.work_group.as_deref())?;
                }
                let team = s.teams().add_member(&tid, &hired.id)?;
                s.audit().record(&actor, "dot.hire", "dot", Some(&hired.id), json!({ "name": hired.name, "role": hired.description, "team": team.name }))?;
                Ok((hired, team))
            })
            .await?;
        self.sync_team_public(&team).await?;
        self.events.emit("teams.changed", json!({ "id": team.id, "action": "hire" }));
        self.events.emit("dots.changed", json!({ "id": hired.id, "action": "hire" }));
        tracing::info!(dot = %hired.id, manager = %manager.id, "funcionario contratado pelo gerente");
        Ok(ToolOutput::ok(format!(
            "'{}' foi contratado como \"{}\" na equipe \"{}\" ({} de {} funcionario(s)). Ele comeca sem memoria: delegue o trabalho com dots_delegate(to=\"{}\") explicando o pedido e o contexto.",
            hired.name, role, team.name, team.member_ids.len(), limit, hired.name
        )))
    }
}
