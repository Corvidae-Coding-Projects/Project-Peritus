use super::*;
use crate::file_import::FileBytes;
use peritus_app_protocol::{
    ConversationId, OperationAcknowledgement, WorkbenchFileImportPreview, WorkbenchFileMetadata,
    WorkbenchFilePreview, WorkbenchFileRange, WorkbenchIntent, WorkbenchQuery,
};

fn opened() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchFiles).expect("files"),
    );
    let query = WorkbenchQuery::new(
        ConversationId::new([71; 16]).expect("id"),
        model.product.as_ref().expect("product").launch.workspace_id(),
    );
    model.chat.workbench.selected = Some(query);
    model.chat.workbench.snapshot = Some(
        WorkbenchSnapshot::new(
            query,
            9,
            ConversationTitle::new("files".to_owned()).expect("title"),
            false,
            false,
        )
        .expect("snapshot"),
    );
    model.chat.buffer = "@src/reference.txt".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty(), "@path only opens an inert draft");
    assert!(model.chat.workbench.files.open);
    model
}

fn ack(model: &mut AppModel, sent: &AppRequestEnvelope) -> Vec<Effect> {
    respond(
        model,
        sent,
        AppResponsePayload::Acknowledged(OperationAcknowledgement::new(sent.request_id())),
    )
}

#[test]
fn external_file_read_upload_preview_and_confirmation_bind_exact_snapshot() {
    let mut model = opened();
    model.chat.workbench.files.path = "/explicit/external.txt".to_owned();
    model.chat.workbench.files.range = "lines:2:2".to_owned();
    model.chat.workbench.files.caption = "Use this exact external line".to_owned();
    let effects = key(&mut model, KeyCode::Char('p'));
    let [Effect::ReadFile { operation, path, range }] = effects.as_slice() else {
        panic!("read: {effects:?}")
    };
    assert_eq!(path.to_string_lossy(), "/explicit/external.txt");
    assert_eq!(*range, WorkbenchFileRange::Lines { first: 2, last: 2 });
    let source = b"first\nsecond\nthird\n";
    let selected = b"second\n";
    let metadata = WorkbenchFileMetadata::new(
        peritus_codec::sha256(source),
        source.len() as u64,
        (6, 13),
        peritus_codec::sha256(selected),
    )
    .expect("metadata");
    let begin = request(&model.update(Action::FileRead {
        operation: *operation,
        result: Ok(FileBytes {
            bytes: selected.to_vec(),
            file: metadata,
            label: "external.txt".to_owned(),
        }),
    }));
    assert!(matches!(begin.payload(), AppRequestPayload::BeginWorkbenchFileUpload(_)));
    let chunk = request(&ack(&mut model, &begin));
    let AppRequestPayload::UploadArtifactChunk(chunk_payload) = chunk.payload() else {
        panic!("chunk")
    };
    assert_eq!(chunk_payload.bytes(), selected);
    let complete = request(&ack(&mut model, &chunk));
    assert!(matches!(complete.payload(), AppRequestPayload::CompleteArtifactUpload(_)));
    let preview_request = request(&ack(&mut model, &complete));
    let AppRequestPayload::PreviewWorkbenchFileImport(import) = preview_request.payload() else {
        panic!("preview request")
    };
    assert_eq!(import.selection().path(), "external.txt");
    assert_eq!(import.file(), metadata);
    let preview = WorkbenchFileImportPreview::new(import.clone(), 3, "fixture-vision".to_owned())
        .expect("preview");
    assert!(
        respond(
            &mut model,
            &preview_request,
            AppResponsePayload::WorkbenchFileImportPreview(preview.clone()),
        )
        .is_empty()
    );
    let sent = request(&key(&mut model, KeyCode::Char('c')));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("confirm") };
    assert!(
        matches!(command.intent(), WorkbenchIntent::AttachFileImport { preview: exact, text } if exact == &preview && text.as_str() == "Use this exact external line")
    );
    assert_eq!(command.expected_revision(), 9);
    respond(&mut model, &sent, receipt(command));
    assert!(model.chat.workbench.files.import_preview.is_none());
    assert!(model.chat.workbench.files.list);
    assert!(model.chat.run_id.is_none());
}

