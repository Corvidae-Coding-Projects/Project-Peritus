//! Direct-folder behavior through the actual daemon-owned conversation service.

mod pipeline;

use super::interaction::block_on;
use super::support::{named_tool_response, text_response};
use super::*;
use peritus_app_protocol::{
    ProductInteractionMode as Mode, ProductInteractionRequest, ProductRoleModels,
    ProductRunConversationQuery,
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
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start");
        let result = wait_for_terminal(&service, id).await;
        assert_eq!(result.phase(), ProductRunPhase::WaitingForUser, "{}", result.summary());
        assert!(result.deliverable().is_none());
        assert!(!root.path().join(".git").exists());
        assert!(!root.path().join(".design").exists());
        let snapshot =
            service.query_interaction(ProductRunConversationQuery::new(id)).expect("snapshot");
        assert_eq!(snapshot.incorporated(), 1);
        let records =
            super::super::persistence::load_records(&service.inner.directory).expect("restore");
        assert_eq!(
            records
                .get(&id)
                .expect("restored conversation")
                .interaction
                .as_ref()
                .expect("interaction")
                .incorporated,
            1
        );
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
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Chat,
                ProductRoleModels::default(),
            ))
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
                .interact(ProductInteractionRequest::new(
                    request,
                    mode,
                    ProductRoleModels::default(),
                ))
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
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Build,
                ProductRoleModels::default(),
            ))
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
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start");
        let result = wait_for_terminal(&service, id).await;
        assert_eq!(result.phase(), ProductRunPhase::Complete, "{}", result.summary());
        let snapshot =
            service.query_interaction(ProductRunConversationQuery::new(id)).expect("snapshot");
        assert!(root.path().join("command-result.txt").exists(), "{snapshot:?}");
        assert_eq!(
            fs::read_to_string(root.path().join("command-result.txt")).expect("command effect"),
            "requested"
        );
        assert!(!root.path().join(".git").exists());
        service.shutdown(Duration::from_secs(5)).await;
    });
}
