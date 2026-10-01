use super::*;
mod freshness;
use peritus_app_protocol::{
    WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputRow, WorkbenchInputSelection,
    WorkbenchInputState, WorkbenchInputText, WorkbenchIntent, WorkbenchQueueIntent,
    WorkbenchQueuePage, WorkbenchQueueQuery,
};

fn opened() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchInputs)
            .expect("feature"),
    );
    let (sent, command) = create(&mut model);
    let query = request(&respond(&mut model, &sent, receipt(&command)));
    respond(
        &mut model,
        &query,
        AppResponsePayload::Workbench(
            WorkbenchSnapshot::new(
                command.query(),
                1,
                ConversationTitle::new("Private fixture".to_owned()).expect("title"),
                false,
                false,
            )
            .expect("snapshot"),
        ),
    );
    key(&mut model, KeyCode::Esc);
    model
}
fn row(state: WorkbenchInputState) -> WorkbenchInputRow {
    WorkbenchInputRow::new(
        WorkbenchInputSelection::new(WorkbenchInputId::new([72; 16]).expect("id"), 3)
            .expect("selection"),
        WorkbenchInputText::new("Exact immutable steering λ".to_owned()).expect("text"),
        state,
        WorkbenchInputOrder::new(Vec::new()).expect("dependencies"),
    )
    .expect("row")
}
fn inspect(model: &mut AppModel) {
    model.chat.buffer = "/queue".to_owned();
    let sent = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchQueue(query) = sent.payload() else {
        panic!("queue query")
    };
    let page_query = WorkbenchQueueQuery::new(query.query(), 5, 0, false).expect("query");
    respond(
        model,
        &sent,
        AppResponsePayload::WorkbenchQueue(
            WorkbenchQueuePage::new(page_query, 1, vec![row(WorkbenchInputState::Queued)])
                .expect("page"),
        ),
    );
    assert!(model.chat.buffer.is_empty(), "successful inspection consumes its slash command");
}

fn fresh_metadata(model: &mut AppModel, sent: &AppRequestEnvelope, revision: u64) -> Vec<Effect> {
    let AppRequestPayload::QueryWorkbench(query) = sent.payload() else {
        panic!("fresh metadata query")
    };
    respond(
        model,
        sent,
        AppResponsePayload::Workbench(
            WorkbenchSnapshot::new(
                *query,
                revision,
                ConversationTitle::new("Current title".into()).unwrap(),
                false,
                false,
            )
            .unwrap(),
        ),
    )
}

#[test]
fn queue_uses_exact_inspected_content_revision_and_only_clears_draft_after_receipt() {
    let mut model = opened();
    inspect(&mut model);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/queue edit 1 Corrected instruction".to_owned();
    let refresh = request(&key(&mut model, KeyCode::Enter));
    assert!(model.chat.workbench.unresolved.is_none());
    let sent = request(&fresh_metadata(&mut model, &refresh, 5));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("command") };
    assert_eq!(command.expected_revision(), 5);
    assert!(
        matches!(command.intent(), WorkbenchIntent::Queue(WorkbenchQueueIntent::Edit { selected, text })
        if selected.revision() == 3 && text.as_str() == "Corrected instruction")
    );
    assert_eq!(model.chat.buffer, "/queue edit 1 Corrected instruction");
    let refreshed = request(&respond(&mut model, &sent, receipt(command)));
    assert!(matches!(refreshed.payload(), AppRequestPayload::QueryWorkbenchQueue(_)));
    assert!(model.chat.buffer.is_empty());
    assert!(model.chat.run_id.is_none());
}

