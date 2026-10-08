//! Composed goals behavior through the actual daemon service.

use super::*;
mod idle;
mod resume_recovery;

#[test]
fn persistent_goal_starts_real_provider_work_pauses_durably_and_resumes_same_run() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = support::stalled_then(
            0x61,
            "goal-writer",
            vec![support::text_response(b"Resumed goal response.")],
        );
        let reviewer = scripted(0x62, "goal-reviewer", Vec::new());
        let fixer = scripted(0x63, "goal-fixer", Vec::new());
        let workspace = WorkspaceId::new([0x64; 16]).expect("workspace");
        let run = RunId::new([0x65; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let objective = WorkbenchInputText::new("Answer the exact current request.".to_owned())
            .expect("objective");
        let brief = command(
            workspace,
            8,
            3,
            WorkbenchIntent::SetBrief {
                field: WorkbenchBriefField::Objective,
                text: objective.clone(),
            },
        );
        assert!(matches!(
            service.workbench_command(actor(), &brief).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let settings = WorkbenchExecutionSettings::new(
            run,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
        );
        let definition = WorkbenchGoalDefinition::new(
            objective,
            vec![WorkbenchGoalCriterionDefinition::new(
                WorkbenchGoalCriterionKind::RunnerAcceptance,
                WorkbenchInputText::new("Strict runner acceptance".to_owned()).expect("criterion"),
                true,
            )],
        )
        .expect("goal definition");
        let start = command(workspace, 9, 4, WorkbenchIntent::StartGoal { definition, settings });
        let started = service.workbench_command(actor(), &start).await;
        assert!(
            matches!(started, AppResponsePayload::WorkbenchReceipt(_)),
            "{started:?}; run present={}",
            service.inner.records.read().expect("records").contains_key(&run)
        );
        for _ in 0..200 {
            if writer.requests.lock().expect("requests").len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(writer.requests.lock().expect("requests").len(), 1);
        let active = match service.workbench_goal(actor(), query(workspace)) {
            AppResponsePayload::WorkbenchGoal(goal) => goal,
            response => panic!("expected active goal, got {response:?}"),
        };
        assert_eq!(active.state(), WorkbenchGoalState::Active);
        assert_eq!(active.usage().requests(), 1);
        // Simulate human think time while the stalled provider's active-time clock advances.
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        let pause = command(
            workspace,
            10,
            active.aggregate_revision() - 1, // An accounting update must not block a user's pause.
            WorkbenchIntent::PauseGoal { goal: active.goal(), mode: WorkbenchGoalPauseMode::Now },
        );
        assert!(matches!(
            service.workbench_command(actor(), &pause).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let stopped = wait_for_terminal(&service, run).await;
        assert_eq!(stopped.phase(), ProductRunPhase::Cancelled);
        assert!(stopped.status().starts_with("Paused"));
        let paused = match service.workbench_goal(actor(), query(workspace)) {
            AppResponsePayload::WorkbenchGoal(goal) => goal,
            response => panic!("expected paused goal, got {response:?}"),
        };
        assert_eq!(paused.state(), WorkbenchGoalState::Paused);
        assert_eq!((paused.attempt(), paused.usage().requests()), (1, 1));
        assert_eq!(paused.usage().roles()[0].completed_requests(), 1);
        assert_eq!(paused.usage().total_tokens(), None);

        let resume = command(
            workspace,
            11,
            paused.aggregate_revision(),
            WorkbenchIntent::ResumeGoal { goal: paused.goal() },
        );
        assert!(matches!(
            service.workbench_command(actor(), &resume).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_eq!(wait_for_terminal(&service, run).await.phase(), ProductRunPhase::WaitingForUser);
        let waiting = match service.workbench_goal(actor(), query(workspace)) {
            AppResponsePayload::WorkbenchGoal(goal) => goal,
            response => panic!("expected resumed goal, got {response:?}"),
        };
        assert_eq!(waiting.run(), run);
        assert_eq!(waiting.attempt(), 2);
        assert_eq!(waiting.state(), WorkbenchGoalState::WaitingForUser);
        assert_eq!(waiting.usage().requests(), 2);
        assert_eq!(writer.requests.lock().expect("requests").len(), 2);
        service.shutdown().await.expect("shutdown product runs");
        drop(service);
        let controls = crate::product_control::ControlStore::open(
            &state.path().join("workbench-v1"),
            peritus_journal::StoreId::new([0x7f; 16]).expect("store"),
        )
        .expect("restart controls");
        let records = crate::product_run::persistence::load_workbench_records(
            &state.path().join("workbench-v1"),
            Some(&controls),
        )
        .expect("restart runs");
        assert!(records.contains_key(&run));
        let directory = state.path().join("workbench-v1/runs");
        let filename = format!(
            "{}.json",
            run.as_bytes().iter().fold(String::new(), |mut hex, byte| {
                use core::fmt::Write as _;
                write!(hex, "{byte:02x}").unwrap();
                hex
            })
        );
        let original = directory.join(&filename);
        let quarantine = directory.join(".quarantine");
        fs::create_dir_all(&quarantine).unwrap();
        fs::rename(&original, quarantine.join(&filename)).unwrap();
        fs::write(quarantine.join("broken.json"), b"not a projection").unwrap();
        let records = crate::product_run::persistence::load_workbench_records(
            &state.path().join("workbench-v1"),
            Some(&controls),
        )
        .expect("recover previously rejected goal");
        assert!(records.contains_key(&run));
        assert!(original.is_file());
        assert!(quarantine.join(&filename).is_file());
        assert_eq!(fs::read(quarantine.join("broken.json")).unwrap(), b"not a projection");
        assert!(!quarantine.join(".quarantine").exists());
        let restored = super::service(
            state.path(),
            repository.path(),
            workspace,
            [&writer, &reviewer, &fixer],
        );
        {
            let cancellation = peritus_journal::JournalCancellation::new();
            let _permit = restored.inner.controls.acquire(&cancellation).expect("queue owner");
            *restored.inner.controls.owner.lock().expect("owner") = Some(controls);
            *restored.inner.records.write().expect("records") = records;
        }
        assert!(
            restored.inner.records.read().expect("records").contains_key(&run),
            "a durable goal run must survive daemon restart"
        );
        let AppResponsePayload::WorkbenchGoal(reopened) =
            restored.workbench_goal(actor(), query(workspace))
        else {
            panic!("goal must survive restart");
        };
        assert_eq!(reopened.state(), waiting.state());
        assert_eq!(reopened.usage().requests(), waiting.usage().requests());
        restored.shutdown().await.expect("shutdown product runs");
    });
}

#[test]
fn goal_continues_after_the_first_provider_request_without_a_budget() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x71,
            "unbounded-goal-writer",
            vec![
                support::named_tool_response(
                    "workspace_list",
                    br#"{"path":"","depth":1}"#.to_vec(),
                ),
                support::text_response(b"Continued after inspecting the workspace."),
            ],
        );
        let reviewer = scripted(0x72, "unbounded-goal-reviewer", Vec::new());
        let fixer = scripted(0x73, "unbounded-goal-fixer", Vec::new());
        let workspace = WorkspaceId::new([0x74; 16]).expect("workspace");
        let run = RunId::new([0x75; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let objective = WorkbenchInputText::new("Inspect and answer.".to_owned()).unwrap();
        assert!(matches!(
            service
                .workbench_command(
                    actor(),
                    &command(
                        workspace,
                        8,
                        3,
                        WorkbenchIntent::SetBrief {
                            field: WorkbenchBriefField::Objective,
                            text: objective.clone(),
                        },
                    ),
                )
                .await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let definition = WorkbenchGoalDefinition::new(
            objective,
            vec![WorkbenchGoalCriterionDefinition::new(
                WorkbenchGoalCriterionKind::RunnerAcceptance,
                WorkbenchInputText::new("Strict runner acceptance".to_owned()).unwrap(),
                true,
            )],
        )
        .unwrap();
        let settings = WorkbenchExecutionSettings::new(
            run,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
        );
        let response = service
            .workbench_command(
                actor(),
                &command(workspace, 9, 4, WorkbenchIntent::StartGoal { definition, settings }),
            )
            .await;
        assert!(matches!(response, AppResponsePayload::WorkbenchReceipt(_)));
        let stopped = wait_for_terminal(&service, run).await;
        assert_eq!(stopped.phase(), ProductRunPhase::WaitingForUser);
        let goal = match service.workbench_goal(actor(), query(workspace)) {
            AppResponsePayload::WorkbenchGoal(goal) => goal,
            response => panic!("expected waiting goal, got {response:?}"),
        };
        assert_eq!(goal.state(), WorkbenchGoalState::WaitingForUser);
        assert_eq!(goal.usage().requests(), 2);
        assert_eq!(goal.usage().tool_calls(), 1);
        assert_eq!(writer.requests.lock().expect("requests").len(), 2);
        service.shutdown().await.expect("shutdown product runs");
    });
}
