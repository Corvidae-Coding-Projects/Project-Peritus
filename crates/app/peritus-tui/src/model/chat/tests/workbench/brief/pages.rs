use super::*;
use peritus_app_protocol::{
    WorkbenchBriefPage, WorkbenchBriefProposalPage, WorkbenchBriefProposalReference,
};

#[test]
fn paged_brief_navigates_exact_text_and_accepts_inspected_revision() {
    let mut model = opened();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchBriefPages)
            .expect("feature"),
    );
    model.chat.buffer = "/brief".into();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchBriefPage(selection) = sent.payload() else {
        panic!("metadata query")
    };
    let text = format!("{}END", "λ".repeat(20_000));
    let reference = WorkbenchBriefProposalReference::new(
        ControlOperationId::new([69; 16]).expect("id"),
        WorkbenchInvocationId::new([70; 16]).expect("invocation"),
        peritus_codec::sha256(text.as_bytes()),
        text.len() as u64,
    )
    .expect("reference");
    let page =
        WorkbenchBriefPage::new(*selection, 9, Vec::new(), 1, vec![reference], 0, Vec::new())
            .expect("metadata page");
    respond(&mut model, &sent, AppResponsePayload::WorkbenchBriefPage(page));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/brief accept acceptance 45454545454545454545454545454545".into();
    assert!(
        key(&mut model, KeyCode::Enter).is_empty(),
        "metadata alone cannot accept an unopened body"
    );
    read_body(&mut model, "/brief show 45454545454545454545454545454545", &text, 0, 32768);
    read_body(&mut model, "/brief text next", &text, 32768, text.len());
    read_body(&mut model, "/brief text previous", &text, 0, 32768);
    model.chat.buffer = "/brief accept acceptance 45454545454545454545454545454545".into();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("accept command")
    };
    assert_eq!(command.expected_revision(), 9);
    assert!(
        matches!(command.intent(), WorkbenchIntent::AcceptBriefProposal { digest, .. } if *digest == reference.digest())
    );
    respond(
        &mut model,
        &sent,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert!(model.chat.workbench.brief.is_none());
    assert!(model.chat.workbench.brief_page.is_none());
    assert!(model.chat.workbench.brief_body.is_none());
    assert!(!model.chat.buffer.is_empty());
}

fn read_body(model: &mut AppModel, command: &str, text: &str, start: usize, end: usize) {
    model.chat.buffer = command.into();
    let sent = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchBriefProposal(selection) = sent.payload() else {
        panic!("body request")
    };
    assert_eq!(selection.offset(), start as u64);
    respond(
        model,
        &sent,
        AppResponsePayload::WorkbenchBriefProposal(
            WorkbenchBriefProposalPage::new(*selection, text[start..end].to_owned())
                .expect("body page"),
        ),
    );
    assert!(model.chat.workbench.open, "requested body page is visible");
    key(model, KeyCode::Esc);
}
