//! Navigate past the first session page and reopen exact identities through keyboard input.

use super::*;
use peritus_app_protocol::{
    ConversationId, ConversationLibraryItem, ConversationLibraryPage, ConversationLibraryQuery,
    WorkbenchQuery,
};

fn library_model() -> AppModel {
    let mut model = enabled_model();
    enable_durable_chat(&mut model);
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationLibrary).unwrap(),
    );
    model
}

fn page(query: &ConversationLibraryQuery) -> ConversationLibraryPage {
    let start = query.offset();
    let end = (start + u32::from(query.limit())).min(130);
    let items = (start..end).map(|id| {
        let mut bytes = [0; 16];
        bytes[..4].copy_from_slice(&(id + 1).to_le_bytes());
        ConversationLibraryItem::new(
            WorkbenchQuery::new(ConversationId::new(bytes).unwrap(), query.workspace()),
            ConversationTitle::new(format!("Saved session {id:03} with a long Unicode title 界界界界界界界界界界界界界界界界界界界界界界界界")).unwrap(),
            false, false, 1, None, None, false, String::new(), None, None,
        ).unwrap()
    }).collect();
    ConversationLibraryPage::new(query.clone(), 130, (end < 130).then_some(end), items).unwrap()
}

fn reply_page(model: &mut AppModel, sent: &AppRequestEnvelope) -> ConversationLibraryPage {
    let AppRequestPayload::QueryConversationLibrary(query) = sent.payload() else {
        panic!("library request")
    };
    let page = page(query);
    assert!(respond(model, sent, AppResponsePayload::ConversationLibrary(page.clone())).is_empty());
    page
}

fn open_library(model: &mut AppModel) -> ConversationLibraryPage {
    model.chat.buffer = "/sessions exact search text".into();
    let sent = request(&key(model, KeyCode::Enter));
    reply_page(model, &sent)
}

#[test]
fn library_pages_and_refresh_retain_the_search_instead_of_refreshing_selected_metadata() {
    let mut model = library_model();
    let first = open_library(&mut model);
    let next = request(&key(&mut model, KeyCode::Char('n')));
    let AppRequestPayload::QueryConversationLibrary(query) = next.payload() else { panic!("next") };
    assert_eq!(query.offset(), 64);
    assert_eq!(query.literal(), first.query().literal());
    assert!(key(&mut model, KeyCode::Char('n')).is_empty(), "one page request at a time");
    let second = reply_page(&mut model, &next);
    let refresh = request(&key(&mut model, KeyCode::Char('r')));
    assert_eq!(
        refresh.payload(),
        &AppRequestPayload::QueryConversationLibrary(second.query().clone())
    );
    reply_page(&mut model, &refresh);
    let previous = request(&key(&mut model, KeyCode::Char('p')));
    assert_eq!(
        previous.payload(),
        &AppRequestPayload::QueryConversationLibrary(first.query().clone())
    );
    reply_page(&mut model, &previous);
    assert!(key(&mut model, KeyCode::Char('p')).is_empty());
}

#[test]
fn library_enter_opens_the_highlighted_identity_without_inference_or_id_transcription() {
    let mut model = library_model();
    let first = open_library(&mut model);
    key(&mut model, KeyCode::Down);
    let open = request(&key(&mut model, KeyCode::Enter));
    assert_eq!(
        open.payload(),
        &AppRequestPayload::QueryWorkbenchExecution(first.items()[1].query())
    );
    assert_eq!(model.chat.workbench.selected, Some(first.items()[1].query()));
}

#[test]
fn failed_initial_library_lookup_can_be_retried_with_r_and_keeps_the_search_draft() {
    use peritus_app_protocol::{AppErrorCode, AppProtocolError};
    let mut model = library_model();
    model.chat.buffer = "/sessions exact search text".into();
    let sent = request(&key(&mut model, KeyCode::Enter));
    respond(
        &mut model,
        &sent,
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::Internal, None)),
    );
    assert_eq!(model.chat.buffer, "/sessions exact search text");
    assert!(model.chat.workbench.message.contains("r retries the search"));
    let retry = request(&key(&mut model, KeyCode::Char('r')));
    assert_eq!(retry.payload(), sent.payload());
    reply_page(&mut model, &retry);
    assert!(model.chat.buffer.is_empty());
}

#[test]
fn final_library_page_and_narrow_selection_are_reachable_and_refresh_preserves_identity() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    for width in [40, 80, 120] {
        let mut model = library_model();
        model.chat.viewport = Some(Rect::new(0, 0, width, 24));
        open_library(&mut model);
        key(&mut model, KeyCode::End);
        assert_eq!(model.chat.workbench.library_selected, 63);
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(
            screen.contains("▶ Saved session 063"),
            "selected session is off screen at {width}: {screen}"
        );
        let refresh = request(&key(&mut model, KeyCode::Char('r')));
        reply_page(&mut model, &refresh);
        assert_eq!(model.chat.workbench.library_selected, 63);
        for _ in 0..2 {
            let next = request(&key(&mut model, KeyCode::Char('n')));
            reply_page(&mut model, &next);
        }
        let last = model.chat.workbench.library.as_ref().unwrap().clone();
        assert_eq!(last.query().offset(), 128);
        assert_eq!(last.items().len(), 2);
        assert!(key(&mut model, KeyCode::Char('n')).is_empty());
        key(&mut model, KeyCode::End);
        assert_eq!(model.chat.workbench.library_selected, 1);
        let open = request(&key(&mut model, KeyCode::Enter));
        assert_eq!(
            open.payload(),
            &AppRequestPayload::QueryWorkbenchExecution(last.items()[1].query())
        );
    }
}