#[test]
fn queue_mutation_keeps_its_exact_input_after_a_metadata_receipt_advances_the_conversation() {
    let mut model = opened();
    inspect(&mut model);
    let query = model.chat.workbench.queue.as_ref().unwrap().query().query();
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/sessions rename Renamed".to_owned();
    let refresh = request(&key(&mut model, KeyCode::Enter));
    let metadata = |revision| {
        WorkbenchSnapshot::new(
            query,
            revision,
            ConversationTitle::new("Renamed".to_owned()).unwrap(),
            false,
            false,
        )
        .unwrap()
    };
    let rename =
        request(&respond(&mut model, &refresh, AppResponsePayload::Workbench(metadata(5))));
    let AppRequestPayload::WorkbenchCommand(command) = rename.payload() else {
        panic!("rename command")
    };
    let refreshed = request(&respond(&mut model, &rename, receipt(command)));
    respond(&mut model, &refreshed, AppResponsePayload::Workbench(metadata(6)));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/queue hold 1".to_owned();
    let refresh = request(&key(&mut model, KeyCode::Enter));
    let hold = request(&fresh_metadata(&mut model, &refresh, 7));
    let AppRequestPayload::WorkbenchCommand(command) = hold.payload() else { panic!("queue hold") };
    assert_eq!(command.expected_revision(), 7, "use fresh aggregate metadata");
    assert!(
        matches!(command.intent(), WorkbenchIntent::Queue(WorkbenchQueueIntent::Hold { selected, held: true }) if *selected == row(WorkbenchInputState::Queued).selected())
    );
    assert_eq!(model.chat.buffer, "/queue hold 1", "keep the draft until its exact receipt");
}

#[test]
fn queue_rejects_uninspected_rows_and_unsupported_operations_without_losing_text() {
    let mut model = opened();
    inspect(&mut model);
    for text in [
        "/queue hold 2",
        "/queue edit 1",
        "/queue order invalid",
        "/queue hold 1 extra",
        "/queue correct 1 changed pending text",
    ] {
        key(&mut model, KeyCode::Esc);
        model.chat.buffer = text.to_owned();
        assert!(key(&mut model, KeyCode::Enter).is_empty());
        assert_eq!(model.chat.buffer, text);
    }
    model
        .features
        .retain(|feature| feature.as_str() != WellKnownProtocolFeature::WorkbenchInputs.as_str());
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/queue add no authority".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.buffer, "/queue add no authority");
}

#[test]
fn queue_panel_preserves_the_composer_and_return_hint_at_all_supported_sizes() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = opened();
    inspect(&mut model);
    model.chat.buffer = "retained draft λ".to_owned();
    model.chat.cursor = 4;
    for (width, height) in [(40, 12), (80, 24), (120, 38)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for expected in ["Queue", "Esc back", "Message Peritus", "retained draft λ"] {
            assert!(text.contains(expected), "{width}x{height} missing {expected}: {text}");
        }
    }
    key(&mut model, KeyCode::Esc);
    assert_eq!(model.chat.buffer, "retained draft λ");
    assert_eq!(model.chat.cursor, 4);
}

#[test]
fn exact_input_detail_reaches_text_beyond_the_compact_list_and_old_scroll_ceiling() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = opened();
    inspect(&mut model);
    let query = model.chat.workbench.queue.as_ref().expect("page").query();
    let text = format!("{}TAIL_EXACT_MARKER", "line\n".repeat(1200));
    let detail = WorkbenchInputRow::new(
        row(WorkbenchInputState::Queued).selected(),
        WorkbenchInputText::new(text).expect("text"),
        WorkbenchInputState::Queued,
        WorkbenchInputOrder::new(Vec::new()).expect("deps"),
    )
    .expect("row");
    model.chat.workbench.queue =
        Some(WorkbenchQueuePage::new(query, 1, vec![detail]).expect("page"));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/queue show 1".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty(), "inspection performs no request");
    assert!(model.chat.buffer.is_empty(), "successful local inspection consumes its command");
    for _ in 0..240 {
        key(&mut model, KeyCode::PageDown);
    }
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
    let text: String = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(text.contains("TAIL_EXACT_MARKER"), "{text}");
    assert!(text.contains("Esc back"));
}
