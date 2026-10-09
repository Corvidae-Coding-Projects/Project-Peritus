//! Live prompts, input, authorization, and restart-retained output through the public projection.
use super::*;
use peritus_app_protocol::{
    WorkbenchInputText, WorkbenchLaunchState, WorkbenchPreviewOutputQuery,
    WorkbenchPreviewOutputStream, WorkbenchPreviewSnapshot, WorkbenchResultQuery,
};
mod evidence;
mod terminal;

#[test]
#[ignore = "owned subprocess fixture"]
fn preview_prompt_fixture() {
    use std::io::{BufRead as _, Write as _};
    println!("EARLY_SIGNAL");
    println!("{}", "x".repeat(200 * 1024));
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
            Some(2000),
            Some(20000),
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
        assert!(!live.outputs()[0].stdout().contains("EARLY_SIGNAL"));
        assert_eq!(live.result().launches()[0].state(), WorkbenchLaunchState::Running);
        let revision = live.result().result_revision();
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(
            observe(&running, result_query).result().result_revision(),
            revision,
            "unchanged polling must not mutate result state"
        );
        let live_check = command(
            workspace,
            0x7a,
            snapshot.revision(),
            WorkbenchIntent::CheckPreviewBehavior {
                launch: launch.operation(),
                observed: text("EARLY_SIGNAL"),
                note: WorkbenchInputText::new("NAME?".to_owned()).expect("note"),
            },
        );
        let live_receipt = running.workbench_command(actor(), &live_check).await;
        assert!(matches!(live_receipt, AppResponsePayload::WorkbenchReceipt(_)));
        let observed_start = evidence::assert_retained(&running, run, &live_check, false);
        let terminal_attachment = terminal::attach_and_reconnect(&running, &live).await;
        terminal_attachment.check_permission_changes(&running, workspace).await;
        #[cfg(windows)]
        let input_bytes = b"da\r".as_slice();
        #[cfg(not(windows))]
        let input_bytes = b"da\n".as_slice();
        let input = command(
            workspace,
            0x77,
            snapshot.revision(),
            WorkbenchIntent::InteractPreview {
                launch: launch.operation(),
                input: peritus_app_protocol::WorkbenchPreviewInput::new(input_bytes.to_vec())
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
        running.shutdown(Duration::from_secs(5)).await;
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
        *restarted.inner.controls.lock().expect("controls") = Some(controls);
        *restarted.inner.records.write().expect("records") = records;
        assert_eq!(restarted.workbench_command(actor(), &live_check).await, live_receipt);
        assert_eq!(evidence::assert_retained(&restarted, run, &live_check, false), observed_start);
        let restored = observe(&restarted, result_query);
        assert_eq!(restored.outputs(), terminal.outputs());
        assert_eq!(restored.result().launches()[0].state(), WorkbenchLaunchState::Exited);
        assert!(!restored.outputs()[0].stdout().contains("EARLY_SIGNAL"));
        let output_stream = WorkbenchPreviewOutputStream::Terminal;
        let range_query = WorkbenchPreviewOutputQuery::new(
            query(workspace),
            run,
            launch.operation(),
            output_stream,
            observed_start,
            12,
        )
        .expect("full output range query");
        let AppResponsePayload::WorkbenchPreviewOutput(range) =
            restarted.workbench_preview_output_range(actor(), range_query)
        else {
            panic!("full output range")
        };
        assert!(range.total_bytes() > 128 * 1024);
        assert!(range.artifact_digest().is_some());
        assert_eq!(range.offset(), observed_start);
        assert_eq!(range.bytes(), b"EARLY_SIGNAL");
        let check = command(
            workspace,
            0x79,
            snapshot.revision(),
            WorkbenchIntent::CheckPreviewBehavior {
                launch: launch.operation(),
                observed: WorkbenchLaunchText::new("EARLY_SIGNAL".to_owned()).expect("behavior"),
                note: WorkbenchInputText::new("full output verified after restart".to_owned())
                    .expect("note"),
            },
        );
        assert!(matches!(
            restarted.workbench_command(actor(), &check).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        evidence::assert_retained(&restarted, run, &check, true);
        evidence::reject_corrupt_and_preserve_legacy(&restarted, run, &check);
        assert_eq!(restarted.workbench_receipt(actor(), &launch), result);
        restarted.shutdown(Duration::from_secs(5)).await;
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
    .unwrap_or_else(|_| {
        panic!(
            "expected {expected:?} (terminal={terminal}) before deadline; latest: {:?}",
            observe(service, query)
        )
    })
}
