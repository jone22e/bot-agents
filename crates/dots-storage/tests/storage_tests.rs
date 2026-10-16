use dots_core::{DotCreate, DotStatus, DotUpdate, EngineKind};
use dots_storage::{Storage, StorageHandle, StorageOptions, latest_version};

fn temp_db() -> (tempfile::TempDir, StorageOptions) {
    let dir = tempfile::tempdir().unwrap();
    let opts = StorageOptions::new(dir.path().join("dots.db")).with_backup_dir(dir.path().join("backups"));
    (dir, opts)
}

#[test]
fn migrations_apply_once_and_are_idempotent() {
    let (_dir, opts) = temp_db();
    let s = Storage::open(opts.clone()).unwrap();
    assert_eq!(s.schema_version().unwrap(), latest_version());
    drop(s);
    // Reabrir nao reaplica nem falha.
    let s = Storage::open(opts).unwrap();
    assert_eq!(s.schema_version().unwrap(), latest_version());
    assert!(s.list_backups().unwrap().is_empty(), "sem migracao pendente nao ha backup");
}

#[test]
fn dot_crud_roundtrip() {
    let mut s = Storage::open_in_memory().unwrap();
    let created = s
        .dots()
        .create(
            "user",
            DotCreate {
                name: "  Revisor ".into(),
                description: "Revisa PRs".into(),
                objective: "Qualidade".into(),
                instructions: "Seja rigoroso".into(),
                primary_engine: Some(EngineKind::Claude),
                model: None,
                workspace_path: None,
                limits: None,
                role: None,
            },
            Some("/tmp/ws/x".into()),
        )
        .unwrap();
    assert_eq!(created.name, "Revisor");
    assert_eq!(created.primary_engine, Some(EngineKind::Claude));
    assert_eq!(created.workspace_path.as_deref(), Some("/tmp/ws/x"));
    assert_eq!(created.status, DotStatus::Active);
    assert_eq!(created.instructions, "Seja rigoroso");

    let updated = s
        .dots()
        .update(
            "user",
            &created.id,
            DotUpdate {
                name: Some("Revisor 2".into()),
                instructions: Some("Nova versao".into()),
                primary_engine: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(updated.name, "Revisor 2");
    assert_eq!(updated.instructions, "Nova versao");
    assert_eq!(updated.primary_engine, None);
    assert_eq!(s.dots().instructions_history(&created.id).unwrap().len(), 2);

    let paused = s.dots().set_status("user", &created.id, DotStatus::Paused).unwrap();
    assert_eq!(paused.status, DotStatus::Paused);

    let copy = s.dots().duplicate("user", &created.id, None, Some("/tmp/ws/y".into())).unwrap();
    assert_eq!(copy.name, "Revisor 2 (copia)");
    assert_eq!(copy.instructions, "Nova versao");
    assert_eq!(s.dots().list(false).unwrap().len(), 2);

    s.dots().set_status("user", &copy.id, DotStatus::Archived).unwrap();
    assert_eq!(s.dots().list(false).unwrap().len(), 1);
    assert_eq!(s.dots().list(true).unwrap().len(), 2);

    s.dots().delete("user", &created.id).unwrap();
    assert!(s.dots().get(&created.id).is_err());

    let audit = s.audit().list_recent(50).unwrap();
    let actions: Vec<&str> = audit.iter().map(|a| a.action.as_str()).collect();
    assert!(actions.contains(&"dot.create"));
    assert!(actions.contains(&"dot.update"));
    assert!(actions.contains(&"dot.pause"));
    assert!(actions.contains(&"dot.duplicate"));
    assert!(actions.contains(&"dot.archive"));
    assert!(actions.contains(&"dot.delete"));
}

#[test]
fn validation_rejects_empty_name() {
    let mut s = Storage::open_in_memory().unwrap();
    let err = s.dots().create("user", DotCreate { name: "   ".into(), ..Default::default() }, None).unwrap_err();
    assert_eq!(err.code(), "VALIDATION");
}

#[test]
fn settings_and_installation_persist_across_reopen() {
    let (_dir, opts) = temp_db();
    let inst_id;
    {
        let mut s = Storage::open(opts.clone()).unwrap();
        let inst = s.installation().ensure("0.1.0", "/data", "macos-arm64").unwrap();
        inst_id = inst.id.clone();
        s.settings().set("onboarding.completed", &serde_json::json!(true)).unwrap();
    }
    let mut s = Storage::open(opts).unwrap();
    let inst = s.installation().ensure("0.2.0", "/data", "macos-arm64").unwrap();
    assert_eq!(inst.id, inst_id, "id da instalacao nunca muda");
    assert_eq!(inst.app_version, "0.2.0");
    let v = s.settings().get("onboarding.completed").unwrap().unwrap();
    assert_eq!(v.value, serde_json::json!(true));
}

#[test]
fn engine_configuration_upsert() {
    let mut s = Storage::open_in_memory().unwrap();
    let list = s.engines().list().unwrap();
    assert_eq!(list.len(), 3);
    assert!(list.iter().all(|e| e.last_detection.is_none()));
    let cfg = s.engines().set_executable_override(EngineKind::Codex, Some("/x/codex")).unwrap();
    assert_eq!(cfg.executable_override.as_deref(), Some("/x/codex"));
    let cfg = s.engines().set_enabled(EngineKind::Codex, false).unwrap();
    assert!(!cfg.enabled);
    assert_eq!(cfg.executable_override.as_deref(), Some("/x/codex"));
    assert_eq!(cfg.model, None);
    let cfg = s.engines().set_model(EngineKind::Codex, Some("  gpt-5-codex ")).unwrap();
    assert_eq!(cfg.model.as_deref(), Some("gpt-5-codex"));
    assert!(!cfg.enabled, "set_model nao religa o motor");
    assert!(s.engines().set_model(EngineKind::Codex, Some("a b")).is_err(), "espaco no modelo e rejeitado");
    let cfg = s.engines().set_model(EngineKind::Codex, Some("")).unwrap();
    assert_eq!(cfg.model, None, "vazio volta ao padrao do CLI");
}

#[test]
fn dot_model_follows_primary_engine() {
    let mut s = Storage::open_in_memory().unwrap();
    let dot = s
        .dots()
        .create("user", DotCreate { name: "M".into(), primary_engine: Some(EngineKind::Claude), model: Some("claude-opus-5-5".into()), ..Default::default() }, None)
        .unwrap();
    assert_eq!(dot.model.as_deref(), Some("claude-opus-5-5"));
    // Atualizacao sem mencionar o modelo mantem o valor.
    let dot = s.dots().update("user", &dot.id, DotUpdate { name: Some("M2".into()), ..Default::default() }).unwrap();
    assert_eq!(dot.model.as_deref(), Some("claude-opus-5-5"));
    // Trocar o motor sem informar modelo descarta o modelo antigo.
    let dot = s.dots().update("user", &dot.id, DotUpdate { primary_engine: Some(Some(EngineKind::Codex)), ..Default::default() }).unwrap();
    assert_eq!(dot.primary_engine, Some(EngineKind::Codex));
    assert_eq!(dot.model, None);
    // Definir e limpar explicitamente.
    let dot = s.dots().update("user", &dot.id, DotUpdate { model: Some(Some("gpt-5-codex".into())), ..Default::default() }).unwrap();
    assert_eq!(dot.model.as_deref(), Some("gpt-5-codex"));
    let copy = s.dots().duplicate("user", &dot.id, None, None).unwrap();
    assert_eq!(copy.model.as_deref(), Some("gpt-5-codex"));
    let dot = s.dots().update("user", &dot.id, DotUpdate { model: Some(None), ..Default::default() }).unwrap();
    assert_eq!(dot.model, None);
    assert!(s.dots().update("user", &dot.id, DotUpdate { model: Some(Some("x y".into())), ..Default::default() }).is_err());
}

#[test]
fn backup_now_creates_file_and_prunes() {
    let (dir, mut opts) = temp_db();
    opts.backup_keep = 2;
    let s = Storage::open(opts).unwrap();
    for i in 0..3 {
        s.backup_now(&format!("t{i}")).unwrap();
    }
    let files = s.list_backups().unwrap();
    assert_eq!(files.len(), 2);
    assert!(dir.path().join("backups").exists());
}

#[tokio::test]
async fn actor_handles_concurrent_calls() {
    let (_dir, opts) = temp_db();
    let actor = StorageHandle::spawn(opts).unwrap();
    let h = actor.handle.clone();
    let mut joins = vec![];
    for i in 0..10 {
        let h = h.clone();
        joins.push(tokio::spawn(async move {
            h.call(move |s| {
                s.dots().create("user", DotCreate { name: format!("dot-{i}"), ..Default::default() }, None)
            })
            .await
        }));
    }
    for j in joins {
        j.await.unwrap().unwrap();
    }
    let n = h.call(|s| Ok(s.dots().list(false)?.len())).await.unwrap();
    assert_eq!(n, 10);
    actor.shutdown();
    assert!(h.call(|_| Ok(())).await.is_err(), "apos shutdown o handle falha");
}

#[test]
fn upgrade_with_new_migration_keeps_data_and_creates_backup() {
    use dots_storage::{MIGRATIONS, Migration};
    let (dir, opts) = temp_db();
    let dot_id;
    {
        let mut s = Storage::open(opts.clone()).unwrap();
        dot_id = s.dots().create("user", DotCreate { name: "Persistente".into(), ..Default::default() }, None).unwrap().id;
    }
    let mut future: Vec<Migration> = MIGRATIONS.to_vec();
    future.push(Migration {
        version: latest_version() + 1,
        name: "test_add_column",
        sql: "ALTER TABLE dots ADD COLUMN test_col TEXT;",
    });
    let mut s = Storage::open_with(opts.clone(), &future).unwrap();
    assert_eq!(s.schema_version().unwrap(), latest_version() + 1);
    assert_eq!(s.dots().get(&dot_id).unwrap().name, "Persistente");
    let backups = s.list_backups().unwrap();
    assert_eq!(backups.len(), 1, "backup pre-migracao deve existir");
    assert!(backups[0].file_name().unwrap().to_string_lossy().contains(&format!("pre-migrate-v{}", latest_version())));
    drop(s);

    // Um binario antigo (sem a migracao nova) deve recusar o banco mais novo.
    let err = Storage::open(opts).err().expect("deveria falhar");
    assert_eq!(err.code(), "STORAGE");
    assert!(dir.path().join("backups").exists());
}

mod phase2 {
    use dots_core::*;
    use dots_storage::Storage;
    use dots_storage::repos::TaskFilter;

    fn with_dot() -> (Storage, String) {
        let mut s = Storage::open_in_memory().unwrap();
        let dot = s.dots().create("user", DotCreate { name: "Exec".into(), primary_engine: Some(EngineKind::Claude), ..Default::default() }, Some("/tmp/ws".into())).unwrap();
        (s, dot.id)
    }

    #[test]
    fn task_and_execution_lifecycle() {
        let (mut s, dot_id) = with_dot();
        let t = s.tasks().create(TaskCreate { dot_id: dot_id.clone(), prompt: "Fazer X".into(), ..Default::default() }, Some(EngineKind::Claude)).unwrap();
        assert_eq!(t.status, TaskStatus::Queued);
        assert_eq!(t.title, "Fazer X");
        assert_eq!(s.tasks().queued(10).unwrap().len(), 1);
        let t = s.tasks().set_status(&t.id, TaskStatus::Running, None).unwrap();
        assert_eq!(t.attempts, 1);
        assert!(t.started_at.is_some());
        let e = s.executions().create(&t.id, &dot_id, EngineKind::Claude, 1, Some("/tmp/ws")).unwrap();
        let r1 = s.executions().add_event(&e.id, &ExecutionEvent::Started { engine_session_id: Some("sess".into()) }).unwrap();
        let r2 = s.executions().add_event(&e.id, &ExecutionEvent::Text { text: "oi".into() }).unwrap();
        assert_eq!((r1.seq, r2.seq), (1, 2));
        assert_eq!(s.executions().get(&e.id).unwrap().events_count, 2);
        let evs = s.executions().events(&e.id, 1, 100).unwrap();
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0].event, ExecutionEvent::Text { .. }));
        let e = s.executions().finish(&e.id, ExecutionStatus::Completed, Some(0), None, Some(0.01), &serde_json::json!({"input_tokens": 3})).unwrap();
        assert_eq!(e.status, ExecutionStatus::Completed);
        s.tasks().set_result(&t.id, "feito").unwrap();
        let t = s.tasks().set_status(&t.id, TaskStatus::Completed, None).unwrap();
        assert_eq!(t.result_text.as_deref(), Some("feito"));
        assert!(t.finished_at.is_some());
        assert_eq!(s.tasks().list(&TaskFilter { dot_id: Some(dot_id), ..Default::default() }).unwrap().len(), 1);
    }

    #[test]
    fn recovery_marks_interrupted() {
        let (mut s, dot_id) = with_dot();
        let a = s.tasks().create(TaskCreate { dot_id: dot_id.clone(), prompt: "A".into(), retry_safe: Some(true), ..Default::default() }, None).unwrap();
        let b = s.tasks().create(TaskCreate { dot_id: dot_id.clone(), prompt: "B".into(), ..Default::default() }, None).unwrap();
        // pedido delegado (nao retry_safe) volta a fila: o gerente espera por ele
        let c = s.tasks().create(TaskCreate { dot_id: dot_id.clone(), prompt: "C".into(), origin: Some("delegation:d1".into()), ..Default::default() }, None).unwrap();
        s.tasks().set_status(&a.id, TaskStatus::Running, None).unwrap();
        s.tasks().set_status(&b.id, TaskStatus::Running, None).unwrap();
        s.tasks().set_status(&c.id, TaskStatus::Running, None).unwrap();
        s.executions().create(&a.id, &dot_id, EngineKind::Codex, 1, None).unwrap();
        assert_eq!(s.executions().mark_interrupted().unwrap(), 1);
        let recovered = s.tasks().recover_interrupted().unwrap();
        assert_eq!(recovered.len(), 3);
        assert_eq!(s.tasks().get(&a.id).unwrap().status, TaskStatus::Queued);
        assert_eq!(s.tasks().get(&b.id).unwrap().status, TaskStatus::Queued, "um reinicio nao mata o trabalho");
        assert_eq!(s.tasks().get(&c.id).unwrap().status, TaskStatus::Queued);
        // segundo reinicio da tarefa comum (max_attempts 1) ja aguarda o usuario
        s.tasks().set_status(&b.id, TaskStatus::Running, None).unwrap();
        s.tasks().recover_interrupted().unwrap();
        assert_eq!(s.tasks().get(&b.id).unwrap().status, TaskStatus::WaitingUser);
    }

    #[test]
    fn conversations_messages_and_sessions() {
        let (mut s, dot_id) = with_dot();
        let c = s.conversations().create(&dot_id, "Geral").unwrap();
        s.conversations().add_message(&c.id, MessageRole::User, "oi", None, None, serde_json::json!({})).unwrap();
        let m = s.conversations().add_message(&c.id, MessageRole::Dot, "", None, None, serde_json::json!({"streaming": true})).unwrap();
        s.conversations().update_message_content(&m.id, "ola!", Some(serde_json::json!({"streaming": false}))).unwrap();
        let msgs = s.conversations().messages(&c.id, 10).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].content, "ola!");
        s.conversations().set_engine_session(&c.id, EngineKind::Claude, Some("uuid-1")).unwrap();
        assert_eq!(s.conversations().engine_session(&c.id, EngineKind::Claude).unwrap().as_deref(), Some("uuid-1"));
        assert_eq!(s.conversations().engine_session(&c.id, EngineKind::Codex).unwrap(), None);
        assert_eq!(s.conversations().get(&c.id).unwrap().message_count, 2);
        assert_eq!(s.conversations().list(Some(&dot_id)).unwrap().len(), 1);
    }

    #[test]
    fn memories_fts_search_and_prune() {
        let (mut s, dot_id) = with_dot();
        s.memories().create(MemoryCreate { dot_id: dot_id.clone(), kind: MemoryKind::LongTerm, content: "O deploy do Flexi usa SSM na AWS".into(), importance: Some(5), source: None }).unwrap();
        s.memories().create(MemoryCreate { dot_id: dot_id.clone(), kind: MemoryKind::LongTerm, content: "Testes rodam com cargo test".into(), importance: Some(1), source: None }).unwrap();
        s.memories().create(MemoryCreate { dot_id: dot_id.clone(), kind: MemoryKind::History, content: "tarefa antiga sobre deploy".into(), importance: Some(0), source: None }).unwrap();
        let hits = s.memories().search(&dot_id, "como fazer deploy?", &[], 10).unwrap();
        assert!(hits.len() >= 2);
        assert!(hits[0].content.contains("deploy"));
        let only_lt = s.memories().search(&dot_id, "deploy", &[MemoryKind::LongTerm], 10).unwrap();
        assert_eq!(only_lt.len(), 1);
        // acentos ignorados (remove_diacritics)
        let acc = s.memories().search(&dot_id, "DEPLÓY", &[], 10).unwrap();
        assert!(!acc.is_empty());
        // sem termos validos -> fallback por importancia
        let fb = s.memories().search(&dot_id, "a b", &[], 10).unwrap();
        assert_eq!(fb[0].importance, 5);
        let w = s.memories().upsert_working(&dot_id, "task-1", "etapa 1".into()).unwrap();
        let w2 = s.memories().upsert_working(&dot_id, "task-1", "etapa 2".into()).unwrap();
        assert_eq!(w.id, w2.id);
        assert_eq!(w2.content, "etapa 2");
        s.memories().update(&w2.id, MemoryUpdate { importance: Some(9), ..Default::default() }).unwrap();
        s.memories().delete(&w2.id).unwrap();
        assert!(s.memories().get(&w2.id).is_err());
        assert_eq!(s.memories().prune(&dot_id, MemoryKind::LongTerm, 1).unwrap(), 1);
    }

    #[test]
    fn approvals_and_policies() {
        let (mut s, dot_id) = with_dot();
        let t = s.tasks().create(TaskCreate { dot_id: dot_id.clone(), prompt: "x".into(), ..Default::default() }, None).unwrap();
        let e = s.executions().create(&t.id, &dot_id, EngineKind::Claude, 1, None).unwrap();
        let a = s.approvals().create(&e.id, &t.id, &dot_id, "dots_write_file", &serde_json::json!({"path": "/tmp/ws/a.txt"}), RiskLevel::Write, &["/tmp/ws/a.txt".into()], "escrever a.txt", 600).unwrap();
        assert_eq!(a.status, ApprovalStatus::Pending);
        assert_eq!(a.dot_name, "Exec");
        assert_eq!(s.approvals().pending_count().unwrap(), 1);
        let a = s.approvals().resolve(&a.id, true, None).unwrap();
        assert_eq!(a.status, ApprovalStatus::Approved);
        assert_eq!(s.approvals().resolve(&a.id, false, None).unwrap_err().code(), "CONFLICT");
        let expired = s.approvals().create(&e.id, &t.id, &dot_id, "dots_run_command", &serde_json::json!({}), RiskLevel::Exec, &[], "cmd", -1).unwrap();
        assert_eq!(s.approvals().expire_due().unwrap(), vec![expired.id.clone()]);
        assert_eq!(s.approvals().get(&expired.id).unwrap().status, ApprovalStatus::Expired);

        let p = s.policies().get(&dot_id).unwrap();
        assert_eq!(p.auto_approve_level, AutoApproveLevel::Read);
        let p = s.policies().update(&dot_id, ToolPolicyUpdate { allowed_commands: Some(vec![" npm test ".into(), "".into()]), auto_approve_level: Some(AutoApproveLevel::Write), ..Default::default() }).unwrap();
        assert_eq!(p.allowed_commands, vec!["npm test".to_string()]);
        assert!(p.auto_approve_level.covers(RiskLevel::Write));
        assert!(!p.auto_approve_level.covers(RiskLevel::Exec));
        assert!(!AutoApproveLevel::Exec.covers(RiskLevel::Critical));

        let ws = s.dots().add_workspace("user", &dot_id, "/tmp/outro", true, false).unwrap();
        assert!(ws.is_external && !ws.can_write);
        assert_eq!(s.dots().workspaces(&dot_id).unwrap().len(), 2);
        s.dots().remove_workspace("user", &dot_id, "/tmp/outro").unwrap();
        assert!(s.dots().remove_workspace("user", &dot_id, "/tmp/ws").is_err(), "primario nao pode ser removido");
        s.changes().record(&e.id, &dot_id, "/tmp/ws/a.txt", "create", "criado").unwrap();
        assert_eq!(s.changes().list(Some(&e.id), None, 10).unwrap().len(), 1);
    }
}

