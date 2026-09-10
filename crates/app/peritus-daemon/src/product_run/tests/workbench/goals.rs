//! Composed goals behavior through the actual daemon service.

use super::*;

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
            WorkbenchGoalBudget::new(None, Some(4), Some(4), None).expect("budget"),
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
        let pause = command(
            workspace,
            10,
            active.aggregate_revision(),
            WorkbenchIntent::PauseGoal { goal: active.goal(), mode: WorkbenchGoalPauseMode::Now },
        );
        assert!(matches!(
            service.workbench_command(actor(), &pause).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_eq!(wait_for_terminal(&service, run).await.phase(), ProductRunPhase::Cancelled);
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
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn configured_goal_request_budget_stops_before_a_second_provider_call() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x71,
            "budget-writer",
            vec![support::named_tool_response(
                "workspace_list",
                br#"{"path":"","depth":1}"#.to_vec(),
            )],
        );
        let reviewer = scripted(0x72, "budget-reviewer", Vec::new());
        let fixer = scripted(0x73, "budget-fixer", Vec::new());
        let workspace = WorkspaceId::new([0x74; 16]).expect("workspace");
        let run = RunId::new([0x75; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let objective =
            WorkbenchInputText::new("Stop exactly at the request limit.".to_owned()).unwrap();
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
            WorkbenchGoalBudget::new(None, Some(1), Some(4), None).unwrap(),
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
        assert_eq!(wait_for_terminal(&service, run).await.phase(), ProductRunPhase::Cancelled);
        let goal = match service.workbench_goal(actor(), query(workspace)) {
            AppResponsePayload::WorkbenchGoal(goal) => goal,
            response => panic!("expected budget-reached goal, got {response:?}"),
        };
        assert_eq!(goal.state(), WorkbenchGoalState::BudgetReached);
        assert_eq!(goal.usage().requests(), 1);
        assert_eq!(goal.usage().tool_calls(), 1);
        assert_eq!(writer.requests.lock().expect("requests").len(), 1);
        assert!(goal.reason().contains("no new operation"));
        service.shutdown(Duration::from_secs(5)).await;
    });
}
