use super::*;
use crate::model::{PromptItem, PromptPhase};
use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, AppResponseEnvelope, AppResponsePayload, PromptBinding,
    PromptCorrelation, PromptId, PromptKind, RequestId,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, Generation, HarnessId, PolicyId, RevisionNumber, RevisionTuple,
    Sha256Digest,
};

fn prompted() -> AppModel {
    let mut model = AppModel::new([41; 32]);
    model.context = Some(context());
    model.view = View::Approvals;
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([1; 16]).unwrap(),
        HarnessId::new([2; 16]).unwrap(),
        WorkspaceId::new([3; 16]).unwrap(),
        Generation::new(1).unwrap(),
        RevisionNumber::new(1).unwrap(),
        PolicyId::new([4; 16]).unwrap(),
        ProviderProfileId::new([5; 16]).unwrap(),
    );
    let correlation = PromptCorrelation::new(
        RequestId::new([6; 16]).unwrap(),
        PromptId::new([7; 16]).unwrap(),
        context().session_id(),
        ActorId::new([8; 16]).unwrap(),
        revision,
        Sha256Digest::new([9; 32]),
        Generation::new(1).unwrap(),
    );
    let binding =
        PromptBinding::new(PromptKind::UserInput, correlation, Vec::new(), Vec::new(), 1, 1)
            .unwrap();
    model.prompts.push(PromptItem { binding, phase: PromptPhase::Pending });
    model
}

fn key(model: &mut AppModel, code: KeyCode) -> Vec<Effect> {
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))))
}

#[test]
fn rejected_prompt_restores_text_and_can_be_reopened_after_canceling_the_editor() {
    let mut model = prompted();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    model.editor.as_mut().unwrap().buffer = "user answer".to_owned();
    let effects = key(&mut model, KeyCode::Enter);
    let Effect::Send(AppMessage::Request(request)) = &effects[0] else { panic!("answer request") };
    let _ = model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::Backpressure, None)),
    ))));
    assert_eq!(model.prompts[0].phase, PromptPhase::Failed);
    assert_eq!(model.editor.as_ref().unwrap().buffer, "user answer");
    key(&mut model, KeyCode::Esc);
    key(&mut model, KeyCode::Enter);
    assert!(model.editor.is_some());
}

#[test]
fn offline_prompt_submission_does_not_get_stuck_submitting() {
    let mut model = prompted();
    model.context = None;
    key(&mut model, KeyCode::Enter);
    model.editor.as_mut().unwrap().buffer = "offline answer".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.prompts[0].phase, PromptPhase::Pending);
    assert_eq!(model.editor.as_ref().unwrap().buffer, "offline answer");
}

#[test]
fn prompt_cancellation_waits_for_acknowledgement_and_disconnect_releases_pending_prompt_state() {
    for disconnect in [true, false] {
        let mut model = prompted();
        assert_eq!(key(&mut model, KeyCode::Char('c')).len(), 1);
        assert_eq!(model.prompts[0].phase, PromptPhase::Submitting);
        if disconnect {
            let _ = model.update(Action::Disconnected("lost".to_owned()));
        } else {
            model.tick_count = 121;
            let effects = model.update(Action::Tick(std::time::Instant::now()));
            assert!(!effects.iter().any(|effect| matches!(effect, Effect::Reconnect)));
            assert_eq!(model.prompts[0].phase, PromptPhase::Submitting);
            model.update(Action::Disconnected("lost".to_owned()));
        }
        assert_eq!(model.prompts[0].phase, PromptPhase::Failed);
        key(&mut model, KeyCode::Enter);
        assert!(model.editor.is_some());
    }
}

#[test]
fn all_prompt_choices_and_constraints_are_reachable_before_answering() {
    use peritus_app_protocol::{PromptChoice, PromptConstraint};
    let mut model = prompted();
    let correlation = model.prompts[0].binding.correlation();
    let choices = (0..20)
        .map(|number| {
            PromptChoice::new(
                format!("option-{number:02}"),
                format!("Human-readable option {number:02} CHOICE_TAIL"),
                64,
                256,
            )
            .unwrap()
        })
        .collect();
    model.prompts[0].binding = PromptBinding::new(
        PromptKind::UserInput,
        correlation,
        choices,
        vec![PromptConstraint::BoundChoiceOnly],
        20,
        1,
    )
    .unwrap();
    model.chat.viewport = Some(ratatui::layout::Rect::new(0, 0, 80, 24));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    for _ in 0..100 {
        key(&mut model, KeyCode::PageDown);
    }
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
    let bottom = frame.buffer.clone();
    let text = bottom.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
    for expected in ["option-19", "BoundChoiceOnly", "Enter: respond"] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    key(&mut model, KeyCode::PageUp);
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
    assert_ne!(frame.buffer, &bottom);
    key(&mut model, KeyCode::Home);
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
    let text = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
    assert!(text.contains("Kind"));
    key(&mut model, KeyCode::Enter);
    model.editor.as_mut().unwrap().buffer = "option-19".to_owned();
    assert!(key(&mut model, KeyCode::Enter).iter().any(|effect| matches!(effect, Effect::Send(_))));
}