mod phase3 {
    use dots_core::*;
    use dots_storage::Storage;

    fn with_dot() -> (Storage, String) {
        let mut s = Storage::open_in_memory().unwrap();
        let dot = s.dots().create("user", DotCreate { name: "Sup".into(), role: Some(DotRole::Supervisor), ..Default::default() }, Some("/tmp/ws".into())).unwrap();
        assert_eq!(dot.role, DotRole::Supervisor);
        (s, dot.id)
    }

    #[test]
    fn events_dedupe_and_unique_delivery() {
        let (mut s, dot_id) = with_dot();
        let a = s.events().insert("file.changed", "watcher", &serde_json::json!({"path": "/x/a.md"}), None, Some("k1"), 0).unwrap();
        assert!(a.is_some());
        let dup = s.events().insert("file.changed", "watcher", &serde_json::json!({"path": "/x/a.md"}), None, Some("k1"), 0).unwrap();
        assert!(dup.is_none(), "chave repetida e ignorada");
        let sub = s.subscriptions().create(SubscriptionCreate { dot_id: dot_id.clone(), event_type: "file.changed".into(), filter: SubscriptionFilter { path_glob: Some("*.md".into()), source_dot_id: None }, prompt_template: "Revise {{path}}".into(), retry_safe: true, cooldown_seconds: Some(0) }).unwrap();
        assert!(s.events().record_delivery(&a.as_ref().unwrap().id, &sub.id, None, "created").unwrap());
        assert!(!s.events().record_delivery(&a.as_ref().unwrap().id, &sub.id, None, "created").unwrap(), "segunda entrega e recusada");
        assert!(s.subscriptions().create(SubscriptionCreate { dot_id: dot_id.clone(), event_type: "schedule.due".into(), prompt_template: "x".into(), ..Default::default() }).is_err(), "tipo nao assinavel");
        assert_eq!(s.subscriptions().enabled_for_type("file.changed").unwrap().len(), 1);
        s.subscriptions().update(&sub.id, SubscriptionUpdate { enabled: Some(false), ..Default::default() }).unwrap();
        assert_eq!(s.subscriptions().enabled_for_type("file.changed").unwrap().len(), 0);
        assert_eq!(s.events().list(Some("file.changed"), 10).unwrap().len(), 1);
    }

