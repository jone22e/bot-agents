//! Equipes: mantem a politica de delegacao e o papel (gerente = supervisor) em sincronia com a composicao da equipe.

use dots_core::{DotRole, DotTeamInfo, DotsResult, Team, TeamCreate, TeamUpdate, ToolPolicyUpdate};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::service::RuntimeService;

#[derive(Deserialize)]
struct IdParams { id: String }
#[derive(Deserialize)]
struct TeamUpdateParams { id: String, patch: TeamUpdate }
#[derive(Deserialize)]
struct ManagerParams { id: String, #[serde(default)] dot_id: Option<String> }
#[derive(Deserialize)]
struct MemberParams { id: String, dot_id: String }
#[derive(Deserialize)]
struct DotParams { dot_id: String }

impl RuntimeService {
    /// O gerente pode delegar exatamente aos funcionarios da equipe e vira supervisor.
    async fn sync_team(&self, team: &Team) -> DotsResult<()> {
        if let Some(m) = team.manager_dot_id.clone() {
            let members = team.member_ids.clone();
            self.storage
                .call(move |s| {
                    s.policies().update(&m, ToolPolicyUpdate { can_delegate_to: Some(members), max_delegation_depth: Some(3), ..Default::default() })?;
                    s.dots().update("system", &m, dots_core::DotUpdate { role: Some(DotRole::Supervisor), ..Default::default() })?;
                    Ok(())
                })
                .await?;
        }
        Ok(())
    }

    /// Quem deixou de ser gerente volta a funcionario e nao delega a ninguem.
    async fn demote(&self, dot_id: &str) -> DotsResult<()> {
        let d = dot_id.to_string();
        self.storage
            .call(move |s| {
                if s.teams().managed_by(&d)?.is_some() { return Ok(()); }
                s.policies().update(&d, ToolPolicyUpdate { can_delegate_to: Some(vec![]), ..Default::default() })?;
                s.dots().update("system", &d, dots_core::DotUpdate { role: Some(DotRole::Worker), ..Default::default() })?;
                Ok(())
            })
            .await
    }

    pub async fn sync_team_public(&self, team: &Team) -> DotsResult<()> {
        self.sync_team(team).await
    }

    fn notify_teams(&self, team_id: Option<&str>) {
        self.events.emit("teams.changed", json!({ "id": team_id }));
        self.events.emit("dots.changed", json!({ "action": "team" }));
    }

    pub async fn handle_teams(&self, method: &str, params: Value) -> DotsResult<Option<Value>> {
        use crate::service::parse_params as p;
        Ok(Some(match method {
            "teams.list" => json!(self.storage.call(|s| s.teams().list()).await?),
            "teams.get" => { let IdParams { id } = p(params)?; json!(self.storage.call(move |s| s.teams().get(&id)).await?) }
            "teams.create" => {
                let input: TeamCreate = p(params)?;
                let t = self.storage.call(move |s| s.teams().create(input)).await?;
                self.sync_team(&t).await?;
                self.audit("team.create", "team", Some(&t.id), json!({ "name": t.name, "manager": t.manager_dot_id, "members": t.member_ids.len() })).await?;
                self.notify_teams(Some(&t.id));
                json!(t)
            }
            "teams.update" => {
                let TeamUpdateParams { id, patch } = p(params)?;
                let t = self.storage.call({ let id = id.clone(); move |s| s.teams().update(&id, patch) }).await?;
                self.audit("team.update", "team", Some(&id), json!({ "name": t.name })).await?;
                self.notify_teams(Some(&id));
                json!(t)
            }
            "teams.delete" => {
                let IdParams { id } = p(params)?;
                let t = self.storage.call({ let id = id.clone(); move |s| s.teams().delete(&id) }).await?;
                if let Some(m) = t.manager_dot_id.as_deref() { self.demote(m).await?; }
                self.audit("team.delete", "team", Some(&id), json!({ "name": t.name })).await?;
                self.notify_teams(Some(&id));
                json!({ "ok": true })
            }
            "teams.set_manager" => {
                let ManagerParams { id, dot_id } = p(params)?;
                let before = self.storage.call({ let id = id.clone(); move |s| s.teams().get(&id) }).await?;
                let t = self.storage.call({ let (id, d) = (id.clone(), dot_id.clone()); move |s| s.teams().set_manager(&id, d.as_deref()) }).await?;
                if let Some(old) = before.manager_dot_id.as_deref().filter(|o| Some(*o) != t.manager_dot_id.as_deref()) { self.demote(old).await?; }
                self.sync_team(&t).await?;
                self.audit("team.set_manager", "team", Some(&id), json!({ "manager": t.manager_dot_id })).await?;
                self.notify_teams(Some(&id));
                json!(t)
            }
            "teams.add_member" => {
                let MemberParams { id, dot_id } = p(params)?;
                let previous = self.storage.call({ let d = dot_id.clone(); move |s| s.teams().member_of(&d) }).await?;
                let t = self.storage.call({ let (id, d) = (id.clone(), dot_id.clone()); move |s| s.teams().add_member(&id, &d) }).await?;
                if let Some(prev) = previous.filter(|p| p.id != t.id) {
                    let refreshed = self.storage.call(move |s| s.teams().get(&prev.id)).await?;
                    self.sync_team(&refreshed).await?;
                }
                self.sync_team(&t).await?;
                self.audit("team.add_member", "team", Some(&id), json!({ "dot_id": dot_id })).await?;
                self.notify_teams(Some(&id));
                json!(t)
            }
            "teams.remove_member" => {
                let DotParams { dot_id } = p(params)?;
                let t = self.storage.call({ let d = dot_id.clone(); move |s| s.teams().remove_member(&d) }).await?;
                if let Some(t) = &t { self.sync_team(t).await?; }
                self.audit("team.remove_member", "dot", Some(&dot_id), json!({ "team": t.as_ref().map(|t| t.id.clone()) })).await?;
                self.notify_teams(t.as_ref().map(|t| t.id.as_str()));
                json!(t)
            }
            "dots.team" => {
                let DotParams { dot_id } = p(params)?;
                let info = self.storage.call(move |s| Ok(DotTeamInfo { member_of: s.teams().member_of(&dot_id)?, manages: s.teams().managed_by(&dot_id)? })).await?;
                json!(info)
            }
            _ => return Ok(None),
        }))
    }
}
