use super::*;
use peritus_app_protocol::{
    ControlOperationId, WorkbenchCheckpointFileMode, WorkbenchCheckpointName,
    WorkbenchCheckpointPath, WorkbenchCheckpointReceipt, WorkbenchCheckpointReferences,
    WorkbenchCheckpointVersion, WorkbenchRestoreReceipt, WorkbenchRestoreStatus,
    WorkbenchRewindDisposition, WorkbenchRewindPath, WorkbenchRewindPreview,
};
use peritus_types::Sha256Digest;

mod drafts;

fn checkpoint_model() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchCheckpoints)
            .expect("feature"),
    );
    model
}

#[test]
fn rewind_modes_bind_a_new_logical_identity() {
    for mode in ["conversation", "combined"] {
        let mut model = checkpoint_model();
        let source = selected(&mut model);
        key(&mut model, KeyCode::Esc);
        model.chat.buffer = format!("/rewind {} {mode}", crate::model::format_id(&[91; 16]));
        let sent = request(&enter_with_metadata(&mut model));
        let AppRequestPayload::PreviewWorkbenchRewind(selection) = sent.payload() else {
            panic!("rewind selection")
        };
        assert_eq!(selection.query(), source.query());
        assert_ne!(selection.child().unwrap(), source.query().conversation());
        assert_eq!(
            selection.mode(),
            if mode == "combined" {
                peritus_app_protocol::WorkbenchRewindMode::Combined
            } else {
                peritus_app_protocol::WorkbenchRewindMode::ConversationOnly
            }
        );
    }
}

fn selected(model: &mut AppModel) -> WorkbenchSnapshot {
    let (sent, command) = create(model);
    let query_request = request(&respond(model, &sent, receipt(&command)));
    let snapshot = WorkbenchSnapshot::new(
        command.query(),
        1,
        ConversationTitle::new("Private fixture".to_owned()).expect("title"),
        false,
        false,
    )
    .expect("snapshot");
    respond(model, &query_request, AppResponsePayload::Workbench(snapshot.clone()));
    snapshot
}

fn version(byte: u8, bytes: u64) -> WorkbenchCheckpointVersion {
    WorkbenchCheckpointVersion::Present {
        digest: Sha256Digest::new([byte; 32]),
        bytes,
        mode: WorkbenchCheckpointFileMode::Regular,
    }
}

fn assert_preview_render(model: &AppModel) {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal = Terminal::new(TestBackend::new(120, 38)).expect("terminal");
    let frame = terminal.draw(|frame| crate::render::draw(frame, model)).expect("draw");
    let rendered: String =
        frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
    for expected in [
        "RESTORE",
        "note.txt",
        "Excluded",
        "Conversation history preserved: true",
        "Press c to confirm this exact preview",
    ] {
        assert!(rendered.contains(expected), "missing {expected}: {rendered}");
    }
}

fn preview_and_cancel(
    model: &mut AppModel,
    checkpoint: &WorkbenchCommand,
    receipt: &WorkbenchCheckpointReceipt,
    baseline: WorkbenchCheckpointVersion,
    owned: WorkbenchCheckpointVersion,
) -> WorkbenchRewindPreview {
    let checkpoint_id = crate::model::format_id(checkpoint.operation().as_bytes());
    key(model, KeyCode::Esc);
    model.chat.buffer = format!("/rewind {checkpoint_id}");
    let request = request(&enter_with_metadata(model));
    let AppRequestPayload::PreviewWorkbenchRewind(rewind) = request.payload() else {
        panic!("rewind preview request")
    };
    assert_eq!(rewind.revision(), 2);
    assert_eq!(rewind.checkpoint(), checkpoint.operation());
    let path = WorkbenchRewindPath::new(
        "note.txt".to_owned(),
        baseline,
        Some(owned),
        owned,
        WorkbenchRewindDisposition::Restore,
    )
    .expect("path");
    let preview = WorkbenchRewindPreview::new(
        *rewind,
        vec![path],
        receipt.exclusions().to_vec(),
        receipt.external_effects().to_vec(),
    )
    .expect("preview");
    assert!(
        respond(model, &request, AppResponsePayload::WorkbenchRewindPreview(preview.clone()),)
            .is_empty()
    );
    assert!(model.chat.workbench.restore_receipt.is_none());
    assert!(model.chat.buffer.is_empty(), "successful rewind inspection clears its command");
    assert_preview_render(model);
    assert!(key(model, KeyCode::Esc).is_empty());
    assert!(model.chat.workbench.rewind_preview.is_none());
    assert!(!model.chat.workbench.open);
    preview
}

