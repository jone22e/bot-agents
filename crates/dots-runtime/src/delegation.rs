//! Delegacao entre DOTs, Supervisor, perguntas ao usuario e continuacoes.

use dots_core::automation::event_types as et;
use dots_core::{ApprovalKind, ApprovalStatus, DelegationStatus, DotStatus, DotsResult, Task, TaskCreate, TaskKind, TaskStatus};
use serde_json::json;

use crate::policy::SharedContext;
use crate::service::RuntimeService;
use crate::tools::ToolOutput;

impl RuntimeService {
    /// `dots_delegate`: cria a tarefa no DOT alvo e registra a delegacao. Nao bloqueia.
    pub async fn delegate(&self, ctx: &SharedContext, to: &str, request: &str, context: Option<&str>) -> DotsResult<ToolOutput> {
        let request = request.trim();
        if request.is_empty() {
            return Ok(ToolOutput::err("o pedido da delegacao esta vazio".into()));
        }
        let policy = ctx.policy.clone();
        if policy.can_delegate_to.is_empty() {
            return Ok(ToolOutput::err("este DOT nao tem permissao para delegar (configure a equipe em Permissoes)".into()));
        }
        let task = self.storage.call({ let t = ctx.task_id.clone(); move |st| st.tasks().get(&t) }).await?;
        let depth = task.delegation_chain.len() as u32;
        if depth >= policy.max_delegation_depth {
            return Ok(ToolOutput::err(format!("profundidade maxima de delegacao ({}) atingida", policy.max_delegation_depth)));
        }
        let to_s = to.trim().to_string();
        let target = self.storage.call(move |st| {
            let all = st.dots().list(false)?;
            Ok(all.into_iter().find(|d| d.id == to_s || d.name.eq_ignore_ascii_case(&to_s)))
        }).await?;
        let Some(target) = target else {
            return Ok(ToolOutput::err(format!("DOT '{to}' nao encontrado; use dots_list_team para ver a equipe")));
        };
        if target.id == ctx.dot_id {
            return Ok(ToolOutput::err("um DOT nao delega para si mesmo".into()));
        }
        if !(policy.can_delegate_to.iter().any(|x| x == "*" || *x == target.id)) {
            return Ok(ToolOutput::err(format!("delegar para '{}' nao esta na lista permitida deste DOT", target.name)));
        }
        if target.status != DotStatus::Active {
            return Ok(ToolOutput::err(format!("DOT '{}' esta {}", target.name, target.status.as_str())));
        }
        if task.delegation_chain.contains(&target.id) || target.id == task.dot_id {
            return Ok(ToolOutput::err(format!("ciclo de delegacao: '{}' ja esta na cadeia desta tarefa", target.name)));
        }
        let correlation = task.correlation_id.clone().unwrap_or_else(|| task.id.clone());
        let mut chain = task.delegation_chain.clone();
        chain.push(ctx.dot_id.clone());
        let prompt = match context.filter(|c| !c.trim().is_empty()) {
            Some(c) => format!("{request}\n\n<context_from_requester>\n{}\n</context_from_requester>", c.trim()),
            None => request.to_string(),
        };
        let child = self
            .create_task(TaskCreate {
                dot_id: target.id.clone(),
                prompt,
                title: Some(format!("Pedido de {}: {}", ctx.dot_name, request.chars().take(60).collect::<String>())),
                kind: Some(TaskKind::Task),
                origin: Some("delegation:pending".into()),
                parent_task_id: Some(task.id.clone()),
                delegation_chain: chain,
                correlation_id: Some(correlation.clone()),
                ..Default::default()
            })
            .await?;
        let (from, to_id, parent, childid, corr, req) = (ctx.dot_id.clone(), target.id.clone(), task.id.clone(), child.id.clone(), correlation.clone(), request.to_string());
        let d = self.storage.call(move |st| {
            let d = st.delegations().create(&from, &to_id, &parent, &childid, &corr, &req)?;
            st.tasks().set_origin(&childid, &format!("delegation:{}", d.id))?;
            Ok(d)
        }).await?;
        let _ = self.emit_event(et::DOT_TASK_REQUESTED, &format!("dot:{}", ctx.dot_id), json!({ "delegation_id": d.id, "from_dot_id": ctx.dot_id, "to_dot_id": target.id, "task_id": child.id, "correlation_id": correlation }), Some(correlation), None, 0).await;
        self.events.emit("delegation.changed", json!({ "id": d.id, "status": "pending" }));
        Ok(ToolOutput::ok(format!(
            "Delegacao {} criada para '{}'. Ela roda em segundo plano; termine seu turno resumindo o que esta aguardando. Quando concluir, voce recebera uma nova tarefa com o resultado.",
            d.id, target.name
        )))
    }

