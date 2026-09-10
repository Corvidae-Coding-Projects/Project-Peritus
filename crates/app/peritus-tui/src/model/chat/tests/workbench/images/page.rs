use super::*;
use peritus_app_protocol::{WorkbenchImagePage, WorkbenchImageQuery};

fn page(scope: WorkbenchQuery) -> WorkbenchImagePage {
    let fixture = peritus_app_protocol::schema::generated_fixture_cases()
        .expect("fixtures")
        .into_iter()
        .find(|case| case.case == "realistic-workbench-image-page")
        .expect("page fixture");
    let AppMessage::Response(envelope) =
        peritus_app_protocol::decode_app_message(&fixture.payload, AppProtocolLimits::PRODUCTION)
            .expect("decode")
    else {
        panic!("response")
    };
    let AppResponsePayload::WorkbenchImages(page) = envelope.payload() else { panic!("page") };
    WorkbenchImagePage::new(
        WorkbenchImageQuery::new(scope, 12, 0).expect("query"),
        page.total(),
        page.rows().to_vec(),
    )
    .expect("rebound")
}

fn inspect(model: &mut AppModel) -> (AppRequestEnvelope, WorkbenchImagePage) {
    model.chat.buffer = "/attach".to_owned();
    let sent = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchImages(query) = sent.payload() else {
        panic!("read-only image query")
    };
    assert_eq!(query.revision(), 0);
    let page = page(query.query());
    respond(model, &sent, AppResponsePayload::WorkbenchImages(page.clone()));
    (sent, page)
}

#[test]
fn image_selection_binds_inspected_revision_and_waits_for_a_receipt() {
    let mut model = opened();
    let (_, page) = inspect(&mut model);
    model.chat.buffer = "unsent composer".to_owned();
    let sent = request(&key(&mut model, KeyCode::Char(' ')));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("select") };
    assert_eq!(command.expected_revision(), 12, "not snapshot revision 9");
    assert_eq!(
        command.intent(),
        &WorkbenchIntent::SelectImage { attachment: page.rows()[0].operation(), selected: false }
    );
    assert!(
        model.chat.workbench.images.page.as_ref().expect("page").rows()[0].selected(),
        "not optimistic"
    );
    assert!(key(&mut model, KeyCode::Char(' ')).is_empty());
    model.chat.buffer = "newer unsent composer".to_owned();
    let refresh = request(&respond(&mut model, &sent, receipt(command)));
    assert!(matches!(refresh.payload(), AppRequestPayload::QueryWorkbenchImages(_)));
    assert_eq!(model.chat.buffer, "newer unsent composer");
    assert!(model.chat.run_id.is_none());
}

#[test]
fn wrong_scope_and_stale_page_disable_selection_without_losing_drafts() {
    let mut model = opened();
    let (_, page) = inspect(&mut model);
    let sent = request(&key(&mut model, KeyCode::Char('r')));
    let wrong = WorkbenchQuery::new(
        ConversationId::new([99; 16]).expect("other"),
        page.query().query().workspace(),
    );
    respond(&mut model, &sent, AppResponsePayload::WorkbenchImages(self::page(wrong)));
    assert!(model.chat.workbench.images.page.is_none());
    assert!(key(&mut model, KeyCode::Char(' ')).is_empty());
    let sent = request(&key(&mut model, KeyCode::Char('r')));
    respond(&mut model, &sent, AppResponsePayload::WorkbenchImages(page));
    let sent = request(&key(&mut model, KeyCode::Char(' ')));
    respond(
        &mut model,
        &sent,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert!(model.chat.workbench.images.page.is_none());
    assert!(key(&mut model, KeyCode::Char(' ')).is_empty());
    assert_eq!(model.chat.buffer, "/attach");
}

#[test]
fn image_list_row_navigation_scroll_and_import_switch_preserve_drafts() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = opened();
    inspect(&mut model);
    model.chat.buffer = "retained draft λ".to_owned();
    for (width, height) in [(40, 12), (80, 24), (120, 38)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for expected in ["Images", "Esc back", "retained draft λ"] {
            assert!(text.contains(expected), "{width}x{height}: {text}");
        }
        if height >= 24 {
            assert!(text.contains("eligible next turn: true"));
        }
    }
    key(&mut model, KeyCode::Right);
    assert_eq!(model.chat.workbench.images.selected, 1);
    key(&mut model, KeyCode::PageDown);
    assert_eq!(model.chat.workbench.scroll, 5);
    key(&mut model, KeyCode::Left);
    assert_eq!(model.chat.workbench.scroll, 0);
    let refresh = request(&key(&mut model, KeyCode::Char('i')));
    assert!(matches!(refresh.payload(), AppRequestPayload::QueryWorkbench(_)), "no implicit read");
    assert!(!model.chat.workbench.images.list);
    assert_eq!(model.chat.buffer, "retained draft λ");
}

#[test]
fn narrow_image_details_are_reachable_line_by_line_without_skipping_identity_or_state() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = opened();
    inspect(&mut model);
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).expect("terminal");
    let mut observed = String::new();
    for offset in 0..50 {
        assert_eq!(model.chat.workbench.scroll, offset);
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        observed.extend(frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol));
        key(&mut model, KeyCode::Down);
    }
    for expected in [
        "reference-70.gif",
        "Selected: true",
        "eligible next",
        "image/gif",
        "SHA-256",
        "Import",
        "Artifact",
        "Caption source",
        "Use this exact reference",
        "r refresh",
    ] {
        assert!(observed.contains(expected), "unreachable: {expected}");
    }
    assert_eq!(model.chat.workbench.images.selected, 0, "scroll is not selection");
}
