use super::*;
use std::path::Path;

pub(super) async fn start_goal(
    service: &ProductRunService,
    workspace: WorkspaceId,
    run: RunId,
    providers: [&Arc<ScriptedProvider>; 3],
) {
    queue(service, workspace).await;
    let objective = WorkbenchInputText::new("Explain the current workspace.".into()).unwrap();
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
    let definition = WorkbenchGoalDefinition::new(
        objective,
        vec![WorkbenchGoalCriterionDefinition::new(
            WorkbenchGoalCriterionKind::RunnerAcceptance,
            WorkbenchInputText::new("Strict runner acceptance".into()).unwrap(),
            true,
        )],
    )
    .unwrap();
    let settings = WorkbenchExecutionSettings::new(
        run,
        ProductProviderSelection::new(
            providers[0].profile.profile_id(),
            providers[1].profile.profile_id(),
            providers[2].profile.profile_id(),
        ),
        ProductInteractionMode::Chat,
        ProductRoleModels::default(),
    );
    let start = command(workspace, 9, 4, WorkbenchIntent::StartGoal { definition, settings });
    assert!(matches!(
        service.workbench_command(actor(), &start).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
}

pub(super) fn goal(
    service: &ProductRunService,
    workspace: WorkspaceId,
) -> peritus_app_protocol::WorkbenchGoalSnapshot {
    let AppResponsePayload::WorkbenchGoal(goal) = service.workbench_goal(actor(), query(workspace))
    else {
        panic!("durable goal")
    };
    goal
}

pub(super) async fn pause_idle(service: &ProductRunService, workspace: WorkspaceId) {
    let waiting = goal(service, workspace);
    let pause = command(
        workspace,
        10,
        waiting.aggregate_revision(),
        WorkbenchIntent::PauseGoal { goal: waiting.goal(), mode: WorkbenchGoalPauseMode::Now },
    );
    assert!(matches!(
        service.workbench_command(actor(), &pause).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    assert_eq!(goal(service, workspace).state(), WorkbenchGoalState::Paused);
}

pub(super) fn resume_command(
    service: &ProductRunService,
    workspace: WorkspaceId,
) -> WorkbenchCommand {
    let current = goal(service, workspace);
    command(
        workspace,
        12,
        current.aggregate_revision(),
        WorkbenchIntent::ResumeGoal { goal: current.goal() },
    )
}

pub(super) fn accept_without_launch(service: &ProductRunService, command: &WorkbenchCommand) {
    use peritus_product_runner::control::{ControlIntent, ControlOperation, OperationId};
    let WorkbenchIntent::ResumeGoal { goal } = command.intent() else { panic!("resume intent") };
    let operation = ControlOperation::new(
        OperationId::new(command.operation().into_bytes()).unwrap(),
        peritus_product_runner::control::ConversationId::new(
            command.query().conversation().into_bytes(),
        )
        .unwrap(),
        actor(),
        command.query().workspace(),
        command.expected_revision(),
        ControlIntent::ResumeGoal {
            goal: OperationId::new(goal.into_bytes()).unwrap(),
            now_unix_millis: 1_000,
        },
    );
    service.with_controls(false, |store| store.accept(&operation)).unwrap();
}

pub(super) fn restore(
    state: &Path,
    repository: &Path,
    workspace: WorkspaceId,
    providers: [&Arc<ScriptedProvider>; 3],
) -> ProductRunService {
    let controls = crate::product_control::ControlStore::open(
        &state.join("workbench-v1"),
        peritus_journal::StoreId::new([0x7f; 16]).unwrap(),
    )
    .unwrap();
    let records = crate::product_run::persistence::load_workbench_records(
        &state.join("workbench-v1"),
        Some(&controls),
    )
    .unwrap();
    let service = service(state, repository, workspace, providers);
    {
        let cancellation = peritus_journal::JournalCancellation::new();
        let _permit = service.inner.controls.acquire(&cancellation).unwrap();
        *service.inner.controls.owner.lock().unwrap() = Some(controls);
        *service.inner.records.write().unwrap() = records;
    }
    service
}

#[test]
fn a_goal_paused_at_an_idle_boundary_resumes_after_restart_without_new_input() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().unwrap();
        let writer = scripted(
            0x61,
            "idle-goal-writer",
            vec![
                support::text_response(b"First answer."),
                support::text_response(b"Explicitly resumed answer."),
            ],
        );
        let reviewer = scripted(0x62, "idle-goal-reviewer", Vec::new());
        let fixer = scripted(0x63, "idle-goal-fixer", Vec::new());
        let workspace = WorkspaceId::new([0x64; 16]).unwrap();
        let run = RunId::new([0x65; 16]).unwrap();
        let original =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        start_goal(&original, workspace, run, [&writer, &reviewer, &fixer]).await;
        assert_eq!(
            wait_for_terminal(&original, run).await.phase(),
            ProductRunPhase::WaitingForUser
        );
        assert!(
            original.retry(run).await.is_err(),
            "idle work cannot silently retry without resume authority"
        );
        pause_idle(&original, workspace).await;
        original.shutdown().await.expect("shutdown product runs");
        drop(original);

        let restored =
            restore(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        assert!(restored.retry(run).await.is_err(), "restart does not resume paused work");
        let resume = resume_command(&restored, workspace);
        let response = restored.workbench_command(actor(), &resume).await;
        assert!(matches!(response, AppResponsePayload::WorkbenchReceipt(_)), "{response:?}");
        assert_eq!(
            wait_for_terminal(&restored, run).await.phase(),
            ProductRunPhase::WaitingForUser
        );
        let resumed = goal(&restored, workspace);
        assert_eq!((resumed.run(), resumed.attempt(), resumed.usage().requests()), (run, 2, 2));
        assert_eq!(writer.requests.lock().unwrap().len(), 2);
        let replay = restored.workbench_command(actor(), &resume).await;
        assert_eq!(replay, response, "an exact resume replay must not admit another attempt");
        assert_eq!(writer.requests.lock().unwrap().len(), 2);
        restored.shutdown().await.expect("shutdown product runs");
    });
}

#[test]
fn committed_resume_without_launch_recovers_once_on_exact_replay_after_restart() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().unwrap();
        let writer = scripted(
            0x81,
            "resume-gap-writer",
            vec![
                support::text_response(b"First answer."),
                support::text_response(b"Recovered answer."),
            ],
        );
        let reviewer = scripted(0x82, "resume-gap-reviewer", Vec::new());
        let fixer = scripted(0x83, "resume-gap-fixer", Vec::new());
        let workspace = WorkspaceId::new([0x84; 16]).unwrap();
        let run = RunId::new([0x85; 16]).unwrap();
        let original =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        start_goal(&original, workspace, run, [&writer, &reviewer, &fixer]).await;
        assert_eq!(
            wait_for_terminal(&original, run).await.phase(),
            ProductRunPhase::WaitingForUser
        );
        pause_idle(&original, workspace).await;
        let resume = resume_command(&original, workspace);
        accept_without_launch(&original, &resume);
        assert_eq!(goal(&original, workspace).attempt(), 2);
        original.shutdown().await.expect("shutdown product runs");
        drop(original);
        assert_eq!(writer.requests.lock().unwrap().len(), 1);

        let restored =
            restore(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        let first_owner = restored.clone();
        let first_command = resume.clone();
        let first_request =
            tokio::spawn(
                async move { first_owner.workbench_command(actor(), &first_command).await },
            );
        let duplicate_owner = restored.clone();
        let duplicate_command = resume.clone();
        let duplicate_request = tokio::spawn(async move {
            duplicate_owner.workbench_command(actor(), &duplicate_command).await
        });
        let first = first_request.await.unwrap();
        let duplicate = duplicate_request.await.unwrap();
        assert!(matches!(first, AppResponsePayload::WorkbenchReceipt(_)), "{first:?}");
        assert_eq!(first, duplicate);
        assert_eq!(
            wait_for_terminal(&restored, run).await.phase(),
            ProductRunPhase::WaitingForUser
        );
        assert_eq!(
            (goal(&restored, workspace).attempt(), writer.requests.lock().unwrap().len()),
            (2, 2)
        );
        restored.shutdown().await.expect("shutdown product runs");
        drop(restored);

        let reopened =
            restore(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        assert_eq!(reopened.workbench_command(actor(), &resume).await, first);
        assert_eq!(writer.requests.lock().unwrap().len(), 2);
        reopened.shutdown().await.expect("shutdown product runs");
    });
}

#[test]
fn replay_of_unlaunched_resume_cannot_override_a_later_goal_clear() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().unwrap();
        let writer =
            scripted(0x91, "resume-clear-writer", vec![support::text_response(b"First answer.")]);
        let reviewer = scripted(0x92, "resume-clear-reviewer", Vec::new());
        let fixer = scripted(0x93, "resume-clear-fixer", Vec::new());
        let workspace = WorkspaceId::new([0x94; 16]).unwrap();
        let run = RunId::new([0x95; 16]).unwrap();
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        start_goal(&service, workspace, run, [&writer, &reviewer, &fixer]).await;
        wait_for_terminal(&service, run).await;
        pause_idle(&service, workspace).await;
        let resume = resume_command(&service, workspace);
        accept_without_launch(&service, &resume);
        let active = goal(&service, workspace);
        let clear = command(
            workspace,
            13,
            active.aggregate_revision(),
            WorkbenchIntent::ClearGoal { goal: active.goal() },
        );
        assert!(matches!(
            service.workbench_command(actor(), &clear).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert!(matches!(
            service.workbench_command(actor(), &resume).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_eq!(goal(&service, workspace).state(), WorkbenchGoalState::Cancelled);
        assert_eq!(writer.requests.lock().unwrap().len(), 1);
        let stranger = ActorId::new([0x99; 16]).unwrap();
        assert!(matches!(
            service.workbench_command(stranger, &resume).await,
            AppResponsePayload::Error(_)
        ));
        service.shutdown().await.expect("shutdown product runs");
    });
}
