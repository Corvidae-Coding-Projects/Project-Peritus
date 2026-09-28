//! Delayed checkpoint inspections cannot consume subsequent user input.
use super::*;

#[test]
fn checkpoint_inspection_responses_preserve_new_drafts_even_after_escape() {
    for command in ["checkpoint show", "rewind"] {
        for cancel in [false, true] {
            let mut model = checkpoint_model();
            let snapshot = selected(&mut model);
            key(&mut model, KeyCode::Esc);
            let checkpoint = ControlOperationId::new([0x42; 16]).unwrap();
            model.chat.buffer =
                format!("/{command} {}", crate::model::format_id(checkpoint.as_bytes()));
            let sent = request(&enter_with_metadata(&mut model));
            let response = match sent.payload() {
                AppRequestPayload::InspectWorkbenchCheckpoint(_) => {
                    AppResponsePayload::WorkbenchCheckpoint(
                        WorkbenchCheckpointReceipt::new(
                            checkpoint,
                            snapshot.query(),
                            snapshot.revision(),
                            WorkbenchCheckpointName::new("Saved state".to_owned()).unwrap(),
                            WorkbenchCheckpointReferences::new(1, 1, 0, None),
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                        )
                        .unwrap(),
                    )
                }
                AppRequestPayload::PreviewWorkbenchRewind(selection) => {
                    AppResponsePayload::WorkbenchRewindPreview(
                        WorkbenchRewindPreview::new(*selection, Vec::new(), Vec::new(), Vec::new())
                            .unwrap(),
                    )
                }
                _ => panic!("read-only checkpoint inspection"),
            };
            if cancel {
                key(&mut model, KeyCode::Esc);
            }
            model.chat.buffer = "Actually, keep the current version.".to_owned();
            assert!(respond(&mut model, &sent, response).is_empty());
            assert_eq!(model.chat.buffer, "Actually, keep the current version.");
            assert!(model.chat.workbench.unresolved.is_none());
        }
    }
}