    #[test]
    fn schedules_due_and_next() {
        let (mut s, dot_id) = with_dot();
        let past = dots_core::now() - chrono::Duration::minutes(1);
        let future = dots_core::now() + chrono::Duration::hours(1);
        let a = s.schedules().create(ScheduleCreate { dot_id: dot_id.clone(), kind: ScheduleKind::Every, spec: "30".into(), prompt: "Verifique".into() }, Some(&past)).unwrap();
        let _b = s.schedules().create(ScheduleCreate { dot_id: dot_id.clone(), kind: ScheduleKind::Daily, spec: "09:00".into(), prompt: "Bom dia".into() }, Some(&future)).unwrap();
        let due = s.schedules().due(&dots_core::now()).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, a.id);
        s.schedules().mark_run(&a.id, Some(&future)).unwrap();
        assert!(s.schedules().due(&dots_core::now()).unwrap().is_empty());
        assert_eq!(s.schedules().next_due().unwrap().unwrap().timestamp(), future.timestamp());
    }

    #[test]
    fn checkpoints_and_invocation_guard() {
        let (mut s, dot_id) = with_dot();
        let t = s.tasks().create(TaskCreate { dot_id: dot_id.clone(), prompt: "x".into(), retry_safe: Some(true), ..Default::default() }, None).unwrap();
        assert_eq!(t.max_attempts, 2);
        assert_eq!(t.origin, "user");
        let cp = s.checkpoints().create(&t.id, "e1", "etapa 1", &serde_json::json!({"files": 1}), "ctx", "res", &[], "proxima", &["/x/a.txt".into()], "auto").unwrap();
        s.checkpoints().create(&t.id, "e1", "etapa 2", &serde_json::json!({}), "", "", &[], "", &[], "dot").unwrap();
        assert_eq!(s.checkpoints().latest(&t.id).unwrap().unwrap().step, "etapa 2");
        assert_eq!(s.checkpoints().list(&t.id).unwrap()[0].id, cp.id);
        assert!(s.checkpoints().find_invocation(&t.id, "dots_run_command", "h1").unwrap().is_none());
        s.checkpoints().record_invocation(&t.id, "e1", "dots_run_command", "h1", "ok", "saida").unwrap();
        assert_eq!(s.checkpoints().find_invocation(&t.id, "dots_run_command", "h1").unwrap().as_deref(), Some("saida"));
        // recuperacao: volta a fila enquanto as tentativas nao passam de max_attempts (retry_safe: 2); depois aguarda usuario
        s.tasks().set_status(&t.id, TaskStatus::Running, None).unwrap();
        let rec = s.tasks().recover_interrupted().unwrap();
        assert_eq!(rec[0].1, TaskStatus::Queued);
        s.tasks().set_status(&t.id, TaskStatus::Running, None).unwrap();
        let rec = s.tasks().recover_interrupted().unwrap();
        assert_eq!(rec[0].1, TaskStatus::Queued, "2a tentativa ainda dentro de max_attempts");
        s.tasks().set_status(&t.id, TaskStatus::Running, None).unwrap();
        let rec = s.tasks().recover_interrupted().unwrap();
        assert_eq!(rec[0].1, TaskStatus::WaitingUser, "3a tentativa esgotou max_attempts");
    }

    #[test]
    fn delegations_and_questions() {
        let (mut s, sup) = with_dot();
        let worker = s.dots().create("user", DotCreate { name: "Worker".into(), ..Default::default() }, None).unwrap();
        let parent = s.tasks().create(TaskCreate { dot_id: sup.clone(), prompt: "coordene".into(), ..Default::default() }, None).unwrap();
        let child = s.tasks().create(TaskCreate { dot_id: worker.id.clone(), prompt: "faca".into(), origin: Some(format!("delegation:x")), parent_task_id: Some(parent.id.clone()), delegation_chain: vec![sup.clone()], correlation_id: Some("corr".into()), ..Default::default() }, None).unwrap();
        assert_eq!(child.parent_task_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(child.delegation_chain, vec![sup.clone()]);
        assert_eq!(child.source, "delegation");
        let d = s.delegations().create(&sup, &worker.id, &parent.id, &child.id, "corr", "faca").unwrap();
        assert_eq!(d.to_dot_name, "Worker");
        assert_eq!(d.status, DelegationStatus::Pending);
        assert_eq!(s.delegations().by_child(&child.id).unwrap().unwrap().id, d.id);
        let d = s.delegations().set_status(&d.id, DelegationStatus::Completed, Some("feito")).unwrap();
        assert!(d.resolved_at.is_some() && d.result.as_deref() == Some("feito"));
        assert_eq!(s.tasks().children(&parent.id).unwrap().len(), 1);
        let p = s.policies().update(&sup, ToolPolicyUpdate { can_delegate_to: Some(vec!["*".into()]), max_delegation_depth: Some(9), ..Default::default() }).unwrap();
        assert_eq!(p.max_delegation_depth, 5, "limitado a 5");
        assert_eq!(p.can_delegate_to, vec!["*".to_string()]);
        let e = s.executions().create(&parent.id, &sup, EngineKind::Claude, 1, None).unwrap();
        let q = s.approvals().create_question(&e.id, &parent.id, &sup, "Qual banco?", &["Postgres".into(), "SQLite".into()], 600).unwrap();
        assert_eq!(q.kind, ApprovalKind::Question);
        assert_eq!(q.options.len(), 2);
        assert_eq!(s.approvals().pending_questions_count().unwrap(), 1);
        assert_eq!(s.approvals().pending_count().unwrap(), 0, "perguntas nao contam como permissoes");
        let q = s.approvals().resolve(&q.id, true, Some("SQLite")).unwrap();
        assert_eq!(q.reason.as_deref(), Some("SQLite"));
    }
}