    pub async fn delegation_status(&self, ctx: &SharedContext, id: Option<&str>) -> DotsResult<ToolOutput> {
        let list = match id {
            Some(i) => vec![self.storage.call({ let i = i.to_string(); move |st| st.delegations().get(&i) }).await?],
            None => self.storage.call({ let t = ctx.task_id.clone(); move |st| st.delegations().for_parent(&t) }).await?,
        };
        if list.is_empty() {
            return Ok(ToolOutput::ok("nenhuma delegacao nesta tarefa".into()));
        }
        Ok(ToolOutput::ok(list.iter().map(|d| format!("- {} -> {}: {} {}", d.id, d.to_dot_name, d.status.as_str(), d.result.as_deref().map(|r| format!("| {}", r.chars().take(300).collect::<String>())).unwrap_or_default())).collect::<Vec<_>>().join("\n")))
    }

    pub async fn cancel_delegation(&self, ctx: &SharedContext, id: &str) -> DotsResult<ToolOutput> {
        let d = self.storage.call({ let i = id.to_string(); move |st| st.delegations().get(&i) }).await?;
        if d.parent_task_id != ctx.task_id && d.from_dot_id != ctx.dot_id {
            return Ok(ToolOutput::err("esta delegacao nao pertence a esta tarefa".into()));
        }
        if let Some(child) = &d.child_task_id {
            self.cancel_task(child).await?;
        }
        let i = id.to_string();
        self.storage.call(move |st| st.delegations().set_status(&i, DelegationStatus::Cancelled, None)).await?;
        self.events.emit("delegation.changed", json!({ "id": id, "status": "cancelled" }));
        Ok(ToolOutput::ok("delegacao cancelada".into()))
    }

    pub async fn list_team(&self, ctx: &SharedContext) -> DotsResult<ToolOutput> {
        let allowed = ctx.policy.can_delegate_to.clone();
        let me = ctx.dot_id.clone();
        let dots = self.storage.call(move |st| {
            let all = st.dots().list(false)?;
            let mut out = vec![];
            for d in all {
                if d.id == me || d.status != DotStatus::Active { continue; }
                if !(allowed.iter().any(|x| x == "*" || *x == d.id)) { continue; }
                let running = st.tasks().running_count_for_dot(&d.id)?;
                out.push((d, running));
            }
            Ok(out)
        }).await?;
        if dots.is_empty() {
            return Ok(ToolOutput::ok("nenhum DOT disponivel para delegacao".into()));
        }
        Ok(ToolOutput::ok(dots.iter().map(|(d, running)| format!("- {} (id {}): {} | assistente: {} | {}", d.name, d.id, if d.objective.is_empty() { d.description.clone() } else { d.objective.clone() }, d.primary_engine.map(|e| e.as_str()).unwrap_or("nenhum"), if *running > 0 { format!("{running} trabalho(s) em andamento") } else { "livre".into() })).collect::<Vec<_>>().join("\n")))
    }

