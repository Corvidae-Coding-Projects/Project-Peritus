//! Scripted successful product pipeline responses shared by checkpoint scenarios.

use crate::product_run::tests::support;

pub(super) fn remove_injected_incomplete_transaction(state: &std::path::Path) {
    let namespaces = std::fs::read_dir(state.join("workbench-folder-transactions")).unwrap();
    let mut removed = 0;
    for namespace in namespaces {
        for entry in std::fs::read_dir(namespace.unwrap().path()).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name().to_str().unwrap().starts_with("txn-") {
                std::fs::remove_dir(entry.path()).expect("injected empty transaction only");
                removed += 1;
            }
        }
    }
    assert_eq!(removed, 1);
}

pub(super) fn assert_receipt_is_observational(
    service: &crate::product_run::ProductRunService,
    folder: &std::path::Path,
    command: &peritus_app_protocol::WorkbenchCommand,
) {
    let conversation = peritus_product_runner::control::ConversationId::new(
        command.query().conversation().into_bytes(),
    )
    .expect("conversation");
    let before = service.with_controls(false, |store| store.load(conversation)).unwrap().unwrap();
    let bytes = std::fs::read(folder.join("note.txt")).unwrap();
    assert!(matches!(
        service.workbench_receipt(super::actor(), command),
        peritus_app_protocol::AppResponsePayload::Error(_)
    ));
    let after = service.with_controls(false, |store| store.load(conversation)).unwrap().unwrap();
    assert_eq!(before, after, "receipt lookup must not settle or publish anything");
    assert_eq!(std::fs::read(folder.join("note.txt")).unwrap(), bytes);
}

pub(super) fn pipeline_responses()
-> Vec<std::collections::VecDeque<peritus_model_protocol::EventEnvelope>> {
    vec![
        support::named_tool_response("run_pipeline", b"{}".to_vec()),
        support::named_tool_response("workspace_list", br#"{"path":"","depth":1}"#.to_vec()),
        support::named_tool_response("workspace_read", br#"{"path":"note.txt"}"#.to_vec()),
        support::text_response(br"# Checkpoint edit design

## Objective and acceptance criteria
Change only note.txt to the exact requested content and preserve every unrelated file.

## Repository findings
This is an in-place artifact folder with one selected target and an existing verification contract.

## Architecture and interfaces
Use the existing bounded workspace write tool for note.txt only.

## Data and control flow
Read the current target, replace its complete bytes, then inspect the result.

## File and module plan
Modify note.txt and no other path.

## Implementation slices
Perform one exact whole-file replacement.

## Verification
Re-read note.txt and require the exact final bytes.

## Risks and non-goals
Do not modify unrelated.txt or private daemon state.
"),
        support::named_tool_response("workspace_list", br#"{"path":"","depth":1}"#.to_vec()),
        support::named_tool_response("workspace_read", br#"{"path":"note.txt"}"#.to_vec()),
        support::named_tool_response(
            "workspace_write",
            br#"{"path":"note.txt","content":"Peritus owned edit\n"}"#.to_vec(),
        ),
        support::text_response(
            br#"{"kind":"complete","run_instructions":"cat note.txt","summary":"Updated only note.txt."}"#,
        ),
        support::named_tool_response("workspace_list", br#"{"path":"","depth":1}"#.to_vec()),
        support::named_tool_response("workspace_read", br#"{"path":"note.txt"}"#.to_vec()),
        support::text_response(
            br#"{"findings":[],"summary":"The selected note has the exact requested bytes."}"#,
        ),
    ]
}