fn preview(model: &mut AppModel) -> WorkbenchFilePreview {
    key(model, KeyCode::Char('g'));
    model.update(Action::TerminalEvent(Event::Paste("lines:2:2".to_owned())));
    assert!(key(model, KeyCode::Enter).is_empty());
    key(model, KeyCode::Char('t'));
    model.update(Action::TerminalEvent(Event::Paste("Use the selected line".to_owned())));
    assert!(key(model, KeyCode::Enter).is_empty());
    let sent = request(&key(model, KeyCode::Char('p')));
    let AppRequestPayload::PreviewWorkbenchFile(selection) = sent.payload() else {
        panic!("preview")
    };
    assert_eq!(selection.range(), WorkbenchFileRange::Lines { first: 2, last: 2 });
    let preview = WorkbenchFilePreview::new(
        selection.clone(),
        peritus_codec::sha256(b"folder"),
        WorkbenchFileMetadata::new(
            peritus_codec::sha256(b"first\nsecond\n"),
            13,
            (6, 13),
            peritus_codec::sha256(b"second\n"),
        )
        .expect("metadata"),
        3,
        "fixture-model".to_owned(),
    )
    .expect("preview");
    assert!(
        respond(model, &sent, AppResponsePayload::WorkbenchFilePreview(preview.clone())).is_empty()
    );
    preview
}

#[test]
fn file_draft_paste_preview_confirm_and_receipt_do_not_start_inference() {
    let mut model = opened();
    let preview = preview(&mut model);
    assert_eq!(model.chat.buffer, "@src/reference.txt");
    let sent = request(&key(&mut model, KeyCode::Char('c')));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("confirm") };
    assert_eq!(command.expected_revision(), 9);
    assert!(
        matches!(command.intent(), WorkbenchIntent::AttachFile { preview: exact, text } if exact == &preview && text.as_str() == "Use the selected line")
    );
    assert!(key(&mut model, KeyCode::Char('c')).is_empty());
    let refresh = request(&respond(&mut model, &sent, receipt(command)));
    assert!(matches!(refresh.payload(), AppRequestPayload::QueryWorkbench(_)));
    assert!(model.chat.buffer.is_empty());
    assert!(model.chat.workbench.files.preview.is_none());
    assert!(model.chat.workbench.files.list);
    assert!(model.chat.run_id.is_none());
}

#[test]
fn file_reconnect_looks_up_original_operation_and_preserves_new_draft() {
    let mut model = opened();
    preview(&mut model);
    let sent = request(&key(&mut model, KeyCode::Char('c')));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("confirm") };
    model.update(Action::Disconnected("receipt lost".to_owned()));
    model.chat.buffer = "new unsent draft".to_owned();
    model.update(Action::Connected {
        context: sent.context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "reconnected".to_owned(),
        downgraded: false,
    });
    let lookup = request(&model.update(Action::NegotiatedFeatures {
        context: sent.context(),
        features: vec![
                ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchControl)
                    .expect("control"),
                ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchFiles)
                    .expect("files"),
            ],
    }));
    assert_eq!(lookup.payload(), &AppRequestPayload::QueryWorkbenchReceipt(command.clone()));
    respond(&mut model, &lookup, receipt(command));
    assert_eq!(model.chat.buffer, "new unsent draft");
    assert_eq!(model.chat.workbench.files.caption, "Use the selected line");
    assert!(model.chat.workbench.files.preview.is_none());
}

#[test]
fn stale_file_preview_clears_confirmation_and_refreshes_revision() {
    let mut model = opened();
    preview(&mut model);
    let sent = request(&key(&mut model, KeyCode::Char('c')));
    respond(
        &mut model,
        &sent,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert!(model.chat.workbench.files.preview.is_none());
    assert!(model.chat.workbench.snapshot.is_none());
    assert_eq!(model.chat.workbench.files.caption, "Use the selected line");
    assert!(key(&mut model, KeyCode::Char('c')).is_empty());
    assert!(matches!(
        request(&key(&mut model, KeyCode::Char('r'))).payload(),
        AppRequestPayload::QueryWorkbench(_)
    ));
}

#[test]
fn file_panel_and_inert_editor_preserve_composer_at_required_sizes() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = opened();
    preview(&mut model);
    model.chat.buffer = "retained draft λ".to_owned();
    for (width, height) in [(40, 12), (80, 24), (120, 38)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for expected in ["Files", "Message Peritus", "retained draft λ"] {
            assert!(text.contains(expected), "{width}x{height}: {expected}: {text}");
        }
        key(&mut model, KeyCode::Char('t'));
        model.update(Action::TerminalEvent(Event::Paste("/stop".to_owned())));
        assert!(key(&mut model, KeyCode::Enter).is_empty());
        assert_eq!(model.chat.buffer, "retained draft λ");
    }
    model
        .features
        .retain(|feature| feature.as_str() != WellKnownProtocolFeature::WorkbenchFiles.as_str());
    assert!(key(&mut model, KeyCode::Char('p')).is_empty());
}
