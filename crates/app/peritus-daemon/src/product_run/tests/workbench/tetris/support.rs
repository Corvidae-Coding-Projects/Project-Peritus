//! Governed upload, scripted provider, launch, and retained-evidence support for the V1 scenario.

use super::*;
use crate::product_run::tests::support as run_support;
use peritus_app_protocol::{
    ArtifactChunk, ArtifactCompletion, ArtifactMetadata, CanonicalMediaType, ProductModelChoice,
    TransferId, WorkbenchBuildIdentity, WorkbenchGoalCriterionState, WorkbenchImageLabel,
    WorkbenchImageRequest, WorkbenchImageUpload,
};
use peritus_model_protocol::{ContentBlock, EventEnvelope};
use peritus_types::ArtifactId;
use std::collections::VecDeque;

const GIF: &[u8] = &[
    71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 44, 0, 0, 0, 0, 1, 0, 1,
    0, 0, 2, 2, 68, 1, 0, 59,
];
pub(super) const OBSERVED: &str = "OBSERVED DROP x=3 y=14 rotation=1";
pub(super) const TETRIS_SOURCE: &str = r##"import os
import sys
import threading
import tkinter as tk

os.environ["DISPLAY"] = sys.argv[1]
root = tk.Tk()
root.title("Peritus Tetris V1")
root.resizable(False, False)
canvas = tk.Canvas(root, width=300, height=440, bg="#111827", highlightthickness=0)
canvas.pack()
status = tk.StringVar(value="READY — arrows supplied through governed stdin")
tk.Label(root, textvariable=status, bg="#0f172a", fg="#e2e8f0", pady=8).pack(fill="x")
state = {"x": 3, "y": 0, "rotation": 0}
shapes = (((0, 0), (1, 0), (0, 1), (1, 1)), ((0, 0), (0, 1), (0, 2), (1, 2)))

def draw():
    canvas.delete("all")
    for row in range(20):
        for column in range(10):
            x0, y0 = column * 22 + 40, row * 20 + 20
            canvas.create_rectangle(x0, y0, x0 + 22, y0 + 20, outline="#243047")
    for dx, dy in shapes[state["rotation"]]:
        x0 = (state["x"] + dx) * 22 + 40
        y0 = (state["y"] + dy) * 20 + 20
        canvas.create_rectangle(x0, y0, x0 + 22, y0 + 20, fill="#22d3ee", outline="#cffafe")

def apply(command):
    if command == "LEFT":
        state["x"] -= 1
    elif command == "RIGHT":
        state["x"] += 1
    elif command == "ROTATE":
        state["rotation"] = 1 - state["rotation"]
    elif command == "DROP":
        state["y"] = 14
    draw()
    status.set(f'{command}  x={state["x"]} y={state["y"]} rotation={state["rotation"]}')
    print(f'OBSERVED {command} x={state["x"]} y={state["y"]} rotation={state["rotation"]}', flush=True)

def read_commands():
    for line in sys.stdin:
        command = line.strip()
        if command in {"LEFT", "RIGHT", "ROTATE", "DROP"}:
            root.after(0, apply, command)

draw()
print("READY TETRIS x=3 y=0 rotation=0", flush=True)
threading.Thread(target=read_commands, daemon=True).start()
root.mainloop()
"##;