#[test]
fn checkpoint_preview_cancel_and_exact_rewind_confirmation_are_native_and_correlated() {
    let mut model = checkpoint_model();
    let initial = selected(&mut model);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/checkpoint before owned edit".to_owned();
    let checkpoint_request = request(&enter_with_metadata(&mut model));
    let AppRequestPayload::WorkbenchCommand(checkpoint_command) = checkpoint_request.payload()
    else {
        panic!("checkpoint command")
    };
    assert!(matches!(
        checkpoint_command.intent(),
        peritus_app_protocol::WorkbenchIntent::CreateCheckpoint(name)
            if name.as_str() == "before owned edit"
    ));
    assert!(model.chat.workbench.checkpoint_receipt.is_none());

    let baseline = version(10, 20);
    let owned = version(11, 19);
    let checkpoint_receipt = WorkbenchCheckpointReceipt::new(
        checkpoint_command.operation(),
        checkpoint_command.query(),
        2,
        WorkbenchCheckpointName::new("before owned edit".to_owned()).expect("name"),
        WorkbenchCheckpointReferences::new(1, 3, 4, Some(5)),
        vec![WorkbenchCheckpointPath::new("note.txt".to_owned(), baseline, None).expect("path")],
        vec!["excerpt.txt: partial selection is not a whole-file restore target".to_owned()],
        vec![
            "Credentials, approvals, process state, and external side effects are excluded."
                .to_owned(),
        ],
    )
    .expect("checkpoint receipt");
    let refresh = request(&respond(
        &mut model,
        &checkpoint_request,
        AppResponsePayload::WorkbenchCheckpoint(checkpoint_receipt.clone()),
    ));
    assert_eq!(model.chat.workbench.checkpoint_receipt.as_ref(), Some(&checkpoint_receipt));
    let at_checkpoint =
        WorkbenchSnapshot::new(initial.query(), 2, initial.title().clone(), false, false)
            .expect("snapshot");
    respond(&mut model, &refresh, AppResponsePayload::Workbench(at_checkpoint));

    let preview =
        preview_and_cancel(&mut model, checkpoint_command, &checkpoint_receipt, baseline, owned);

    let checkpoint_id = crate::model::format_id(checkpoint_command.operation().as_bytes());
    model.chat.buffer = format!("/rewind {checkpoint_id}");
    let second_request = request(&enter_with_metadata(&mut model));
    respond(
        &mut model,
        &second_request,
        AppResponsePayload::WorkbenchRewindPreview(preview.clone()),
    );
    let apply_request = request(&key(&mut model, KeyCode::Char('c')));
    let AppRequestPayload::WorkbenchCommand(apply) = apply_request.payload() else {
        panic!("restore command")
    };
    assert!(matches!(
        apply.intent(),
        peritus_app_protocol::WorkbenchIntent::ApplyRewind(sent) if sent == &preview
    ));
    assert_eq!(apply.expected_revision(), 2);

    let recovery = ControlOperationId::new([99; 16]).expect("recovery");
    let restored = WorkbenchRestoreReceipt::new(
        apply.operation(),
        checkpoint_command.operation(),
        recovery,
        apply.query(),
        4,
        WorkbenchRestoreStatus::Applied,
        vec!["note.txt".to_owned()],
        Vec::new(),
        preview.external_effects().to_vec(),
    )
    .expect("restore receipt");
    let refresh = request(&respond(
        &mut model,
        &apply_request,
        AppResponsePayload::WorkbenchRestore(restored.clone()),
    ));
    assert_eq!(model.chat.workbench.restore_receipt.as_ref(), Some(&restored));
    assert!(model.chat.workbench.rewind_preview.is_none());
    assert!(model.chat.buffer.is_empty());
    assert!(model.chat.workbench.message.contains("durably applied"));
    let after_restore =
        WorkbenchSnapshot::new(initial.query(), 4, initial.title().clone(), false, false)
            .expect("snapshot");
    respond(&mut model, &refresh, AppResponsePayload::Workbench(after_restore));
    assert_eq!(model.chat.workbench.snapshot.as_ref().expect("snapshot").revision(), 4);
}