    /// `dots_ask_user`: registra a pergunta; a resposta volta numa tarefa de continuacao.
    /// Em um pedido delegado, a pergunta vai para quem delegou (o gerente), que responde com
    /// `dots_answer_question`; o usuario so e envolvido se o gerente decidir perguntar a ele.
    pub async fn ask_user(&self, ctx: &SharedContext, question: &str, options: Vec<String>) -> DotsResult<ToolOutput> {
        let q = question.trim().to_string();
        if q.is_empty() {
            return Ok(ToolOutput::err("pergunta vazia".into()));
        }
        let (eid, tid, did, opts) = (ctx.execution_id.clone(), ctx.task_id.clone(), ctx.dot_id.clone(), options.clone());
        let qq = q.clone();
        let a = self.storage.call(move |st| st.approvals().create_question(&eid, &tid, &did, &qq, &opts, 24 * 3600)).await?;
        self.events.emit("approval.requested", json!(a));
        let tid2 = ctx.task_id.clone();
        if let Some(d) = self.storage.call(move |st| st.delegations().by_child(&tid2)).await? {
            let parent = self.storage.call({ let p = d.parent_task_id.clone(); move |st| st.tasks().get(&p) }).await?;
            let mut prompt = format!(
                "Pergunta do seu funcionario {} sobre o pedido que voce delegou (delegacao {}).\nPedido: {}\nPergunta: {}\n",
                d.to_dot_name, d.id, d.request.chars().take(300).collect::<String>(), q
            );
            if !options.is_empty() {
                prompt.push_str(&format!("Opcoes que ele sugeriu: {}\n", options.join(" | ")));
            }
            prompt.push_str(&format!(
                "\nResponda com dots_answer_question(question_id=\"{}\", answer=\"...\"). Se a resposta estiver nas suas instrucoes, nos dados ou nas suas memorias, responda direto. So pergunte ao usuario (dots_ask_user) se a decisao for dele; nesse caso, responda ao funcionario quando tiver a resposta.",
                a.id
            ));
            let input = TaskCreate {
                dot_id: d.from_dot_id.clone(),
                prompt,
                title: Some(format!("Pergunta de {}: {}", d.to_dot_name, q.chars().take(60).collect::<String>())),
                kind: Some(parent.kind),
                conversation_id: parent.conversation_id.clone(),
                origin: Some(format!("question:{}", a.id)),
                correlation_id: parent.correlation_id.clone().or(Some(parent.id.clone())),
                delegation_chain: parent.delegation_chain.clone(),
                parent_task_id: parent.parent_task_id.clone(),
                ..Default::default()
            };
            self.create_task(input).await?;
            return Ok(ToolOutput::ok(format!(
                "Pergunta encaminhada ao seu gerente {} ({}). Termine seu turno explicando que aguarda a resposta dele; voce recebera uma nova tarefa com ela.",
                d.from_dot_name, a.id
            )));
        }
        Ok(ToolOutput::ok(format!("Pergunta registrada ({}). Termine seu turno explicando que aguarda a resposta; voce recebera uma nova tarefa com ela.", a.id)))
    }

    /// `dots_answer_question`: o gerente responde a pergunta de um funcionario em pedido delegado por ele.
    pub async fn answer_question(&self, ctx: &SharedContext, question_id: &str, answer: &str) -> DotsResult<ToolOutput> {
        let answer = answer.trim().to_string();
        if answer.is_empty() {
            return Ok(ToolOutput::err("resposta vazia".into()));
        }
        let qid = question_id.trim().to_string();
        let Ok(a) = self.storage.call({ let q = qid.clone(); move |st| st.approvals().get(&q) }).await else {
            return Ok(ToolOutput::err(format!("pergunta '{qid}' nao encontrada")));
        };
        if a.kind != ApprovalKind::Question {
            return Ok(ToolOutput::err("isto nao e uma pergunta de funcionario".into()));
        }
        if a.status != ApprovalStatus::Pending {
            return Ok(ToolOutput::err("esta pergunta ja foi respondida".into()));
        }
        match self.storage.call({ let t = a.task_id.clone(); move |st| st.delegations().by_child(&t) }).await? {
            Some(d) if d.from_dot_id == ctx.dot_id => {}
            _ => return Ok(ToolOutput::err("esta pergunta nao e de um funcionario em pedido delegado por voce".into())),
        }
        let (q2, ans2) = (qid.clone(), answer.clone());
        let approval = self.storage.call(move |st| st.approvals().resolve(&q2, true, Some(&ans2))).await?;
        self.registry.resolve_approval(&qid, true);
        self.events.emit("approval.resolved", json!(approval));
        let _ = self.emit_event(et::APPROVAL_GRANTED, &format!("dot:{}", ctx.dot_id), json!({ "approval_id": qid, "dot_id": approval.dot_id, "dot_name": approval.dot_name, "task_id": approval.task_id, "tool": approval.tool, "kind": approval.kind.as_str(), "answered_by": ctx.dot_id }), None, Some(format!("approval:{qid}:granted")), 0).await;
        let (actor, q3, ans3) = (format!("dot:{}", ctx.dot_id), qid.clone(), answer.clone());
        let _ = self.storage.call(move |st| st.audit().record(&actor, "approval.approve", "approval", Some(&q3), json!({ "reason": ans3 })).map(|_| ())).await;
        self.continue_after_question(&approval, &format!("seu gerente {}", ctx.dot_name)).await;
        Ok(ToolOutput::ok(format!("Resposta enviada a {}. Ele continua o trabalho e o resultado chega a voce quando terminar.", approval.dot_name)))
    }