pub(super) fn writer_script() -> Vec<VecDeque<EventEnvelope>> {
    vec![
        tool("workspace_list", r#"{"path":"","depth":3}"#),
        tool("workspace_read", r#"{"path":"Cargo.toml","start_line":1,"end_line":500}"#),
        tool("workspace_read", r#"{"path":"src/lib.rs","start_line":1,"end_line":500}"#),
        run_support::text_response(b"# Native Tetris design\n\n## Objective and acceptance criteria\nCreate an actual playable Tk preview driven by LEFT, RIGHT, ROTATE, and DROP.\n\n## Repository findings\nThe small fixture permits one standalone Python entrypoint.\n\n## Architecture and interfaces\nUse Tk Canvas plus a bounded stdin command adapter.\n\n## Data and control flow\nCommands update explicit piece state on Tk's event loop.\n\n## File and module plan\nCreate only tetris.py.\n\n## Implementation slices\nRender, accept commands, report observations.\n\n## Verification\nCompile and exercise all four controls.\n\n## Risks and non-goals\nThis proves the V1 interaction slice, not a complete game engine.\n"),
        tool("workspace_list", r#"{"path":"","depth":3}"#),
        tool("workspace_read", r#"{"path":"src/lib.rs","start_line":1,"end_line":500}"#),
        write_tool(),
        run_support::text_response(br#"{"kind":"complete","run_instructions":"python3 tetris.py","summary":"Created the playable controlled Tk Tetris preview."}"#),
    ]
}

pub(super) fn reviewer_script() -> Vec<VecDeque<EventEnvelope>> {
    vec![
        tool("workspace_list", r#"{"path":"","depth":3}"#),
        tool("workspace_read", r#"{"path":"tetris.py","start_line":1,"end_line":500}"#),
        run_support::text_response(br#"{"findings":[],"summary":"The playable native preview and exact controls are present."}"#),
    ]
}

fn tool(name: &str, arguments: &str) -> VecDeque<EventEnvelope> {
    let value: serde_json::Value = serde_json::from_str(arguments).expect("tool arguments");
    run_support::named_tool_response(name, serde_json::to_vec(&value).expect("arguments"))
}

fn write_tool() -> VecDeque<EventEnvelope> {
    let mut value: serde_json::Value =
        serde_json::from_str(r#"{"path":"tetris.py","content":""}"#).expect("write arguments");
    value["content"] = serde_json::Value::String(TETRIS_SOURCE.to_owned());
    run_support::named_tool_response(
        "workspace_write",
        serde_json::to_vec(&value).expect("write arguments"),
    )
}

pub(super) fn criterion(
    kind: WorkbenchGoalCriterionKind,
    label: &str,
) -> WorkbenchGoalCriterionDefinition {
    WorkbenchGoalCriterionDefinition::new(kind, input_text(label), true)
}

pub(super) fn input_text(value: &str) -> WorkbenchInputText {
    WorkbenchInputText::new(value.to_owned()).expect("input text")
}

pub(super) fn text(value: &str) -> WorkbenchLaunchText {
    WorkbenchLaunchText::new(value.to_owned()).expect("launch text")
}

fn assert_receipt(response: AppResponsePayload) {
    assert!(matches!(response, AppResponsePayload::WorkbenchReceipt(_)), "{response:?}");
}

pub(super) fn operation(id: u8) -> ControlOperationId {
    ControlOperationId::new([id; 16]).expect("operation")
}

pub(super) async fn accept(
    service: &ProductRunService,
    workspace: WorkspaceId,
    id: u8,
    revision: u64,
    intent: WorkbenchIntent,
) {
    assert_receipt(
        service.workbench_command(actor(), &command(workspace, id, revision, intent)).await,
    );
}

#[allow(clippy::too_many_arguments, reason = "explicit authenticated preview bindings")]
pub(super) async fn accept_preview(
    service: &ProductRunService,
    authority: &crate::AuthorityHandle,
    session: SessionId,
    workspace: WorkspaceId,
    id: u8,
    revision: u64,
    intent: WorkbenchIntent,
) {
    let command = command(workspace, id, revision, intent);
    assert_receipt(preview::preview_command(service, authority, session, &command).await);
}

pub(super) async fn attach_image(
    service: &ProductRunService,
    authority: &crate::AuthorityHandle,
    session: SessionId,
    workspace: WorkspaceId,
    writer: &Arc<ScriptedProvider>,
) {
    let transfer = TransferId::new([0xd7; 16]).expect("transfer");
    let artifact = ArtifactId::new([0xd8; 16]).expect("artifact");
    let digest = peritus_codec::sha256(GIF);
    let metadata = ArtifactMetadata::new(
        transfer,
        artifact,
        GIF.len() as u64,
        CanonicalMediaType::new("image/gif".to_owned(), 128).expect("media type"),
        digest,
        1024,
        1024,
    )
    .expect("metadata");
    service
        .begin_workbench_image_upload(
            authority,
            actor(),
            session,
            &WorkbenchImageUpload::new(query(workspace), 3, metadata.clone()).expect("upload"),
            1024,
        )
        .await
        .expect("begin governed upload");
    authority
        .upload_artifact_chunk(
            actor(),
            session,
            ArtifactChunk::new(transfer, artifact, 0, 0, GIF.to_vec(), 1024).expect("chunk"),
        )
        .await
        .expect("upload exact bytes");
    authority
        .complete_artifact_upload(
            actor(),
            session,
            ArtifactCompletion::new(transfer, artifact, GIF.len() as u64, digest),
        )
        .await
        .expect("complete upload");
    let request = WorkbenchImageRequest::new(
        query(workspace),
        3,
        artifact,
        writer.profile.profile_id(),
        ProductModelChoice::default(),
        WorkbenchImageLabel::new("tetris-reference.gif".to_owned()).expect("label"),
    )
    .expect("image selection");
    let AppResponsePayload::WorkbenchImagePreview(preview) =
        service.preview_workbench_image(authority, actor(), &request).await
    else {
        panic!("image preview failed")
    };
    let confirm = command(
        workspace,
        0xd9,
        3,
        WorkbenchIntent::AttachImage {
            preview,
            text: input_text("Use this exact confirmed reference in the Tetris build."),
        },
    );
    let response = service.confirm_workbench_image(authority, actor(), &confirm).await;
    assert!(
        matches!(response, AppResponsePayload::WorkbenchReceipt(ref receipt) if receipt.accepted_revision() == 4),
        "{response:?}"
    );
}

pub(super) fn assert_image_reached_writer(writer: &ScriptedProvider) {
    let requests = writer.requests.lock().expect("writer requests");
    assert_eq!(requests.len(), 8, "complete design and writer scripts ran");
    let images = requests
        .iter()
        .flat_map(peritus_model_protocol::ModelRequest::messages)
        .flat_map(peritus_model_protocol::Message::content)
        .filter_map(|block| match block {
            ContentBlock::Image(image) => Some(image),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!images.is_empty(), "confirmed image reached the capable writer");
    assert!(images.iter().all(|image| {
        image.inline_bytes_for_wire() == Some(GIF)
            && image.digest() == Some(peritus_codec::sha256(GIF))
    }));
}

pub(super) fn launch_profile(
    run: RunId,
    repository: &std::path::Path,
    display: &str,
) -> WorkbenchLaunchProfile {
    let source = WorkbenchLaunchSource::new(
        WorkbenchLaunchSourceKind::ManagedCandidate,
        text("."),
        peritus_product_runner::ProductRunner::candidate_digest(repository)
            .expect("candidate digest"),
    );
    let build = WorkbenchBuildIdentity::new(
        text("tetris.py"),
        peritus_codec::sha256(&fs::read(repository.join("tetris.py")).expect("build bytes")),
    );
    WorkbenchLaunchProfile::new(
        run,
        text("python3"),
        vec![text("tetris.py"), text(display)],
        text("."),
        Vec::new(),
        source,
        Some(build),
        2_000,
        20_000,
        true,
    )
    .expect("launch profile")
}

pub(super) fn assert_goal(
    service: &ProductRunService,
    workspace: WorkspaceId,
    state: WorkbenchGoalState,
    graphical: bool,
) {
    let goal = preview::goal_snapshot(service, workspace);
    assert_eq!(goal.state(), state);
    preview::assert_criterion(
        &goal,
        WorkbenchGoalCriterionKind::RunnerAcceptance,
        WorkbenchGoalCriterionState::Satisfied,
    );
    preview::assert_criterion(
        &goal,
        WorkbenchGoalCriterionKind::GraphicalPlaytest,
        if graphical {
            WorkbenchGoalCriterionState::Satisfied
        } else {
            WorkbenchGoalCriterionState::Unavailable
        },
    );
}

pub(super) async fn preserve_and_assert_result(
    service: &ProductRunService,
    authority: &crate::AuthorityHandle,
    workspace: WorkspaceId,
    run: RunId,
    capture_id: ControlOperationId,
    window: u64,
) {
    let page = service
        .result_page(actor(), WorkbenchResultQuery::new(query(workspace), run))
        .expect("result page");
    let result = page.launches().first().expect("launch result");
    assert_eq!(result.state(), WorkbenchLaunchState::Stopped);
    assert!(result.process().is_some() && result.ready() && result.stdout_digest().is_some());
    assert_eq!(result.profile().source().kind(), WorkbenchLaunchSourceKind::ManagedCandidate);
    assert_eq!(result.profile().build().expect("build").path().as_str(), "tetris.py");
    assert_eq!(result.interactions().len(), 4);
    assert!(result.interactions().iter().all(|receipt| receipt.observed()));
    assert_eq!(result.behavior_checks(), 1);
    assert_eq!(result.captures().len(), 1);
    let capture = result.captures().first().expect("capture");
    assert_eq!(
        (capture.operation(), capture.state()),
        (capture_id, WorkbenchCaptureState::Captured)
    );
    assert_eq!(capture.target(), WorkbenchCaptureTarget::x11_window(window).expect("window"));
    assert_eq!(result.feedback().len(), 1);
    let feedback = result.feedback().first().expect("feedback");
    assert_eq!(feedback.capture(), capture_id);
    assert_eq!(feedback.feedback(), WorkbenchReviewFeedback::KeepBehavior);
    assert_eq!(
        feedback.message().as_str(),
        "Keep this controlled piece movement and visible board."
    );
    let evidence = result.evidence();
    assert!(evidence.built() && evidence.launched() && evidence.captured());
    assert!(evidence.behavior_checked() && evidence.human_reviewed());
    let scope = crate::artifact::ArtifactScope::new(actor(), query(workspace));
    let (catalog, bytes) = authority
        .read_scoped_artifact(
            scope,
            capture.artifact().expect("capture artifact"),
            16 * 1024 * 1024,
        )
        .await
        .expect("capture bytes");
    let digest = capture.image_digest().expect("image digest");
    assert_eq!(catalog.digest(), digest);
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    if let Some(directory) = std::env::var_os("PERITUS_V1_EVIDENCE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        fs::create_dir_all(&directory).expect("evidence directory");
        fs::write(directory.join("tetris-v1.png"), &bytes).expect("capture evidence");
        fs::write(directory.join("tetris-v1.py"), TETRIS_SOURCE).expect("source evidence");
        let metadata = format!(
            "run={:?}\ncapture={:?}\nwindow={window}\nstate=stopped\nready=true\ninteractions=4\nbehavior_checks=1\nfeedback=keep-behavior\nobserved={OBSERVED}\nimage_sha256={digest:?}\n",
            run.as_bytes(),
            capture_id.as_bytes(),
        );
        fs::write(directory.join("tetris-v1-evidence.txt"), metadata).expect("metadata evidence");
    }
}
