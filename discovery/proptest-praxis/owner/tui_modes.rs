//! Actual composer/reducer replay: stop read-only work before a requested mode transition.
use super::*;
use crate::model::AppModel;
use peritus_app_protocol::*;
use peritus_types::{ProviderProfileId, RunId, SessionId, WorkspaceId};

fn snapshot(mode: Mode, state: ProductRunOperationState) -> ProductInteractionSnapshot {
    let run = RunId::new([1; 16]).unwrap();
    let profile = ProviderProfileId::new([2; 16]).unwrap();
    let running = state == ProductRunOperationState::Running;
    let operation = ProductRunOperation::new(
        ProductRunOperationKind::Execution,
        state,
        "mode-replay".into(),
        "Observed native run".into(),
        String::new(),
        ProductRunLegalControls::none().with(if running {
            Control::Cancel
        } else {
            Control::Retry
        }),
    )
    .unwrap();
    let run = ProductRunSnapshot::new(
        run,
        WorkspaceId::new([3; 16]).unwrap(),
        ProductProviderSelection::new(profile, profile, profile),
        if running { ProductRunPhase::Writing } else { ProductRunPhase::Cancelled },
        1,
        "Mode transition replay".into(),
        "Observed".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        operation,
    )
    .unwrap();
    ProductInteractionSnapshot::new(run, mode, ProductRoleModels::default(), 0, 0, vec![], None)
        .unwrap()
}
fn model() -> AppModel {
    let mut model = AppModel::with_product([5; 32], None);
    model.context = Some(ProtocolContext::new(
        ProtocolId::new([6; 16]).unwrap(),
        ProtocolVersion::new(2, 1).unwrap(),
        SessionId::new([7; 16]).unwrap(),
    ));
    model.features = [
        WellKnownProtocolFeature::WorkbenchControl,
        WellKnownProtocolFeature::WorkbenchInputs,
        WellKnownProtocolFeature::WorkbenchExecution,
        WellKnownProtocolFeature::WorkbenchConversation,
    ]
    .into_iter()
    .map(|f| ProtocolFeatureName::well_known(f).unwrap())
    .collect();
    let s = snapshot(Mode::Review, ProductRunOperationState::Running);
    model.chat.run_id = Some(s.snapshot().run_id());
    model.chat.workbench.selected = Some(WorkbenchQuery::new(
        ConversationId::new([8; 16]).unwrap(),
        s.snapshot().workspace_id(),
    ));
    model.accept_chat(s);
    model
}

#[test]
fn review_to_chat_cancels_then_submits_the_retained_message_once() {
    let mut m = model();
    let effects = m.chat_mode_command(Mode::Chat, "continue coding");
    assert!(effects.iter().any(|effect|matches!(effect,Effect::Send(AppMessage::Request(r)) if matches!(r.payload(),AppRequestPayload::ControlProductRun(c) if c.action()==Control::Cancel))));
    assert_eq!(m.chat.mode, Mode::Review);
    assert_eq!(m.chat.buffer, "continue coding");
    assert!(m.finish_chat_mode_change().is_empty());
    m.accept_chat(snapshot(Mode::Review, ProductRunOperationState::Cancelled));
    let effects = m.finish_chat_mode_change();
    assert_eq!(m.chat.mode, Mode::Chat);
    assert!(effects.iter().any(|effect|matches!(effect,Effect::Send(AppMessage::Request(r)) if matches!(r.payload(),AppRequestPayload::QueryWorkbenchExecution(_)))));
    assert!(m.finish_chat_mode_change().is_empty());
}

#[test]
fn edited_draft_or_changed_destination_never_submits_the_old_followup() {
    for navigate in [false, true] {
        let mut m = model();
        m.chat_mode_command(Mode::Chat, "continue coding");
        if navigate {
            m.chat.workbench.selected = None;
        } else {
            "changed draft".clone_into(&mut m.chat.buffer);
        }
        m.accept_chat(snapshot(Mode::Review, ProductRunOperationState::Cancelled));
        assert!(m.finish_chat_mode_change().is_empty());
        assert!(!m.mode_change_pending());
    }
}

#[test]
fn daemon_mode_acknowledgements_update_ui_without_overwriting_a_local_choice() {
    let mut m = model();
    m.accept_chat(snapshot(Mode::Chat, ProductRunOperationState::Running));
    assert_eq!(m.chat.mode, Mode::Chat);
    m.chat.mode = Mode::Plan;
    m.accept_chat(snapshot(Mode::Chat, ProductRunOperationState::Running));
    assert_eq!(m.chat.mode, Mode::Plan);
}

#[test]
fn disconnect_abandons_automatic_followup_and_retains_the_draft() {
    let mut m = model();
    m.chat_mode_command(Mode::Chat, "continue coding");
    m.update(crate::action::Action::Disconnected("transport interrupted".into()));
    assert!(!m.mode_change_pending());
    assert_eq!(m.chat.buffer, "continue coding");
    m.accept_chat(snapshot(Mode::Review, ProductRunOperationState::Cancelled));
    assert!(m.finish_chat_mode_change().is_empty());
}

#[test]
fn local_mode_selection_cannot_queue_a_message_into_the_old_review_execution() {
    let mut m = model();
    m.chat.mode = Mode::Chat;
    let effects = m.send_chat_message("continue coding".into());
    assert!(effects.iter().any(|effect|matches!(effect,Effect::Send(AppMessage::Request(r)) if matches!(r.payload(),AppRequestPayload::ControlProductRun(c) if c.action()==Control::Cancel))));
    assert!(m.mode_change_pending());
    assert!(m.send_chat_message("a newer draft".into()).is_empty());
}
