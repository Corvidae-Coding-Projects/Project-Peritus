use super::*;
use peritus_app_protocol::{
    WorkbenchContextDisposition as D, WorkbenchContextPage, WorkbenchContextPreference as P,
    WorkbenchContextQuery, WorkbenchContextRow, WorkbenchContextSource as S,
    WorkbenchContextView as V, WorkbenchInputId, WorkbenchInputSelection, WorkbenchIntent,
    WorkbenchQuery,
};

fn opened() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchContext)
            .expect("feature"),
    );
    model.chat.workbench.selected = Some(WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([64; 16]).expect("conversation"),
        model.product.as_ref().expect("product").launch.workspace_id(),
    ));
    model
}
fn row(revision: u64) -> WorkbenchContextRow {
    WorkbenchContextRow::new(
        S::Input(
            WorkbenchInputSelection::new(WorkbenchInputId::new([65; 16]).expect("input"), revision)
                .expect("selection"),
        ),
        peritus_types::Sha256Digest::new([66; 32]),
        123,
        D::Eligible,
    )
    .expect("row")
}
fn page(query: WorkbenchContextQuery, total: u32) -> WorkbenchContextPage {
    let query =
        WorkbenchContextQuery::new(query.query(), 9, query.offset(), query.view()).expect("query");
    let rows =
        (query.offset()..total.min(query.offset() + 32)).map(|i| row(u64::from(i) + 1)).collect();
    WorkbenchContextPage::new(query, total, None, rows).expect("page")
}
fn inspect(model: &mut AppModel, total: u32) {
    model.chat.buffer = "/context next".to_owned();
    let sent = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchContext(query) = sent.payload() else {
        panic!("context query");
    };
    respond(model, &sent, AppResponsePayload::WorkbenchContext(page(*query, total)));
}

#[test]
fn context_is_read_only_feature_gated_and_retains_draft_when_a_late_response_arrives() {
    let mut unavailable = enabled_model();
    unavailable.chat.buffer = "/context".to_owned();
    assert!(key(&mut unavailable, KeyCode::Enter).is_empty());
    assert_eq!(unavailable.chat.buffer, "/context");
    let mut model = opened();
    model.chat.buffer = "/context next".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchContext(query) = sent.payload() else {
        panic!("query");
    };
    assert_eq!(query.view(), V::Next);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "my retained draft λ".to_owned();
    model.chat.cursor = 3;
    respond(&mut model, &sent, AppResponsePayload::WorkbenchContext(page(*query, 1)));
    assert!(!model.chat.workbench.open);
    assert!(model.chat.workbench.context_page.is_some());
    assert_eq!(model.chat.buffer, "my retained draft λ");
    assert_eq!(model.chat.cursor, 3);
    assert!(model.chat.run_id.is_none());
}

#[test]
fn context_pagination_uses_the_inspected_revision_and_rejects_cross_view_responses() {
    let mut model = opened();
    inspect(&mut model, 33);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/context more".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchContext(query) = sent.payload() else {
        panic!("query");
    };
    assert_eq!(query.offset(), 32);
    assert_eq!(query.revision(), 9);
    respond(&mut model, &sent, AppResponsePayload::WorkbenchContext(page(*query, 33)));
    assert_eq!(model.chat.workbench.context_page.as_ref().expect("page").rows().len(), 1);
    assert!(model.chat.workbench.open);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/context next".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let wrong = WorkbenchContextQuery::new(query.query(), 9, 0, V::History).expect("query");
    let wrong = WorkbenchContextPage::new(wrong, 0, None, Vec::new()).expect("page");
    respond(&mut model, &sent, AppResponsePayload::WorkbenchContext(wrong));
    assert!(model.chat.workbench.context_page.is_none());
    assert_eq!(model.chat.buffer, "/context next");
}

#[test]
fn context_preferences_bind_the_exact_numbered_row_and_inspected_revision() {
    let mut model = opened();
    inspect(&mut model, 1);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/context pin 1".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("context preference command");
    };
    assert_eq!(command.expected_revision(), 9);
    assert!(matches!(
        command.intent(),
        WorkbenchIntent::SetContext {
            source: S::Input(selected),
            preference: Some(P::Pinned),
        } if selected.revision() == 1
    ));

    let mut invalid = opened();
    inspect(&mut invalid, 1);
    key(&mut invalid, KeyCode::Esc);
    invalid.chat.buffer = "/context exclude 1".to_owned();
    assert!(key(&mut invalid, KeyCode::Enter).is_empty());
    assert_eq!(invalid.chat.buffer, "/context exclude 1");
}

#[test]
fn context_panel_keeps_composer_and_exit_hint_visible_at_all_supported_sizes() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = opened();
    inspect(&mut model, 1);
    model.chat.buffer = "retained draft λ".to_owned();
    for (width, height) in [(40, 12), (80, 24), (120, 38)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for expected in ["Context", "Esc back", "Message Peritus", "retained draft λ"] {
            assert!(text.contains(expected), "{width}x{height} missing {expected}: {text}");
        }
    }
}

#[test]
fn context_image_row_renders_exact_size_and_explicit_exclusion_without_exposing_content() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = opened();
    inspect(&mut model, 1);
    let query = model.chat.workbench.context_page.as_ref().expect("page").query();
    let row = WorkbenchContextRow::new(
        S::Image {
            operation: peritus_app_protocol::ControlOperationId::new([67; 16]).expect("operation"),
            input: WorkbenchInputId::new([68; 16]).expect("input"),
            artifact: peritus_types::ArtifactId::new([69; 16]).expect("artifact"),
        },
        peritus_types::Sha256Digest::new([70; 32]),
        4321,
        D::UserExcluded,
    )
    .expect("row")
    .with_preference(P::Excluded)
    .expect("preference");
    model.chat.workbench.context_page =
        Some(WorkbenchContextPage::new(query, 1, None, vec![row]).expect("page"));
    let mut terminal = Terminal::new(TestBackend::new(120, 38)).expect("terminal");
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
    let text: String = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
    for expected in [
        "Image",
        "caption",
        "artifact",
        "excluded: user context preference",
        "Preference: excluded",
        "4321 bytes",
        "SHA256",
        "Message Peritus",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
}