    /// Ha pergunta pendente desta tarefa (encaminhada ao gerente ou ao usuario)?
    async fn has_pending_question(&self, task_id: &str) -> bool {
        let tid = task_id.to_string();
        self.storage
            .call(move |st| Ok(st.approvals().list(true, 500)?.into_iter().any(|a| a.task_id == tid && a.kind == ApprovalKind::Question)))
            .await
            .unwrap_or(false)
    }

    /// Chamado quando uma tarefa termina: fecha a delegacao e, se todas as irmas terminaram, cria a continuacao do pai.
    pub async fn on_task_finished(&self, task: &Task, status: TaskStatus, result: &str, error: Option<&str>) {
        let child_id = task.id.clone();
        let Ok(Some(d)) = self.storage.call(move |st| st.delegations().by_child(&child_id)).await else { return };
        // Terminou perguntando: a delegacao segue aberta ate a resposta e a continuacao (que vira a nova filha).
        if status == TaskStatus::Completed && self.has_pending_question(&task.id).await {
            return;
        }
        let dstatus = match status {
            TaskStatus::Completed => DelegationStatus::Completed,
            TaskStatus::Cancelled => DelegationStatus::Cancelled,
            _ => DelegationStatus::Failed,
        };
        let summary = if status == TaskStatus::Completed { result.to_string() } else { format!("[{}] {}", status.as_str(), error.unwrap_or("")) };
        let (did, s) = (d.id.clone(), summary.clone());
        let _ = self.storage.call(move |st| st.delegations().set_status(&did, dstatus, Some(&s))).await;
        self.events.emit("delegation.changed", json!({ "id": d.id, "status": dstatus.as_str() }));
        let _ = self.emit_event(et::DELEGATION_COMPLETED, &format!("dot:{}", task.dot_id), json!({ "delegation_id": d.id, "parent_task_id": d.parent_task_id, "status": dstatus.as_str() }), Some(d.correlation_id.clone()), Some(format!("delegation-done:{}", d.id)), 0).await;

        // Continuacao do solicitante quando todas as delegacoes da tarefa pai terminaram.
        let parent_id = d.parent_task_id.clone();
        let (parent, siblings) = match self.storage.call(move |st| Ok((st.tasks().get(&parent_id)?, st.delegations().for_parent(&parent_id)?))).await {
            Ok(x) => x,
            Err(_) => return,
        };
        if parent.status == TaskStatus::Cancelled || siblings.iter().any(|s| !s.status.is_terminal()) {
            return;
        }
        let already = self.storage.call({ let pid = parent.id.clone(); move |st| Ok(st.tasks().list(&dots_storage::repos::TaskFilter { limit: 50, ..Default::default() })?.into_iter().any(|t| t.origin == format!("followup:{pid}"))) }).await.unwrap_or(false);
        if already {
            return;
        }
        let mut prompt = String::from("Continuacao: as delegacoes que voce pediu terminaram. Resultados:\n");
        for s in &siblings {
            prompt.push_str(&format!("\n## {} ({})\nPedido: {}\nResultado: {}\n", s.to_dot_name, s.status.as_str(), s.request, s.result.as_deref().unwrap_or("(sem resultado)")));
        }
        prompt.push_str("\nConsolide os resultados e responda ao usuario. Se algo ficou bloqueado, explique e, se necessario, use dots_ask_user.");
        let input = TaskCreate {
            dot_id: parent.dot_id.clone(),
            prompt,
            title: Some(format!("Continuacao: {}", parent.title)),
            kind: Some(parent.kind),
            conversation_id: parent.conversation_id.clone(),
            origin: Some(format!("followup:{}", parent.id)),
            correlation_id: parent.correlation_id.clone().or(Some(parent.id.clone())),
            delegation_chain: parent.delegation_chain.clone(),
            parent_task_id: parent.parent_task_id.clone(),
            ..Default::default()
        };
        match self.create_task(input).await {
            Ok(t) => tracing::info!(parent = %parent.id, followup = %t.id, "continuacao criada apos delegacoes"),
            Err(e) => tracing::warn!(parent = %parent.id, error = %e, "falha ao criar continuacao"),
        }
    }

