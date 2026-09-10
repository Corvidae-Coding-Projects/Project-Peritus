use super::*;

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
    let (sent, preview) = upload(&mut model, b"fixture pixels");
    respond(&mut model, &sent, AppResponsePayload::WorkbenchImagePreview(preview));
    model.chat.workbench.images.caption = "retained caption".to_owned();
    let previous = model.chat.models.clone();
    let changed = peritus_app_protocol::ProductModelChoice::new("changed-model".to_owned(), true)
        .expect("model");
    model.chat.models = ProductRoleModels::new(changed.clone(), changed.clone(), changed);
    assert!(key(&mut model, KeyCode::Char('c')).is_empty());
    model.chat.models = previous;
    let sent = request(&key(&mut model, KeyCode::Char('c')));
    respond(
        &mut model,
        &sent,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert!(model.chat.workbench.images.preview.is_none());
    assert!(model.chat.workbench.snapshot.is_none());
    assert_eq!(model.chat.workbench.images.caption, "retained caption");
    assert_eq!(model.chat.buffer, "/attach /explicit/reference.gif");
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