mod flexi_phase {
    use dots_core::*;
    use dots_storage::Storage;

    #[test]
    fn app_connections_and_dot_integrations() {
        let mut s = Storage::open_in_memory().unwrap();
        let dot = s.dots().create("user", DotCreate { name: "ML".into(), ..Default::default() }, None).unwrap();
        let c = s.apps().create("flexi", "Flexi producao", "https://flexi.c2s-business.com/", Some("brasiltec"), "x@y.com", ConnectionStatus::NeedsLogin, true).unwrap();
        assert_eq!(c.base_url, "https://flexi.c2s-business.com");
        assert_eq!(c.status, ConnectionStatus::NeedsLogin);
        s.apps().set_session(&c.id, Some("Fulano"), Some(7), Some("Empresa"), Some("c2s_brasiltec")).unwrap();
        let c = s.apps().get(&c.id).unwrap();
        assert_eq!(c.status, ConnectionStatus::Connected);
        assert_eq!(c.user_name.as_deref(), Some("Fulano"));
        assert!(c.last_login_at.is_some());
        s.apps().set_status(&c.id, ConnectionStatus::Error, Some("rede")).unwrap();
        assert_eq!(s.apps().get(&c.id).unwrap().status_detail.as_deref(), Some("rede"));
        assert!(s.apps().integration(&dot.id, "flexi").unwrap().is_none());
        let i = s.apps().set_integration(&dot.id, "flexi", Some(&c.id), Some("ml_creator"), Some("mercadolivre")).unwrap().unwrap();
        assert_eq!(i.role_template.as_deref(), Some("ml_creator"));
        assert_eq!(s.apps().members("flexi").unwrap(), vec![(dot.id.clone(), "mercadolivre".to_string(), "worker".to_string())]);
        let again = s.apps().set_integration(&dot.id, "flexi", Some(&c.id), None, None).unwrap().unwrap();
        assert_eq!((again.role_template.as_deref(), again.work_group.as_deref()), (Some("ml_creator"), Some("mercadolivre")), "trocar so a conexao preserva papel e area");
        assert_eq!(i.connection_id, c.id);
        assert_eq!(s.apps().dots_using(&c.id).unwrap(), vec![dot.id.clone()]);
        assert!(s.apps().set_integration(&dot.id, "flexi", Some("nao-existe"), None, None).is_err());
        assert!(s.apps().set_integration(&dot.id, "flexi", None, None, None).unwrap().is_none());
        s.apps().delete(&c.id).unwrap();
        assert_eq!(s.apps().list(Some("flexi")).unwrap().len(), 0);
    }
}

