//! Live prompts, input, authorization, and restart-retained output through the public projection.
use super::*;
use peritus_app_protocol::{WorkbenchLaunchState, WorkbenchPreviewSnapshot, WorkbenchResultQuery};
mod terminal;

#[test]
#[ignore = "owned subprocess fixture"]
fn preview_prompt_fixture() {
    use std::io::{BufRead as _, Write as _};
    println!("NAME? ");
    std::io::stdout().flush().expect("prompt");
    let name = std::io::stdin().lock().lines().next().expect("input").expect("line");
    println!("HELLO {name}");
    eprintln!("GOODBYE diagnostic");
}

#[test]
fn preview_prompt_is_visible_before_input_and_retained_after_restart() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x71, "chat", vec![support::text_response(b"Ready.")]);
        let reviewer = scripted(0x72, "review", Vec::new());
        let fixer = scripted(0x73, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x74; 16]).expect("workspace");
        let run = RunId::new([0x75; 16]).expect("run");
        let running =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&running, workspace).await;
        assert!(matches!(
            running
                .workbench_command(actor(), &start(workspace, run, [&writer, &reviewer, &fixer]))
                .await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        wait_for_terminal(&running, run).await;
        let AppResponsePayload::Workbench(snapshot) =
            running.workbench_query(actor(), query(workspace))
        else {
            panic!("scope")
        };
        let text = |value: &str| WorkbenchLaunchText::new(value.to_owned()).expect("text");
        let executable = std::env::current_exe().expect("executable");
        let profile = WorkbenchLaunchProfile::new(
            run,
            text(&executable.to_string_lossy()),
            [
                "--ignored",
                "--exact",
                "product_run::tests::workbench::preview_output::preview_prompt_fixture",
                "--nocapture",
            ]
            .into_iter()
            .map(text)
            .collect(),
            text("."),
            Vec::new(),
            WorkbenchLaunchSource::new(
                WorkbenchLaunchSourceKind::ManagedCandidate,
                text("."),
                peritus_product_runner::ProductRunner::candidate_digest(repository.path())
                    .expect("digest"),
            ),
            None,
            2000,
            20000,
            true,
        )
        .expect("profile");
        let rename = command(
            workspace,
            0x78,
            snapshot.revision(),
            WorkbenchIntent::RenameConversation(
                ConversationTitle::new("Renamed while previewing".to_owned()).expect("title"),
            ),
        );
        assert!(matches!(
            running.workbench_command(actor(), &rename).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let launch =
            command(workspace, 0x76, snapshot.revision(), WorkbenchIntent::StartPreview(profile));
        let result = running.workbench_command(actor(), &launch).await;
        assert!(matches!(result, AppResponsePayload::WorkbenchReceipt(_)), "{result:?}");
        assert_eq!(running.workbench_receipt(actor(), &launch), result);
        assert!(matches!(
            running.workbench_receipt(ActorId::new([99; 16]).unwrap(), &launch),
            AppResponsePayload::Error(_)
        ));
        let result_query = WorkbenchResultQuery::new(query(workspace), run);
        assert!(matches!(
            running.workbench_preview(ActorId::new([99; 16]).expect("stranger"), result_query),
            AppResponsePayload::Error(_)
        ));
        let live = wait_for_output(&running, result_query, "NAME?", false).await;
        assert_eq!(live.result().launches()[0].state(), WorkbenchLaunchState::Running);
        let revision = live.result().result_revision();
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(
            observe(&running, result_query).result().result_revision(),
            revision,
            "unchanged polling must not mutate result state"
        );
        let terminal_attachment = terminal::attach_and_reconnect(&running, &live);
        terminal_attachment.check_permission_changes(&running, workspace).await;
        let input = command(
            workspace,
            0x77,
            snapshot.revision(),
            WorkbenchIntent::InteractPreview {
                launch: launch.operation(),
                input: peritus_app_protocol::WorkbenchPreviewInput::new(b"da\n".to_vec())
                    .expect("input"),
            },
        );
        assert!(matches!(
            running.workbench_command(actor(), &input).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let terminal = wait_for_output(&running, result_query, "GOODBYE diagnostic", true).await;
        assert!(terminal.outputs()[0].stdout().contains("HELLO Ada"));
        terminal_attachment.finish().await;
        running.shutdown().await.expect("shutdown product runs");
        drop(running);
        let controls = crate::product_control::ControlStore::open(
            &state.path().join("workbench-v1"),
            peritus_journal::StoreId::new([0x7f; 16]).expect("store"),
        )
        .expect("restart controls");
        let records = crate::product_run::persistence::load_workbench_records(
            &state.path().join("workbench-v1"),
            Some(&controls),
        )
        .expect("durable records");
        assert!(records.contains_key(&run), "preview run must restore");
        let restarted =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        {
            let cancellation = peritus_journal::JournalCancellation::new();
            let _permit = restarted.inner.controls.acquire(&cancellation).expect("queue owner");
            *restarted.inner.controls.owner.lock().expect("controls") = Some(controls);
            *restarted.inner.records.write().expect("records") = records;
        }
        let restored = observe(&restarted, result_query);
        assert_eq!(restored.outputs(), terminal.outputs());
        assert_eq!(restored.result().launches()[0].state(), WorkbenchLaunchState::Exited);
        assert_eq!(restarted.workbench_receipt(actor(), &launch), result);
        restarted.shutdown().await.expect("shutdown product runs");
    });
}

fn observe(service: &ProductRunService, query: WorkbenchResultQuery) -> WorkbenchPreviewSnapshot {
    match service.workbench_preview(actor(), query) {
        AppResponsePayload::WorkbenchPreview(value) => value,
        other => panic!("preview output: {other:?}"),
    }
}
async fn wait_for_output(
    service: &ProductRunService,
    query: WorkbenchResultQuery,
    expected: &str,
    terminal: bool,
) -> WorkbenchPreviewSnapshot {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let observed = observe(service, query);
            if observed.outputs().iter().any(|output| {
                output.stdout().contains(expected) || output.stderr().contains(expected)
            }) && (!terminal
                || observed.result().launches()[0].state() == WorkbenchLaunchState::Exited)
            {
                return observed;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("expected output before deadline")
}