#[test]
fn checkpoint_show_loads_exact_historical_references_without_mutation() {
    let mut model = checkpoint_model();
    let snapshot = selected(&mut model);
    key(&mut model, KeyCode::Esc);
    let checkpoint = ControlOperationId::new([0x42; 16]).expect("checkpoint");
    let draft = format!("/checkpoint show {}", crate::model::format_id(checkpoint.as_bytes()));
    model.chat.buffer.clone_from(&draft);

    let request = request(&enter_with_metadata(&mut model));
    let AppRequestPayload::InspectWorkbenchCheckpoint(inspect) = request.payload() else {
        panic!("checkpoint inspection request")
    };
    assert_eq!(inspect.query(), snapshot.query());
    assert_eq!(inspect.revision(), snapshot.revision());
    assert_eq!(inspect.checkpoint(), checkpoint);

    let receipt = WorkbenchCheckpointReceipt::new(
        checkpoint,
        snapshot.query(),
        snapshot.revision(),
        WorkbenchCheckpointName::new("Historical branch point".to_owned()).expect("name"),
        WorkbenchCheckpointReferences::new(1, 2, 3, Some(4)),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("receipt");
    assert!(
        respond(&mut model, &request, AppResponsePayload::WorkbenchCheckpoint(receipt.clone()),)
            .is_empty()
    );
    assert_eq!(model.chat.workbench.checkpoint_receipt.as_ref(), Some(&receipt));
    assert!(model.chat.buffer.is_empty(), "successful checkpoint inspection clears its command");
    assert!(model.chat.workbench.message.contains("Checkpoint loaded"));
}

#[test]
fn checkpoint_after_another_panel_edit_refreshes_without_overwriting_a_new_draft() {
    for changed_mind in [false, true] {
        let mut model = checkpoint_model();
        let selected = selected(&mut model);
        model.features.push(
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchBrief)
                .expect("brief feature"),
        );
        key(&mut model, KeyCode::Esc);
        model.chat.buffer = "/brief".to_owned();
        let inspect_brief = request(&key(&mut model, KeyCode::Enter));
        let brief = |revision| {
            peritus_app_protocol::WorkbenchBrief::with_sources(
                selected.query(),
                revision,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                0,
            )
            .expect("brief")
        };
        respond(&mut model, &inspect_brief, AppResponsePayload::WorkbenchBrief(brief(1)));
        key(&mut model, KeyCode::Esc);
        model.chat.buffer = "/brief constraints Preserve my draft".to_owned();
        let edit_brief = request(&key(&mut model, KeyCode::Enter));
        let AppRequestPayload::WorkbenchCommand(command) = edit_brief.payload() else {
            panic!("brief edit")
        };
        let refresh_brief = request(&respond(&mut model, &edit_brief, receipt(command)));
        respond(&mut model, &refresh_brief, AppResponsePayload::WorkbenchBrief(brief(2)));
        key(&mut model, KeyCode::Esc);
        model.chat.buffer = "/checkpoint Keep this boundary".to_owned();
        let inspect = request(&key(&mut model, KeyCode::Enter));
        assert!(
            matches!(inspect.payload(), AppRequestPayload::QueryWorkbench(query) if *query == selected.query())
        );
        assert!(model.chat.workbench.unresolved.is_none());
        if changed_mind {
            model.chat.buffer = "Actually wait".to_owned();
        }
        let current =
            WorkbenchSnapshot::new(selected.query(), 2, selected.title().clone(), false, false)
                .expect("current metadata");
        let effects = respond(&mut model, &inspect, AppResponsePayload::Workbench(current));
        if changed_mind {
            assert!(effects.is_empty());
            assert_eq!(model.chat.buffer, "Actually wait");
            assert!(model.chat.workbench.unresolved.is_none());
        } else {
            let edit = request(&effects);
            assert!(
                matches!(edit.payload(), AppRequestPayload::WorkbenchCommand(command) if command.expected_revision() == 2 && matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::CreateCheckpoint(name) if name.as_str() == "Keep this boundary"))
            );
            assert_eq!(model.chat.buffer, "/checkpoint Keep this boundary");
        }
    }
}

#[test]
fn rewind_refreshes_unobserved_automatic_edits_and_refresh_key_rebinds_revision() {
    let mut model = checkpoint_model();
    let source = selected(&mut model);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = format!("/rewind {} files", "51".repeat(16));
    let sent = request(&key(&mut model, KeyCode::Enter));
    assert!(matches!(sent.payload(), AppRequestPayload::QueryWorkbench(_)));
    let fresh = |revision| {
        WorkbenchSnapshot::new(source.query(), revision, source.title().clone(), false, false)
            .unwrap()
    };
    let preview = request(&respond(&mut model, &sent, AppResponsePayload::Workbench(fresh(30))));
    assert!(
        matches!(preview.payload(), AppRequestPayload::PreviewWorkbenchRewind(request) if request.revision() == 30)
    );
    let AppRequestPayload::PreviewWorkbenchRewind(selection) = preview.payload() else {
        panic!("preview")
    };
    let receipt =
        WorkbenchRewindPreview::new(*selection, Vec::new(), Vec::new(), Vec::new()).unwrap();
    respond(&mut model, &preview, AppResponsePayload::WorkbenchRewindPreview(receipt));
    let refresh = request(&key(&mut model, KeyCode::Char('r')));
    assert!(matches!(refresh.payload(), AppRequestPayload::QueryWorkbench(_)));
    assert!(model.chat.workbench.rewind_preview.is_none());
    let next = request(&respond(&mut model, &refresh, AppResponsePayload::Workbench(fresh(31))));
    assert!(
        matches!(next.payload(), AppRequestPayload::PreviewWorkbenchRewind(request) if request.revision() == 31)
    );
}