#[test]
fn dismissal_is_recorded_and_memory_is_wiped() {
    use dots_core::{MemoryCreate, MemoryKind, MessageRole, TeamCreate};
    use dots_storage::repos::DismissalInput;
    let mut s = Storage::open_in_memory().unwrap();
    let mk = |s: &mut Storage, name: &str| s.dots().create("user", DotCreate { name: name.into(), description: "".into(), objective: "".into(), instructions: "".into(), primary_engine: None, model: None, workspace_path: None, limits: None, role: None }, Some(format!("/tmp/ws/{name}"))).unwrap();
    let boss = mk(&mut s, "Milei");
    let emp = mk(&mut s, "Lula");
    let team = s.teams().create(TeamCreate { name: "Mercado Livre".into(), description: "".into(), manager_dot_id: Some(boss.id.clone()), member_ids: vec![emp.id.clone()] }).unwrap();
    s.memories().create(MemoryCreate { dot_id: emp.id.clone(), kind: MemoryKind::LongTerm, content: "preco do kit".into(), importance: Some(3), source: None }).unwrap();
    s.memories().create(MemoryCreate { dot_id: emp.id.clone(), kind: MemoryKind::History, content: "publicou".into(), importance: None, source: None }).unwrap();
    s.memories().create(MemoryCreate { dot_id: boss.id.clone(), kind: MemoryKind::LongTerm, content: "do chefe".into(), importance: None, source: None }).unwrap();
    let conv = s.conversations().main(&emp.id).unwrap();
    s.conversations().add_message(&conv.id, MessageRole::User, "oi", None, None, serde_json::json!({})).unwrap();

    let wiped = s.memories().delete_all_for_dot(&emp.id).unwrap();
    assert_eq!(wiped, 2);
    assert_eq!(s.conversations().delete_for_dot(&emp.id).unwrap(), 1);
    assert!(s.memories().list(&emp.id, None, 0).unwrap().is_empty());
    assert_eq!(s.memories().list(&boss.id, None, 0).unwrap().len(), 1, "a memoria do gerente fica");
    assert!(s.memories().search(&emp.id, "kit", &[], 10).unwrap().is_empty(), "o indice FTS acompanha");
    let fresh = s.conversations().main(&emp.id).unwrap();
    assert_ne!(fresh.id, conv.id);
    assert!(s.conversations().messages(&fresh.id, 10).unwrap().is_empty());

    let d = s.dismissals().create(DismissalInput { team_id: Some(&team.id), team_name: &team.name, manager_dot_id: &boss.id, manager_name: "Milei", dot_id: &emp.id, dot_name: "Lula", reason: "publicou sem revisar".into(), lesson: "", memories_wiped: wiped }).unwrap();
    assert_eq!(d.memories_wiped, 2);
    assert_eq!(s.dismissals().for_team(&team.id, 0).unwrap().len(), 1);
    assert_eq!(s.dismissals().for_dot(&emp.id, 0).unwrap()[0].reason, "publicou sem revisar");
    assert_eq!(s.dismissals().by_manager(&boss.id, 0).unwrap().len(), 1);
    assert!(s.dismissals().create(DismissalInput { team_id: None, team_name: "", manager_dot_id: &boss.id, manager_name: "", dot_id: &emp.id, dot_name: "", reason: "  ", lesson: "", memories_wiped: 0 }).is_err(), "motivo obrigatorio");
    // O DOT continua na equipe e o registro sobrevive a exclusao do DOT.
    assert_eq!(s.teams().get(&team.id).unwrap().member_ids, vec![emp.id.clone()]);
    s.dots().delete("user", &emp.id).unwrap();
    let kept = s.dismissals().recent(10).unwrap();
    assert_eq!(kept.len(), 1);
    assert!(kept[0].dot_id.is_none() && kept[0].dot_name == "Lula");
}
