//! Direct-folder behavior through the actual daemon-owned conversation service.

mod pipeline;

use super::interaction::block_on;
use super::support::{named_tool_response, text_response};
use super::*;
use peritus_app_protocol::{
    ProductInteractionMode as Mode, ProductInteractionQuery, ProductRoleModels,
};

fn pipeline_prefix() -> Vec<std::collections::VecDeque<peritus_model_protocol::EventEnvelope>> {
    vec![
        named_tool_response("run_pipeline", b"{}".to_vec()),
        named_tool_response("workspace_list", br#"{"path":"","depth":1}"#.to_vec()),
        named_tool_response("workspace_read", br#"{"path":"note.txt"}"#.to_vec()),
        text_response(br"# In-place requested file operation

## Objective and acceptance criteria
Perform only the user's requested operation in this original directory. Preserve unrelated files and private daemon state. Inspect the source and resulting contents independently before accepting completion.

## Workspace findings
The source is note.txt in an ordinary directory, not a managed Git candidate. No Git setup, whole-directory snapshot, or rollback operation is appropriate.

## Implementation
Read note.txt, declare any additional command output before running the command, and change only the requested output. Keep all effects within the literal request.

## Verification and review
Use the existing exact-target checks and independent reviewer. Compare actual file contents to the user's requested text or copy operation. Report missing checks as unverified, never complete.

## Risks and non-goals
Do not touch private-peritus-state or unrelated.txt. Do not create a parallel workflow or assume an unsuccessful command worked.
"),
        named_tool_response("workspace_list", br#"{"path":"","depth":1}"#.to_vec()),
        named_tool_response("workspace_read", br#"{"path":"note.txt"}"#.to_vec()),
    ]
}

fn pipeline_review(
    path: &str,
) -> Vec<std::collections::VecDeque<peritus_model_protocol::EventEnvelope>> {
    vec![
        named_tool_response("workspace_list", br#"{"path":"","depth":1}"#.to_vec()),
        named_tool_response("workspace_read", serde_json::to_vec(&serde_json::json!({"path":path})).expect("arguments")),
        text_response(br#"{"findings":[],"summary":"The actual requested file contents match the request; exact-target checks passed."}"#),
    ]
}

fn artifact_contract(root: &std::path::Path) {
    fs::write(root.join("peritus-workspace.toml"), "schema_version = 1\nkind = \"artifact\"\n")
        .expect("pre-existing artifact verification contract");
}

async fn wait_for_owned_run_task(service: &ProductRunService, run_id: RunId) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let finished = {
                let tasks = service.inner.tasks.lock().await;
                tasks
                    .iter()
                    .find(|(owner, _)| *owner == run_id)
                    .expect("the daemon retains the exact run task")
                    .1
                    .is_finished()
            };
            if finished {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("daemon-owned task completion");
}

fn effect_receipt_frames(bytes: &[u8]) -> Vec<(usize, serde_json::Value)> {
    let mut frames = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let header_end = offset.checked_add(8).expect("receipt header end");
        let mut length_bytes = [0_u8; 8];
        length_bytes
            .copy_from_slice(bytes.get(offset..header_end).expect("complete receipt header"));
        let length =
            usize::try_from(u64::from_le_bytes(length_bytes)).expect("receipt frame length");
        let frame_end = header_end.checked_add(length).expect("receipt frame end");
        let payload = bytes.get(header_end..frame_end).expect("complete receipt frame");
        frames.push((frame_end, serde_json::from_slice(payload).expect("receipt JSON")));
        offset = frame_end;
    }
    frames
}

fn recover_lost_native_command_observer(
    service: &ProductRunService,
    writer: &ScriptedProvider,
    root: &std::path::Path,
    run_id: RunId,
) {
    let requests_before_recovery = writer.requests.lock().expect("provider requests").len();
    let run_hex = run_id.as_bytes().iter().fold(String::new(), |mut value, byte| {
        use std::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
        value
    });
    let effects = service.inner.directory.join(format!("{run_hex}.effects.bin"));
    let original_receipt_bytes = fs::read(&effects).expect("completed command ledger");
    let original_frames = effect_receipt_frames(&original_receipt_bytes);
    assert_eq!(original_frames.len(), 4, "one command has admission and terminal frames");
    assert_eq!(original_frames[0].1["state"], "started");
    assert!(original_frames[0].1["native_owner"].is_null());
    assert_eq!(original_frames[1].1["state"], "started");
    assert!(original_frames[1].1["native_owner"].is_object());
    let retained_bytes = u64::try_from(original_frames[1].0).expect("retained ledger length");
    let ledger = fs::OpenOptions::new().write(true).open(&effects).expect("open command ledger");
    ledger.set_len(retained_bytes).expect("simulate lost terminal observer");
    ledger.sync_data().expect("persist truncated command ledger");
    drop(ledger);
    let pending = peritus_product_runner::uncertain_effects(&effects)
        .expect("inspect truncated command receipt");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].state(), peritus_product_runner::UncertainEffectState::Started);
    assert!(!pending[0].owner_inactive());

    {
        let mut records = service.inner.records.write().expect("records");
        let record = records.get_mut(&run_id).expect("completed command run");
        let completed = record.snapshot.clone();
        record.snapshot = crate::product_run::snapshot::replace_snapshot(
            &completed,
            ProductRunPhase::RecoveryRequired,
            "The terminal native command observer was lost",
            completed.summary(),
        )
        .expect("recovery snapshot");
        "The run was interrupted after its native command completed"
            .clone_into(&mut record.interruption_cause);
        crate::product_run::persistence::persist_record(&service.inner.directory, record)
            .expect("persist recovery state");
    }

    let observations = service
        .query_observations(ProductRunQuery::exact(run_id))
        .expect("reconcile exact native command before retry");
    assert!(
        !service
            .inner
            .command_recoveries
            .lock()
            .expect("command recovery markers")
            .contains(&run_id),
        "the exact run recovery marker is released after projection"
    );
    let recovered = observations.first().expect("exact run observation").snapshot();
    let operation = recovered.operation();
    assert_eq!(recovered.phase(), ProductRunPhase::RecoveryRequired);
    assert_eq!(operation.kind(), peritus_app_protocol::ProductRunOperationKind::Execution);
    assert_eq!(operation.state(), peritus_app_protocol::ProductRunOperationState::RecoveryRequired);
    assert!(operation.legal_controls().retry());
    service
        .ensure_control_legal(run_id, ProductRunControlAction::Retry)
        .expect("daemon control path permits exact retry");
    assert!(
        peritus_product_runner::uncertain_effects(&effects)
            .expect("recovered command receipt")
            .is_empty(),
        "the real terminal owner result completes the receipt"
    );
    let recovered_bytes = fs::read(&effects).expect("recovered command ledger");
    let recovered_frames = effect_receipt_frames(&recovered_bytes);
    assert_eq!(recovered_frames.len(), 4, "recovery appends only Applied and Completed");
    let applied = &recovered_frames[recovered_frames.len() - 2].1;
    assert_eq!(applied["state"], "applied");
    assert!(applied["output"]["success"].as_bool().expect("terminal success"));
    assert_eq!(applied["output"]["exit_code"], 0);
    assert!(applied["owner_inactive"].as_bool().expect("inactive command owner"));
    assert_eq!(recovered_frames.last().expect("completed recovery frame").1["state"], "completed");
    assert_eq!(writer.requests.lock().expect("provider requests").len(), requests_before_recovery);
    assert_eq!(
        fs::read_to_string(root.join("command-result.txt")).expect("single command effect"),
        "requested"
    );
}

pub(super) fn folder_service(
    root: &std::path::Path,
    writer: &Arc<ScriptedProvider>,
    writable: bool,
) -> (ProductRunService, ProductRunRequest) {
    let root = root.canonicalize().expect("canonical directory");
    let state = root.join("private-peritus-state");
    fs::create_dir_all(&state).expect("private state");
    fs::write(state.join("private.txt"), "unchanged private data").expect("private file");
    let id = WorkspaceId::new([0x64; 16]).expect("workspace");
    // Production keeps its C2 catalog rooted at the separate managed-workspace namespace;
    // direct folders are additional product capabilities, not fake managed registrations.
    let managed = state.join("managed-workspaces");
    fs::create_dir_all(&managed).expect("managed namespace");
    let mut service = service(&state, &managed, id, [writer, writer, writer]);
    Arc::get_mut(&mut service.inner).expect("unique fixture").workspaces.insert(id, root.clone());
    let identity = peritus_workspace::FolderIdentity::observe(&root).expect("directory identity");
    let digest = identity.digest().as_bytes().iter().fold(String::new(), |mut value, byte| {
        use std::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
        value
    });
    let folder = toml::from_str(&format!("workspace_id = {:?}\nroot = {:?}\nidentity = {:?}\nwritable = {writable}\nprotected_paths = [{:?}]\n", "64".repeat(16), root.to_str().expect("root"), digest, state.to_str().expect("state"))).expect("folder declaration");
    Arc::get_mut(&mut service.inner).expect("unique fixture").folders.insert(id, folder);
    let request = ProductRunRequest::new(
        RunId::new([0x65; 16]).expect("run"),
        id,
        ProductProviderSelection::new(
            writer.profile.profile_id(),
            writer.profile.profile_id(),
            writer.profile.profile_id(),
        ),
        "Change only note.txt to requested text; do not touch other files.".to_owned(),
    )
    .expect("request");
    (service, request)
}

#[test]
fn greeting_in_a_non_git_home_shaped_folder_needs_no_baseline_or_scan() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        let writer =
            scripted(0x61, "folder-chat", vec![text_response(b"Hello from an ordinary folder.")]);
        let (service, request) = folder_service(root.path(), &writer, false);
        let id = request.run_id();
        service
            .start_interaction(request, Mode::Chat, ProductRoleModels::default())
            .await
            .expect("start");
        let result = wait_for_terminal(&service, id).await;
        assert_eq!(result.phase(), ProductRunPhase::WaitingForUser, "{}", result.summary());
        assert!(result.deliverable().is_none());
        assert!(!root.path().join(".git").exists());
        assert!(!root.path().join(".design").exists());
        let snapshot =
            service.query_interaction(ProductInteractionQuery::new(id)).expect("snapshot");
        assert_eq!(snapshot.incorporated(), 1);
        let records = service.load_test_records().expect("restore");
        assert_eq!(records.get(&id).expect("restored conversation").interaction.incorporated, 1);
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn requested_edits_land_in_the_original_folder_and_cannot_overwrite_private_state() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        fs::write(root.path().join("note.txt"), "original").expect("original file");
        fs::write(root.path().join("unrelated.txt"), "leave alone").expect("unrelated file");
        artifact_contract(root.path());
        let writer = scripted(
            0x62,
            "folder-edit",
            pipeline_prefix().into_iter().chain([
                named_tool_response(
                    "workspace_write",
                    br#"{"path":"note.txt","content":"requested text"}"#.to_vec(),
                ),
                named_tool_response(
                    "workspace_write",
                    br#"{"path":"private-peritus-state/private.txt","content":"must not write"}"#
                        .to_vec(),
                ),
                text_response(br#"{"kind":"complete","run_instructions":"cat note.txt","summary":"Requested edit completed in place."}"#),
            ]).chain(pipeline_review("note.txt")).collect(),
        );
        let (service, request) = folder_service(root.path(), &writer, true);
        let id = request.run_id();
        service
            .start_interaction(request, Mode::Chat, ProductRoleModels::default())
            .await
            .expect("start");
        let result = wait_for_terminal(&service, id).await;
        assert_eq!(result.phase(), ProductRunPhase::Complete, "{}", result.summary());
        assert!(result.gates().contains("Exact-target acceptance: PASS"));
        assert!(!result.review().is_empty());
        assert!(
            !service
                .inner
                .records
                .read()
                .expect("records")
                .get(&id)
                .expect("record")
                .candidate_actionable
        );
        assert_eq!(
            fs::read_to_string(root.path().join("note.txt")).expect("edited file"),
            "requested text"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("unrelated.txt")).expect("retained file"),
            "leave alone"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("private-peritus-state/private.txt"))
                .expect("private state"),
            "unchanged private data"
        );
        assert!(!root.path().join(".git").exists());
        assert!(result.deliverable().is_none());
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn folder_trust_and_read_only_modes_are_enforced_in_the_daemon() {
    block_on(async {
        for (mode, writable) in [(Mode::Chat, false), (Mode::Plan, true), (Mode::Review, true)] {
            let root = tempfile::tempdir().expect("folder");
            fs::write(root.path().join("note.txt"), "original").expect("original file");
            let writer = scripted(
                0x63,
                "restricted-folder",
                vec![
                    named_tool_response("run_pipeline", b"{}".to_vec()),
                    named_tool_response(
                        "workspace_write",
                        br#"{"path":"note.txt","content":"forbidden"}"#.to_vec(),
                    ),
                    text_response(b"Read-only response."),
                ],
            );
            let (service, request) = folder_service(root.path(), &writer, writable);
            let id = request.run_id();
            service
                .start_interaction(request, mode, ProductRoleModels::default())
                .await
                .expect("start");
            let result = wait_for_terminal(&service, id).await;
            assert_eq!(result.phase(), ProductRunPhase::WaitingForUser, "{}", result.summary());
            assert_eq!(
                fs::read_to_string(root.path().join("note.txt")).expect("unchanged"),
                "original"
            );
            service.shutdown(Duration::from_secs(5)).await;
        }
    });
}

#[test]
fn git_candidate_delivery_is_rejected_before_admitting_a_folder_run() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        let writer = scripted(0x66, "folder", Vec::new());
        let (service, request) = folder_service(root.path(), &writer, true);
        let error = service
            .start_interaction(request, Mode::Build, ProductRoleModels::default())
            .await
            .expect_err("Git delivery requires its real capabilities");
        assert_eq!(error, super::super::ProductRunServiceError::GitRequired);
        assert!(service.inner.records.read().expect("records").is_empty());
        assert!(!root.path().join(".git").exists());
    });
}