    /// Resposta do usuario a uma pergunta: cria a continuacao com a resposta.
    pub async fn on_question_answered(&self, approval: &dots_core::Approval) {
        self.continue_after_question(approval, "o usuario").await
    }

    /// Pergunta expirada sem resposta: o DOT continua sabendo disso (e a delegacao, se houver, pode fechar).
    pub async fn on_question_expired(&self, approval_id: &str) {
        let id = approval_id.to_string();
        let Ok(a) = self.storage.call(move |st| st.approvals().get(&id)).await else { return };
        if a.kind == ApprovalKind::Question {
            self.continue_after_question(&a, "ninguem").await;
        }
    }

    /// Continuacao de quem perguntou, com a resposta. `who` = quem respondeu ("o usuario", "seu gerente X").
    /// Se a tarefa era um pedido delegado, a continuacao passa a ser a filha da delegacao.
    pub async fn continue_after_question(&self, approval: &dots_core::Approval, who: &str) {
        let tid = approval.task_id.clone();
        let Ok(task) = self.storage.call(move |st| st.tasks().get(&tid)).await else { return };
        let answer = approval.reason.clone().unwrap_or_default();
        let prompt = format!("Continuacao: {who} respondeu a sua pergunta.\nPergunta: {}\nResposta: {}\n\nProssiga com o trabalho.", approval.summary, if answer.is_empty() { "(sem resposta: rejeitada)".into() } else { answer });
        let input = TaskCreate {
            dot_id: task.dot_id.clone(),
            prompt,
            title: Some(format!("Continuacao: {}", task.title)),
            kind: Some(task.kind),
            conversation_id: task.conversation_id.clone(),
            origin: Some(format!("followup:{}", task.id)),
            correlation_id: task.correlation_id.clone().or(Some(task.id.clone())),
            delegation_chain: task.delegation_chain.clone(),
            parent_task_id: task.parent_task_id.clone(),
            ..Default::default()
        };
        if task.status == TaskStatus::WaitingUser {
            let t = task.id.clone();
            let _ = self.storage.call(move |st| st.tasks().set_status(&t, TaskStatus::Completed, None)).await;
            self.events.emit("task.changed", json!({ "id": task.id, "dot_id": task.dot_id, "status": "COMPLETED" }));
        }
        if let Ok(follow) = self.create_task(input).await {
            let (old, new) = (task.id.clone(), follow.id.clone());
            let _ = self.storage.call(move |st| {
                if let Some(d) = st.delegations().by_child(&old)? {
                    st.delegations().set_child(&d.id, &new)?;
                }
                Ok(())
            }).await;
        }
    }

    /// Cancela a tarefa e, em cascata, as filhas delegadas.
    pub async fn cancel_task(&self, id: &str) -> DotsResult<()> {
        let task = self.storage.call({ let id = id.to_string(); move |s| s.tasks().request_cancel(&id) }).await?;
        let execs = self.storage.call({ let id = id.to_string(); move |s| s.executions().list_for_task(&id) }).await?;
        let mut signalled = false;
        for e in execs.iter().filter(|e| e.status == dots_core::ExecutionStatus::Running) {
            signalled |= self.registry.cancel(&e.id);
        }
        if !signalled && matches!(task.status, TaskStatus::Queued | TaskStatus::WaitingUser) {
            let id2 = id.to_string();
            self.storage.call(move |s| s.tasks().set_status(&id2, TaskStatus::Cancelled, Some("cancelada pelo usuario"))).await?;
        }
        let children = self.storage.call({ let id = id.to_string(); move |s| s.tasks().children(&id) }).await?;
        for c in children.into_iter().filter(|c| !c.status.is_terminal()) {
            Box::pin(self.cancel_task(&c.id)).await?;
        }
        self.events.emit("task.changed", json!({ "id": id, "dot_id": task.dot_id, "status": "CANCELLING" }));
        self.scheduler_notify.notify_one();
        Ok(())
    }
}
