use super::*;
use crate::model::{Editor, EditorKind};
use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, AppRequestEnvelope, AppResponseEnvelope, AppResponsePayload,
};

fn model() -> AppModel {
    let provider = ProviderProfileId::new([81; 16]).unwrap();
    let launch = ProductLaunchContext::new(
        WorkspaceId::new([82; 16]).unwrap(),
        "/managed/project".to_owned(),
        vec![ProductProviderOption::new(provider, "Test")],
        Some(0),
    )
    .unwrap();
    let mut model = AppModel::with_product([83; 32], Some(launch));
    model.context = Some(context());
    model.view = View::Runs;
    model
}

fn draft(model: &mut AppModel, kind: EditorKind, text: &str) {
    model.editor = Some(Editor {
        kind,
        title: "Draft",
        hint: "Enter submits",
        buffer: text.to_owned(),
        cursor: text.len(),
        pasted_command: false,
    });
}

fn submit(model: &mut AppModel) -> Vec<Effect> {
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    ))))
}

fn request(effects: Vec<Effect>) -> AppRequestEnvelope {
    effects
        .into_iter()
        .find_map(|effect| {
            if let Effect::Send(AppMessage::Request(request)) = effect {
                Some(request)
            } else {
                None
            }
        })
        .unwrap()
}

fn reject(model: &mut AppModel, request: &AppRequestEnvelope) {
    let _ = model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::Backpressure, None)),
    ))));
}

#[test]
fn offline_submission_retains_the_complete_modal_draft() {
    let mut model = model();
    model.context = None;
    draft(&mut model, EditorKind::ProductTask, "build this\nwith café text");
    assert!(submit(&mut model).is_empty());
    assert_eq!(model.editor.as_ref().unwrap().buffer, "build this\nwith café text");
}

#[test]
fn process_id_validation_rejection_and_timeout_preserve_input() {
    let mut model = model();
    draft(&mut model, EditorKind::ProcessId, "1111111111111111111111111111111g");
    assert!(submit(&mut model).is_empty());
    assert_eq!(model.editor.as_ref().unwrap().buffer, "1111111111111111111111111111111g");
    draft(&mut model, EditorKind::ProcessId, "11111111111111111111111111111111");
    let first = request(submit(&mut model));
    reject(&mut model, &first);
    assert_eq!(model.editor.as_ref().unwrap().buffer, "11111111111111111111111111111111");
    let second = request(submit(&mut model));
    assert!(model.pending_started.contains_key(&second.request_id()));
    model.tick_count = 121;
    assert!(model.expire_pending_requests());
    assert_eq!(model.editor.as_ref().unwrap().buffer, "11111111111111111111111111111111");
}

#[test]
fn reconnect_keeps_the_open_editor_and_never_resubmits_it() {
    let mut model = model();
    draft(&mut model, EditorKind::ProductTask, "retain while reconnecting");
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('r'),
        KeyModifiers::CONTROL,
    ))));
    assert!(matches!(effects.as_slice(), [Effect::Reconnect]));
    assert_eq!(model.editor.as_ref().unwrap().buffer, "retain while reconnecting");
    assert!(model.pending.is_empty());
}

#[test]
fn rejected_task_or_message_restores_the_exact_draft() {
    for kind in [
        EditorKind::ProductTask,
        EditorKind::ProductMessage(peritus_types::RunId::new([90; 16]).unwrap()),
    ] {
        let mut model = model();
        draft(&mut model, kind.clone(), "change my mind\nkeep this reply");
        let request = request(submit(&mut model));
        assert!(model.editor.is_none());
        reject(&mut model, &request);
        let restored = model.editor.as_ref().unwrap();
        assert_eq!(restored.kind, kind);
        assert_eq!(restored.buffer, "change my mind\nkeep this reply");
        assert_eq!(restored.cursor, restored.buffer.len());
    }
}

#[test]
fn late_rejection_keeps_the_new_draft_and_recovers_the_old_one_separately() {
    let mut model = model();
    draft(&mut model, EditorKind::ProductTask, "first submitted task");
    let request = request(submit(&mut model));
    draft(&mut model, EditorKind::ProductTask, "new task still being typed");
    reject(&mut model, &request);
    assert_eq!(model.editor.as_ref().unwrap().buffer, "new task still being typed");
    model.editor = None;
    model.open_task_composer();
    assert_eq!(model.editor.as_ref().unwrap().buffer, "first submitted task");
}

#[test]
fn disconnect_and_timeout_retain_text_without_resubmitting_it() {
    for disconnect in [true, false] {
        let mut model = model();
        draft(&mut model, EditorKind::ProductTask, "retain unknown outcome");
        let _ = request(submit(&mut model));
        if disconnect {
            let effects = model.update(Action::Disconnected("connection lost".to_owned()));
            assert!(effects.is_empty());
        } else {
            model.tick_count = 121;
            assert!(model.expire_pending_requests());
        }
        let editor = model.editor.as_ref().unwrap();
        assert_eq!(editor.buffer, "retain unknown outcome");
        assert!(editor.hint.contains("may already have been accepted"));
        assert!(model.pending.is_empty());
    }
}

#[test]
fn modal_up_and_down_follow_the_visible_unicode_rows() {
    let mut model = model();
    model.chat.viewport = Some(ratatui::layout::Rect::new(0, 0, 80, 24));
    draft(&mut model, EditorKind::ProductTask, "first 界λ\nnext 界λ");
    model.handle_editor_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(model.editor.as_ref().unwrap().cursor, "first 界".len());
    model.handle_editor_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let editor = model.editor.as_ref().unwrap();
    assert_eq!(editor.cursor, editor.buffer.len());
}

#[test]
fn modal_paste_and_typing_enforce_submission_limits_without_truncating_the_draft() {
    let mut model = model();
    let maximum = peritus_app_protocol::MAX_PRODUCT_TASK_BYTES;
    let original = "x".repeat(maximum - 1);
    draft(&mut model, EditorKind::ProductTask, &original);
    model.update(Action::TerminalEvent(Event::Paste("界".to_owned())));
    assert!(
        model.editor.as_ref().unwrap().buffer == original,
        "the complete prior draft must survive"
    );
    model.handle_editor_key(KeyEvent::new(KeyCode::Char('界'), KeyModifiers::NONE));
    assert!(
        model.editor.as_ref().unwrap().buffer == original,
        "the complete prior draft must survive"
    );
    model.handle_editor_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(model.editor.as_ref().unwrap().buffer.len(), maximum);
    model.handle_editor_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(model.editor.as_ref().unwrap().buffer.len(), maximum);
    assert!(model.notice.as_ref().unwrap().text.contains("draft retained"));
    model.handle_editor_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(
        model.editor.as_ref().unwrap().buffer == original,
        "the complete prior draft must survive"
    );
}