#[cfg(unix)]
#[test]
fn requested_command_runs_in_the_original_folder_with_daemon_owned_processes() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        fs::write(root.path().join("note.txt"), "requested").expect("source file");
        artifact_contract(root.path());
        let writer = scripted(0x67, "folder-command", pipeline_prefix().into_iter().chain([
            named_tool_response("workspace_scope", br#"{"paths":["command-result.txt"]}"#.to_vec()),
            named_tool_response("run_command", br#"{"program":"/bin/cp","args":["note.txt","command-result.txt"],"cwd":".","purpose":"external_effect","timeout_seconds":10}"#.to_vec()),
            text_response(br#"{"kind":"complete","run_instructions":"cat command-result.txt","summary":"Requested command finished in the original folder."}"#),
        ]).chain(pipeline_review("command-result.txt")).collect());
        let (service, request) = folder_service(root.path(), &writer, true);
        let id = request.run_id();
        let request = ProductRunRequest::new(
            id,
            request.workspace_id(),
            request.providers(),
            "Run cp to copy note.txt to command-result.txt in this folder.".to_owned(),
        )
        .expect("command request");
        service
            .start_interaction(request, Mode::Chat, ProductRoleModels::default())
            .await
            .expect("start");
        let result = wait_for_terminal(&service, id).await;
        wait_for_owned_run_task(&service, id).await;
        assert_eq!(result.phase(), ProductRunPhase::Complete, "{}", result.summary());
        let snapshot =
            service.query_interaction(ProductInteractionQuery::new(id)).expect("snapshot");
        assert!(root.path().join("command-result.txt").exists(), "{snapshot:?}");
        let command = snapshot
            .activities()
            .iter()
            .find(|activity| {
                activity.kind() == peritus_app_protocol::ProductActivityKind::Tool
                    && activity.text().starts_with("Ran /bin/cp note.txt command-result.txt")
            })
            .expect("actual command is visible in the conversation");
        assert!(command.detail().contains("Exit code: 0"));
        assert!(
            !snapshot
                .activities()
                .iter()
                .any(|activity| activity.text().starts_with("Calling /bin/cp")),
            "completion updates the original entry"
        );
        // Persistence retains exact model envelopes; the public snapshot presents their prose.
        let durable_activities =
            service.inner.records.read().unwrap().get(&id).unwrap().interaction.activities.clone();
        let restored = service.load_test_records().expect("durable activities");
        assert_eq!(restored.get(&id).unwrap().interaction.activities, durable_activities);
        assert_eq!(
            fs::read_to_string(root.path().join("command-result.txt")).expect("command effect"),
            "requested"
        );

        recover_lost_native_command_observer(&service, &writer, root.path(), id);
        assert!(!root.path().join(".git").exists());
        service.shutdown(Duration::from_secs(5)).await;
    });
}
