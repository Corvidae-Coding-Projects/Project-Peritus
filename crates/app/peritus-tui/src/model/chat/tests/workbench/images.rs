use super::*;
use crate::image_import::ImageBytes;
mod page;
mod recovery;
use peritus_app_protocol::{
    ControlOperationId, ConversationId, OperationAcknowledgement, WorkbenchImageFormat,
    WorkbenchImageLabel, WorkbenchImageMetadata, WorkbenchImagePreview, WorkbenchImageRequest,
    WorkbenchIntent, WorkbenchQuery,
};

fn opened() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchImages)
            .expect("feature"),
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
            ConversationTitle::new("image fixture".to_owned()).expect("title"),
            false,
            false,
        )
        .expect("snapshot"),
    );
    model
}
fn read_request(model: &mut AppModel) -> ControlOperationId {
    model.chat.buffer = "/attach /explicit/reference.gif".to_owned();
    let effects = key(model, KeyCode::Enter);
    let [Effect::ReadImage { operation, path }] = effects.as_slice() else {
        panic!("read: {effects:?}")
    };
    assert_eq!(path.to_string_lossy(), "/explicit/reference.gif");
    *operation
}
fn bytes(bytes: &[u8]) -> ImageBytes {
    ImageBytes {
        bytes: bytes.to_vec(),
        digest: peritus_codec::sha256(bytes),
        label: WorkbenchImageLabel::new("reference.gif".to_owned()).expect("label"),
    }
}
fn ack(model: &mut AppModel, sent: &AppRequestEnvelope) -> Vec<Effect> {
    respond(
        model,
        sent,
        AppResponsePayload::Acknowledged(OperationAcknowledgement::new(sent.request_id())),
    )
}
fn upload(model: &mut AppModel, bytes: &[u8]) -> (AppRequestEnvelope, WorkbenchImagePreview) {
    let operation = read_request(model);
    let begin =
        request(&model.update(Action::ImageRead { operation, result: Ok(self::bytes(bytes)) }));
    assert!(matches!(begin.payload(), AppRequestPayload::BeginWorkbenchImageUpload(_)));
    let mut next = request(&ack(model, &begin));
    let mut observed = Vec::new();
    let mut ordinal = 0;
    while let AppRequestPayload::UploadArtifactChunk(chunk) = next.payload() {
        assert_eq!(chunk.offset(), observed.len() as u64);
        assert_eq!(chunk.ordinal(), ordinal);
        observed.extend_from_slice(chunk.bytes());
        let prior = next;
        next = request(&ack(model, &prior));
        assert!(ack(model, &prior).is_empty(), "duplicate ACK cannot advance transfer");
        ordinal += 1;
    }
    assert_eq!(observed, bytes);
    assert!(matches!(next.payload(), AppRequestPayload::CompleteArtifactUpload(_)));
    let sent = request(&ack(model, &next));
    let AppRequestPayload::PreviewWorkbenchImage(selection) = sent.payload() else {
        panic!("preview query")
    };
    let preview = preview(selection.clone(), bytes);
    (sent, preview)
}
fn preview(selection: WorkbenchImageRequest, bytes: &[u8]) -> WorkbenchImagePreview {
    WorkbenchImagePreview::new(
        selection,
        WorkbenchImageMetadata::new(
            peritus_codec::sha256(bytes),
            bytes.len() as u64,
            WorkbenchImageFormat::Gif,
            (1, 1),
            1,
        )
        .expect("metadata"),
        3,
        "fixture-vision".to_owned(),
    )
    .expect("preview")
}

#[test]
fn image_upload_is_chunk_ack_driven_and_confirmation_binds_preview_not_a_later_snapshot() {
    let mut model = opened();
    let (sent, preview) = upload(&mut model, &vec![7; 64 * 1024 + 17]);
    assert!(
        respond(&mut model, &sent, AppResponsePayload::WorkbenchImagePreview(preview.clone()))
            .is_empty()
    );
    assert!(key(&mut model, KeyCode::Char('c')).is_empty(), "caption is explicit");
    model.chat.workbench.images.caption = "Use this exact image λ".to_owned();
    model.chat.workbench.snapshot = Some(
        WorkbenchSnapshot::new(
            preview.request().query(),
            15,
            ConversationTitle::new("Later snapshot".to_owned()).expect("title"),
            false,
            false,
        )
        .expect("snapshot"),
    );
    let sent = request(&key(&mut model, KeyCode::Char('c')));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("confirmation")
    };
    assert_eq!(command.expected_revision(), 9);
    assert!(
        matches!(command.intent(), WorkbenchIntent::AttachImage { preview: exact, text } if exact == &preview && text.as_str() == "Use this exact image λ")
    );
    assert_eq!(model.chat.buffer, "/attach /explicit/reference.gif");
    assert!(key(&mut model, KeyCode::Char('c')).is_empty());
    respond(&mut model, &sent, receipt(command));
    assert!(model.chat.buffer.is_empty());
    assert!(model.chat.workbench.images.preview.is_none());
    assert!(model.chat.run_id.is_none());
}

#[test]
fn foreign_preview_and_wrong_ack_never_enable_confirmation_or_advance_upload() {
    let mut model = opened();
    let operation = read_request(&mut model);
    let begin =
        request(&model.update(Action::ImageRead { operation, result: Ok(bytes(b"pixels")) }));
    assert!(
        respond(
            &mut model,
            &begin,
            AppResponsePayload::Acknowledged(OperationAcknowledgement::new(
                peritus_app_protocol::RequestId::new([97; 16]).expect("wrong")
            ))
        )
        .is_empty()
    );
    let chunk = request(&ack(&mut model, &begin));
    assert!(matches!(chunk.payload(), AppRequestPayload::UploadArtifactChunk(_)));
    let complete = request(&ack(&mut model, &chunk));
    let sent = request(&ack(&mut model, &complete));
    let AppRequestPayload::PreviewWorkbenchImage(selection) = sent.payload() else {
        panic!("query")
    };
    respond(
        &mut model,
        &sent,
        AppResponsePayload::WorkbenchImagePreview(preview(selection.clone(), b"wrong!")),
    );
    assert!(model.chat.workbench.images.preview.is_none());
    model.chat.workbench.images.caption = "keep caption".to_owned();
    assert!(key(&mut model, KeyCode::Char('c')).is_empty());
    assert_eq!(model.chat.workbench.images.caption, "keep caption");
}

#[test]
fn import_editor_paste_and_escape_do_not_run_commands_or_change_the_composer() {
    let mut model = opened();
    model.chat.workbench.open = true;
    model.chat.workbench.images.open = true;
    model.chat.buffer = "original unsent draft".to_owned();
    key(&mut model, KeyCode::Char('t'));
    model.update(Action::TerminalEvent(Event::Paste("/stop".to_owned())));
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.workbench.images.caption, "/stop");
    assert_eq!(model.chat.buffer, "original unsent draft");
    key(&mut model, KeyCode::Esc);
    assert!(!model.chat.workbench.open);
    assert_eq!(model.chat.buffer, "original unsent draft");
}

#[test]
fn late_read_after_disconnect_does_not_upload_and_preserves_the_path_and_caption() {
    let mut model = opened();
    let operation = read_request(&mut model);
    model.chat.workbench.images.caption = "retained caption".to_owned();
    model.update(Action::Disconnected("lost connection".to_owned()));
    assert!(model.update(Action::ImageRead { operation, result: Ok(bytes(b"pixels")) }).is_empty());
    assert_eq!(model.chat.workbench.images.path, "/explicit/reference.gif");
    assert_eq!(model.chat.workbench.images.caption, "retained caption");
}
