//! Explicit native qualification for graphical goal evidence.

#![cfg(target_os = "linux")]

use super::*;
use peritus_app_protocol::{WorkbenchBuildIdentity, WorkbenchGoalCriterionState};

mod native;

pub(super) use native::{find_window, start_xvfb, test_authority};

#[test]
#[ignore = "requires Xvfb, xdotool, ImageMagick import, and Python Tk"]
fn daemon_preview_evidence_satisfies_only_the_graphical_goal_criterion() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let (mut xvfb, display) = start_xvfb();
        let preview_path = repository.path().join("preview.py");
        fs::write(
            &preview_path,
            format!(
                r#"import os, sys, threading, tkinter as tk
os.environ["DISPLAY"] = {display:?}
root = tk.Tk()
root.title("Peritus Controlled Preview")
state = tk.StringVar(value="state=0")
label = tk.Label(root, textvariable=state, width=24, height=6)
label.pack()
print("READY state=0", flush=True)
def move_right():
    state.set("state=1 moved-right")
    label.pack_configure(padx=(100, 0))
def reader():
    for line in sys.stdin:
        value = line.strip()
        if value == "MOVE_RIGHT":
            root.after(0, move_right)
            print("OBSERVED MOVE_RIGHT state=1", flush=True)
threading.Thread(target=reader, daemon=True).start()
root.mainloop()
"#
            ),
        )
        .expect("controlled preview source");
        let writer = scripted(0xb1, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0xb2, "reviewer", clean_review());
        let fixer = scripted(0xb3, "fixer", Vec::new());
        let workspace = WorkspaceId::new([0xb4; 16]).expect("workspace");
        let run = RunId::new([0xb5; 16]).expect("run");
        let mut service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        Arc::get_mut(&mut service.inner)
            .expect("exclusive service before launch")
            .preview_capture = super::super::super::PreviewCaptureHost {
            display: Some(display.clone()),
            program: Some(std::path::PathBuf::from("/usr/bin/import")),
        };
        let session = SessionId::new([0xba; 16]).expect("session");
        let (authority, authority_task) = test_authority(state.path(), session);
        queue(&service, workspace).await;
        let objective = WorkbenchInputText::new("Ship the playable native preview.".to_owned())
            .expect("objective");
        let brief = command(
            workspace,
            0xbd,
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
            ProductInteractionMode::Build,
            ProductRoleModels::default(),
        );
        let definition = WorkbenchGoalDefinition::new(
            objective,
            vec![
                WorkbenchGoalCriterionDefinition::new(
                    WorkbenchGoalCriterionKind::RunnerAcceptance,
                    WorkbenchInputText::new("Strict runner acceptance".to_owned())
                        .expect("runner criterion"),
                    true,
                ),
                WorkbenchGoalCriterionDefinition::new(
                    WorkbenchGoalCriterionKind::GraphicalPlaytest,
                    WorkbenchInputText::new("Native selected-window playtest".to_owned())
                        .expect("graphical criterion"),
                    true,
                ),
            ],
            WorkbenchGoalBudget::new(None, None, None, None).expect("budget"),
        )
        .expect("goal definition");
        let start =
            command(workspace, 0xbe, 4, WorkbenchIntent::StartGoal { definition, settings });
        assert!(matches!(
            service.workbench_command(actor(), &start).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let terminal = wait_for_terminal(&service, run).await;
        assert_eq!(terminal.phase(), ProductRunPhase::Complete, "{}", terminal.summary());

        let before = goal_snapshot(&service, workspace);
        assert_eq!(before.state(), WorkbenchGoalState::WaitingForUser);
        assert_criterion(
            &before,
            WorkbenchGoalCriterionKind::RunnerAcceptance,
            WorkbenchGoalCriterionState::Satisfied,
        );
        assert_criterion(
            &before,
            WorkbenchGoalCriterionKind::GraphicalPlaytest,
            WorkbenchGoalCriterionState::Unavailable,
        );

        let current = match service.workbench_query(actor(), query(workspace)) {
            AppResponsePayload::Workbench(value) => value.revision(),
            value => panic!("workbench snapshot: {value:?}"),
        };
        let text = |value: &str| WorkbenchLaunchText::new(value.to_owned()).expect("launch text");
        let preview_digest = peritus_codec::sha256(
            &fs::read(&preview_path).expect("read exact preview build identity"),
        );
        let profile = WorkbenchLaunchProfile::new(
            run,
            text("python3"),
            vec![text("preview.py")],
            text("."),
            Vec::new(),
            WorkbenchLaunchSource::new(
                WorkbenchLaunchSourceKind::ManagedCandidate,
                text("."),
                peritus_product_runner::ProductRunner::candidate_digest(repository.path())
                    .expect("candidate digest"),
            ),
            Some(WorkbenchBuildIdentity::new(text("preview.py"), preview_digest)),
            2_000,
            20_000,
            true,
        )
        .expect("launch profile");
        let launch = command(workspace, 0xb6, current, WorkbenchIntent::StartPreview(profile));
        assert!(matches!(
            preview_command(&service, &authority, session, &launch).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let window = find_window(&display, "Peritus Controlled Preview");
        let interaction = command(
            workspace,
            0xb7,
            current,
            WorkbenchIntent::InteractPreview {
                launch: launch.operation(),
                input: peritus_app_protocol::WorkbenchPreviewInput::new(b"MOVE_RIGHT\n".to_vec())
                    .expect("input"),
            },
        );
        assert!(matches!(
            preview_command(&service, &authority, session, &interaction).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        std::thread::sleep(Duration::from_millis(100));
        let capture = command(
            workspace,
            0xb8,
            current,
            WorkbenchIntent::CapturePreview(WorkbenchCaptureRequest::new(
                launch.operation(),
                WorkbenchCaptureTarget::x11_window(window).expect("selected window"),
                WorkbenchCaptureConsent::Granted,
            )),
        );
        assert!(matches!(
            preview_command(&service, &authority, session, &capture).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_criterion(
            &goal_snapshot(&service, workspace),
            WorkbenchGoalCriterionKind::GraphicalPlaytest,
            WorkbenchGoalCriterionState::Unavailable,
        );
        let stop = command(
            workspace,
            0xb9,
            current,
            WorkbenchIntent::StopPreview { launch: launch.operation() },
        );
        assert!(matches!(
            preview_command(&service, &authority, session, &stop).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let refused = |id, note: &str| {
            command(
                workspace,
                id,
                current,
                WorkbenchIntent::CheckPreviewBehavior {
                    launch: launch.operation(),
                    observed: text("OBSERVED MOVE_RIGHT state=1"),
                    note: WorkbenchInputText::new(note.to_owned()).unwrap(),
                },
            )
        };
        let wrong = refused(0xba, "Unrelated criterion");
        assert!(matches!(
            preview_command(&service, &authority, session, &wrong).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_criterion(
            &goal_snapshot(&service, workspace),
            WorkbenchGoalCriterionKind::GraphicalPlaytest,
            WorkbenchGoalCriterionState::Unavailable,
        );
        let owned =
            service.inner.preview_processes.lock().unwrap().remove(&launch.operation()).unwrap();
        let foreign = refused(0xbc, "Native selected-window playtest");
        assert!(matches!(
            preview_command(&service, &authority, session, &foreign).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_criterion(
            &goal_snapshot(&service, workspace),
            WorkbenchGoalCriterionKind::GraphicalPlaytest,
            WorkbenchGoalCriterionState::Unavailable,
        );
        service.inner.preview_processes.lock().unwrap().insert(launch.operation(), owned);
        let original = fs::read(&preview_path).unwrap();
        fs::write(&preview_path, b"stale source and build\n").unwrap();
        let outdated = refused(0xbd, "Native selected-window playtest");
        assert!(matches!(
            preview_command(&service, &authority, session, &outdated).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_criterion(
            &goal_snapshot(&service, workspace),
            WorkbenchGoalCriterionKind::GraphicalPlaytest,
            WorkbenchGoalCriterionState::Unavailable,
        );
        fs::write(&preview_path, original).unwrap();
        let behavior = command(
            workspace,
            0xbb,
            current,
            WorkbenchIntent::CheckPreviewBehavior {
                launch: launch.operation(),
                observed: text("OBSERVED MOVE_RIGHT state=1"),
                note: WorkbenchInputText::new("Native selected-window playtest".to_owned())
                    .expect("behavior note"),
            },
        );
        assert!(matches!(
            preview_command(&service, &authority, session, &behavior).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));

        let after = goal_snapshot(&service, workspace);
        assert_criterion(
            &after,
            WorkbenchGoalCriterionKind::RunnerAcceptance,
            WorkbenchGoalCriterionState::Satisfied,
        );
        assert_criterion(
            &after,
            WorkbenchGoalCriterionKind::GraphicalPlaytest,
            WorkbenchGoalCriterionState::Satisfied,
        );
        assert_eq!(after.state(), WorkbenchGoalState::Achieved);

        let page = service
            .result_page(actor(), WorkbenchResultQuery::new(query(workspace), run))
            .expect("result page");
        let result = &page.launches()[0];
        assert_eq!(result.state(), WorkbenchLaunchState::Stopped);
        assert!(result.process().is_some());
        assert!(result.ready());
        assert_eq!(result.behavior_checks(), 4);
        assert_eq!(result.captures()[0].state(), WorkbenchCaptureState::Captured);
        let artifact = result.captures()[0].artifact().expect("published artifact");
        let scope = crate::artifact::ArtifactScope::new(actor(), query(workspace));
        let (catalog, bytes) = authority
            .read_scoped_artifact(scope, artifact, 16 * 1_024 * 1_024)
            .await
            .expect("published capture bytes");
        assert_eq!(catalog.digest(), result.captures()[0].image_digest().expect("digest"));
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        service.shutdown(Duration::from_secs(5)).await;
        authority.stop().await.expect("stop authority");
        authority_task.await.expect("authority task").expect("authority shutdown");
        xvfb.kill().expect("stop Xvfb");
        let _ = xvfb.wait();
    });
}

pub(super) fn goal_snapshot(
    service: &ProductRunService,
    workspace: WorkspaceId,
) -> peritus_app_protocol::WorkbenchGoalSnapshot {
    match service.workbench_goal(actor(), query(workspace)) {
        AppResponsePayload::WorkbenchGoal(goal) => goal,
        response => panic!("expected goal snapshot, got {response:?}"),
    }
}

pub(super) fn assert_criterion(
    goal: &peritus_app_protocol::WorkbenchGoalSnapshot,
    kind: WorkbenchGoalCriterionKind,
    expected: WorkbenchGoalCriterionState,
) {
    let criterion = goal
        .criteria()
        .iter()
        .find(|criterion| criterion.definition().kind() == kind)
        .expect("criterion");
    assert_eq!(criterion.state(), expected);
}

pub(super) async fn preview_command(
    service: &ProductRunService,
    authority: &crate::AuthorityHandle,
    session: SessionId,
    command: &WorkbenchCommand,
) -> AppResponsePayload {
    service
        .workbench_preview_command(
            authority,
            actor(),
            session,
            CorrelationId::new(command.operation().into_bytes()).expect("correlation"),
            64 * 1_024,
            command,
        )
        .await
}
