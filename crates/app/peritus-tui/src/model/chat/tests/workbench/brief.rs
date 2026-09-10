use super::*;
use peritus_app_protocol::{
    ControlOperationId, WorkbenchBrief, WorkbenchBriefEntry, WorkbenchBriefField as F,
    WorkbenchBriefProposal, WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputRow,
    WorkbenchInputSelection, WorkbenchInputState as S, WorkbenchInputText, WorkbenchIntent,
    WorkbenchInvocationId, WorkbenchQuery,
};

fn opened() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchBrief).expect("feature"),
    );
    model.chat.workbench.selected = Some(WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([67; 16]).expect("id"),
        model.product.as_ref().expect("product").launch.workspace_id(),
    ));
    model
}
fn brief(query: WorkbenchQuery) -> WorkbenchBrief {
    let source = WorkbenchInputRow::new(
        WorkbenchInputSelection::new(WorkbenchInputId::new([68; 16]).expect("id"), 2)
            .expect("revision"),
        WorkbenchInputText::new("Exact user requirement λ".to_owned()).expect("text"),
        S::Held,
        WorkbenchInputOrder::new(Vec::new()).expect("order"),
    )
    .expect("row");
    let proposal_text = WorkbenchInputText::new("Agent proposed exact criterion".to_owned())
        .expect("proposal text");
    let proposal = WorkbenchBriefProposal::new(
        ControlOperationId::new([69; 16]).expect("proposal"),
        WorkbenchInvocationId::new([70; 16]).expect("invocation"),
        peritus_codec::sha256(proposal_text.as_str().as_bytes()),
        proposal_text,
    )
    .expect("proposal");
    WorkbenchBrief::with_sources(
        query,
        9,
        vec![WorkbenchBriefEntry::new(F::Objective, source).expect("entry")],
        vec![proposal],
        Vec::new(),
        0,
    )
    .expect("brief")
}

#[test]
fn exact_agent_proposal_requires_inspection_and_explicit_field_acceptance() {
    let mut model = opened();
    model.chat.buffer = "/brief accept acceptance 45454545454545454545454545454545".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(!model.chat.buffer.is_empty());
    inspect(&mut model);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/brief accept acceptance 45454545454545454545454545454545".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("command") };
    assert_eq!(command.expected_revision(), 9);
    assert!(matches!(
        command.intent(),
        WorkbenchIntent::AcceptBriefProposal { field: F::Acceptance, proposal, digest }
            if proposal.as_bytes() == &[69; 16]
                && *digest == peritus_codec::sha256(b"Agent proposed exact criterion")
    ));
    assert_eq!(model.chat.buffer, "/brief accept acceptance 45454545454545454545454545454545");
}
fn inspect(model: &mut AppModel) {
    model.chat.buffer = "/brief".to_owned();
    let sent = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchBrief(query) = sent.payload() else { panic!("query") };
    respond(model, &sent, AppResponsePayload::WorkbenchBrief(brief(*query)));
}

#[test]
fn brief_requires_capability_and_inspection_and_rejects_bad_fields_without_losing_drafts() {
    let mut unsupported = enabled_model();
    unsupported.chat.buffer = "/brief".to_owned();
    assert!(key(&mut unsupported, KeyCode::Enter).is_empty());
    assert_eq!(unsupported.chat.buffer, "/brief");
    let mut model = opened();
    for text in ["/brief objective uninspected", "/brief objective", "/brief invented text"] {
        model.chat.buffer = text.to_owned();
        assert!(key(&mut model, KeyCode::Enter).is_empty());
        assert_eq!(model.chat.buffer, text);
    }
    inspect(&mut model);
    assert!(model.chat.run_id.is_none());
    assert_eq!(model.chat.buffer, "/brief");
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/brief acceptance Confirm restart".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("command") };
    assert_eq!(command.expected_revision(), 9);
    assert!(
        matches!(command.intent(), WorkbenchIntent::SetBrief { field: F::Acceptance, text } if text.as_str() == "Confirm restart")
    );
    assert_eq!(model.chat.buffer, "/brief acceptance Confirm restart");
    let refresh = request(&respond(&mut model, &sent, receipt(command)));
    assert!(matches!(refresh.payload(), AppRequestPayload::QueryWorkbenchBrief(_)));
    assert!(model.chat.buffer.is_empty());
}

#[test]
fn brief_stale_rejection_invalidates_inspection_and_late_receipts_preserve_new_drafts() {
    let mut model = opened();
    inspect(&mut model);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/brief objective Revised requirement".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    respond(
        &mut model,
        &sent,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert!(model.chat.workbench.brief.is_none());
    key(&mut model, KeyCode::Esc);
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.buffer, "/brief objective Revised requirement");
    inspect(&mut model);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/brief constraints No uploads".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("command") };
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "new draft λ".to_owned();
    model.chat.cursor = 3;
    respond(&mut model, &sent, receipt(command));
    assert_eq!(model.chat.buffer, "new draft λ");
    assert_eq!(model.chat.cursor, 3);
    assert!(!model.chat.workbench.open);
}

#[test]
fn brief_panel_keeps_composer_and_escape_visible_and_labels_exact_source_state() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = opened();
    inspect(&mut model);
    model.chat.buffer = "retained draft λ".to_owned();
    for (width, height) in [(40, 12), (80, 24), (120, 38)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for expected in ["Task brief", "Esc back", "Message Peritus", "retained draft λ"] {
            assert!(text.contains(expected), "{width}x{height}: {expected}");
        }
        if height >= 24 {
            for expected in ["held", "content rev 2", "Exact user requirement λ"] {
                assert!(text.contains(expected), "{text}");
            }
        }
    }
}
