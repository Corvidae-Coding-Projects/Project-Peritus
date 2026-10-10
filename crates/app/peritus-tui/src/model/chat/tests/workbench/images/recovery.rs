use super::*;

#[test]
fn escape_cancels_unconfirmed_image_read_or_upload_but_retains_the_caption() {
    for uploading in [false, true] {
        let mut model = opened();
        let operation = read_request(&mut model);
        model.chat.workbench.images.caption = "Keep this caption".into();
        let sent = uploading.then(|| {
            request(&model.update(Action::ImageRead { operation, result: Ok(bytes(b"pixels")) }))
        });
        let effects = key(&mut model, KeyCode::Esc);
        assert!(!model.workbench_request_pending());
        if let Some(sent) = sent {
            let cancel = request(&effects);
            let AppRequestPayload::BeginWorkbenchImageUpload(upload) = sent.payload() else {
                panic!("upload")
            };
            assert!(matches!(cancel.payload(), AppRequestPayload::CancelArtifact(cancel)
                if cancel.transfer_id() == upload.metadata().transfer_id()
                && cancel.artifact_id() == upload.metadata().artifact_id()));
            assert!(ack(&mut model, &cancel).is_empty());
            assert!(ack(&mut model, &sent).is_empty());
        } else {
            assert!(effects.is_empty());
            assert!(
                model
                    .update(Action::ImageRead { operation, result: Ok(bytes(b"pixels")) })
                    .is_empty()
            );
        }
        assert!(model.chat.workbench.images.preview.is_none());
        assert_eq!(model.chat.workbench.images.caption, "Keep this caption");
        assert_eq!(model.chat.buffer, "/attach /explicit/reference.gif");
        assert!(model.chat.workbench.unresolved.is_none());
    }
}

#[test]
fn reconnect_resolves_the_exact_confirmed_preview_and_never_creates_a_second_import() {
    let mut model = opened();
    let (sent, preview) = upload(&mut model, b"fixture pixels");
    respond(&mut model, &sent, AppResponsePayload::WorkbenchImagePreview(preview));
    model.chat.workbench.images.caption = "original caption".to_owned();
    let sent = request(&key(&mut model, KeyCode::Char('c')));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("confirm") };
    key(&mut model, KeyCode::Esc);
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
                ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchImages)
                    .expect("image"),
            ],
    }));
    assert_eq!(lookup.payload(), &AppRequestPayload::QueryWorkbenchReceipt(command.clone()));
    respond(&mut model, &lookup, receipt(command));
    assert_eq!(model.chat.buffer, "new unsent draft");
    assert_eq!(model.chat.workbench.images.caption, "original caption");
    assert!(model.chat.workbench.images.preview.is_none());
    assert!(!model.chat.workbench.open);
}

#[test]
fn provider_change_requires_new_preview_and_stale_confirmation_retains_caption() {
    let mut model = opened();
    let original = ProviderProfileId::new([0xb4; 16]).expect("original provider");
    active_chat_writer_binding(&mut model, original, "original-model");
    let (sent, preview) = upload(&mut model, b"fixture pixels");
    respond(&mut model, &sent, AppResponsePayload::WorkbenchImagePreview(preview));
    model.chat.workbench.images.caption = "retained caption".to_owned();
    assert!(model.chat.workbench.images.preview.is_some());

    let changed = ProviderProfileId::new([0xb5; 16]).expect("changed provider");
    active_chat_writer_binding(&mut model, changed, "changed-model");
    assert!(model.chat.workbench.images.preview.is_none());
    assert!(key(&mut model, KeyCode::Char('c')).is_empty());

    let mut confirmation = opened();
    active_chat_writer_binding(&mut confirmation, changed, "changed-model");
    let (sent, preview) = upload(&mut confirmation, b"fixture pixels");
    assert_eq!(preview.request().provider(), changed);
    assert_eq!(preview.request().model().id(), "changed-model");
    respond(&mut confirmation, &sent, AppResponsePayload::WorkbenchImagePreview(preview));
    confirmation.chat.workbench.images.caption = "retained caption".to_owned();
    let sent = request(&key(&mut confirmation, KeyCode::Char('c')));
    respond(
        &mut confirmation,
        &sent,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert!(confirmation.chat.workbench.images.preview.is_none());
    assert!(confirmation.chat.workbench.snapshot.is_none());
    assert_eq!(confirmation.chat.workbench.images.caption, "retained caption");
    assert_eq!(confirmation.chat.buffer, "/attach /explicit/reference.gif");
}

#[test]
fn image_panel_and_field_focus_preserve_composer_at_all_required_sizes() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = opened();
    let (sent, preview) = upload(&mut model, b"fixture pixels");
    respond(&mut model, &sent, AppResponsePayload::WorkbenchImagePreview(preview));
    model.chat.buffer = "retained draft λ".to_owned();
    for (width, height) in [(40, 12), (80, 24), (120, 38)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for expected in ["Attach image", "Esc back", "Message Peritus", "retained draft λ"] {
            assert!(text.contains(expected), "{width}x{height}: {expected}: {text}");
        }
        if height >= 24 {
            assert!(text.contains("image/gif"));
            assert!(text.contains("fixture-vision"));
            assert!(text.contains("SHA-256"));
        }
        key(&mut model, KeyCode::Char('t'));
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("editor");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        assert!(text.contains("Caption / instruction"));
        assert!(text.contains("retained draft λ"));
        key(&mut model, KeyCode::Esc);
    }
}
