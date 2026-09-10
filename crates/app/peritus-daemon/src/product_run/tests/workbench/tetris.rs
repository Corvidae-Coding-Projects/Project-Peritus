//! Ignored V1 composition proving governed image-to-build-to-native-playtest evidence.

#![cfg(target_os = "linux")]

use super::*;
use peritus_app_protocol::WorkbenchPreviewInput;

#[path = "tetris/support.rs"]
mod fixture_support;
use fixture_support::*;

#[test]
#[ignore = "requires Xvfb, xdotool, ImageMagick import, and Python Tk"]
fn v1_image_build_and_controlled_tetris_playtest_preserve_exact_evidence() {
    interaction::block_on(async {
        let repository = tetris_repository();
        let state = tempfile::tempdir().expect("state");
        let (mut xvfb, display) = preview::start_xvfb();
        let writer = support::scripted_images(0xd1, "tetris-writer", writer_script());
        let reviewer = support::scripted_images(0xd2, "tetris-reviewer", reviewer_script());
        let fixer = support::scripted_images(0xd3, "tetris-fixer", Vec::new());
        let workspace = WorkspaceId::new([0xd4; 16]).expect("workspace");
        let run = RunId::new([0xd5; 16]).expect("run");
        let mut service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        Arc::get_mut(&mut service.inner)
            .expect("exclusive service before launch")
            .preview_capture = super::super::super::PreviewCaptureHost {
            display: Some(display.clone()),
            program: Some(std::path::PathBuf::from("/usr/bin/import")),
        };
        let session = SessionId::new([0xd6; 16]).expect("session");
        let (authority, authority_task) = preview::test_authority(state.path(), session);

        queue_tetris(&service, workspace).await;
        attach_image(&service, &authority, session, workspace, &writer).await;
        let objective = WorkbenchInputText::new(
            "Use the confirmed visual reference and build a playable native Tk Tetris preview."
                .to_owned(),
        )
        .expect("objective");
        accept(
            &service,
            workspace,
            0xda,
            4,
            WorkbenchIntent::SetBrief {
                field: WorkbenchBriefField::Objective,
                text: objective.clone(),
            },
        )
        .await;
        let definition = WorkbenchGoalDefinition::new(
            objective,
            vec![
                criterion(WorkbenchGoalCriterionKind::RunnerAcceptance, "Qualified candidate"),
                criterion(WorkbenchGoalCriterionKind::GraphicalPlaytest, "Controlled native play"),
            ],
            WorkbenchGoalBudget::new(None, None, None, None).expect("goal budget"),
        )
        .expect("goal");
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
        accept(&service, workspace, 0xdb, 5, WorkbenchIntent::StartGoal { definition, settings })
            .await;
        let terminal = wait_for_terminal(&service, run).await;
        assert_eq!(
            terminal.phase(),
            ProductRunPhase::Complete,
            "status={} summary={} gates={} review={} writer_requests={} scripts_left={}",
            terminal.status(),
            terminal.summary(),
            terminal.gates(),
            terminal.review(),
            writer.requests.lock().expect("requests").len(),
            writer.responses.lock().expect("scripts").len(),
        );
        assert_eq!(fs::read_to_string(repository.path().join("tetris.py")).unwrap(), TETRIS_SOURCE);
        assert_image_reached_writer(&writer);
        assert_goal(&service, workspace, WorkbenchGoalState::WaitingForUser, false);

        let revision = match service.workbench_query(actor(), query(workspace)) {
            AppResponsePayload::Workbench(value) => value.revision(),
            response => panic!("workbench snapshot: {response:?}"),
        };
        accept_preview(
            &service,
            &authority,
            session,
            workspace,
            0xdc,
            revision,
            WorkbenchIntent::StartPreview(launch_profile(run, repository.path(), &display)),
        )
        .await;
        let launch = operation(0xdc);
        let window = preview::find_window(&display, "Peritus Tetris V1");
        for (id, input) in
            [(0xdd, "LEFT\n"), (0xde, "RIGHT\n"), (0xdf, "ROTATE\n"), (0xe0, "DROP\n")]
        {
            accept_preview(
                &service,
                &authority,
                session,
                workspace,
                id,
                revision,
                WorkbenchIntent::InteractPreview {
                    launch,
                    input: WorkbenchPreviewInput::new(input.as_bytes().to_vec()).expect("input"),
                },
            )
            .await;
        }
        std::thread::sleep(Duration::from_millis(200));
        accept_preview(
            &service,
            &authority,
            session,
            workspace,
            0xe1,
            revision,
            WorkbenchIntent::CapturePreview(WorkbenchCaptureRequest::new(
                launch,
                WorkbenchCaptureTarget::x11_window(window).expect("selected window"),
                WorkbenchCaptureConsent::Granted,
            )),
        )
        .await;
        let capture = operation(0xe1);
        for (id, intent) in [
            (0xe2, WorkbenchIntent::StopPreview { launch }),
            (
                0xe3,
                WorkbenchIntent::AddArtifactFeedback {
                    capture,
                    feedback: WorkbenchReviewFeedback::KeepBehavior,
                    message: input_text("Keep this controlled piece movement and visible board."),
                    region: None,
                },
            ),
            (
                0xe4,
                WorkbenchIntent::CheckPreviewBehavior {
                    launch,
                    observed: text(OBSERVED),
                    note: input_text("Controlled native play"),
                },
            ),
        ] {
            accept_preview(&service, &authority, session, workspace, id, revision, intent).await;
        }

        assert_goal(&service, workspace, WorkbenchGoalState::Achieved, true);
        preserve_and_assert_result(&service, &authority, workspace, run, capture, window).await;
        service.shutdown(Duration::from_secs(5)).await;
        authority.stop().await.expect("stop authority");
        authority_task.await.expect("authority task").expect("authority shutdown");
        xvfb.kill().expect("stop Xvfb");
        let _ = xvfb.wait();
    });
}

fn tetris_repository() -> tempfile::TempDir {
    let repository = repository();
    fs::write(repository.path().join("src/lib.rs"), "pub const fn before() -> u32 {\n    1\n}\n")
        .expect("formatted fixture");
    let output = std::process::Command::new("git")
        .args(["commit", "--all", "--quiet", "--amend", "--no-edit"])
        .current_dir(repository.path())
        .output()
        .expect("amend fixture baseline");
    assert!(output.status.success(), "git: {}", String::from_utf8_lossy(&output.stderr));
    repository
}

async fn queue_tetris(service: &ProductRunService, workspace: WorkspaceId) {
    accept(
        service,
        workspace,
        3,
        0,
        WorkbenchIntent::CreateConversation(
            ConversationTitle::new("governed Tetris V1".to_owned()).expect("title"),
        ),
    )
    .await;
    let input = WorkbenchInputId::new([4; 16]).expect("input");
    accept(
        service,
        workspace,
        5,
        1,
        WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
            WorkbenchNewInput::new(
                input,
                input_text("OBSOLETE_NEVER_SEND"),
                WorkbenchInputOrder::new(Vec::new()).expect("dependencies"),
            )
            .expect("input"),
        )),
    )
    .await;
    accept(
        service,
        workspace,
        6,
        2,
        WorkbenchIntent::Queue(WorkbenchQueueIntent::Edit {
            selected: WorkbenchInputSelection::new(input, 1).expect("selection"),
            text: input_text("Build playable Tetris with LEFT, RIGHT, ROTATE, and DROP controls."),
        }),
    )
    .await;
}
