use super::*;
use crate::{WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputSelection, WorkbenchInputText};

fn query() -> WorkbenchQuery {
    WorkbenchQuery::new(
        crate::ConversationId::new([1; 16]).expect("conversation"),
        peritus_types::WorkspaceId::new([2; 16]).expect("workspace"),
    )
}
fn source(id: u8, state: WorkbenchInputState) -> WorkbenchInputRow {
    WorkbenchInputRow::new(
        WorkbenchInputSelection::new(WorkbenchInputId::new([id; 16]).expect("id"), 1)
            .expect("revision"),
        WorkbenchInputText::new("Explicitly confirmed".to_owned()).expect("text"),
        state,
        WorkbenchInputOrder::new(Vec::new()).expect("order"),
    )
    .expect("row")
}
#[test]
fn brief_rejects_uncommitted_duplicate_noncanonical_and_superseded_bindings() {
    let objective = WorkbenchBriefEntry::new(
        WorkbenchBriefField::Objective,
        source(3, WorkbenchInputState::Held),
    )
    .expect("entry");
    let constraints = WorkbenchBriefEntry::new(
        WorkbenchBriefField::Constraints,
        source(4, WorkbenchInputState::Withdrawn),
    )
    .expect("entry");
    assert!(WorkbenchBrief::new(query(), 0, Vec::new()).is_err());
    assert!(WorkbenchBrief::new(query(), 1, vec![objective.clone(), objective.clone()]).is_err());
    assert!(WorkbenchBrief::new(query(), 1, vec![constraints.clone(), objective.clone()]).is_err());
    let reused = WorkbenchBriefEntry::new(
        WorkbenchBriefField::Constraints,
        source(3, WorkbenchInputState::Held),
    )
    .expect("entry");
    assert!(WorkbenchBrief::new(query(), 1, vec![objective.clone(), reused]).is_err());
    assert!(
        WorkbenchBriefEntry::new(
            WorkbenchBriefField::Objective,
            source(3, WorkbenchInputState::Superseded)
        )
        .is_err()
    );
    assert!(WorkbenchBrief::new(query(), 1, vec![objective, constraints]).is_ok());
}

#[test]
fn brief_separates_agent_proposals_and_observed_facts_from_confirmed_fields() {
    let text = WorkbenchInputText::new("Agent-proposed criterion".to_owned()).expect("text");
    let digest = peritus_codec::sha256(text.as_str().as_bytes());
    let proposal = WorkbenchBriefProposal::new(
        ControlOperationId::new([8; 16]).expect("operation"),
        WorkbenchInvocationId::new([7; 16]).expect("invocation"),
        digest,
        text,
    )
    .expect("proposal");
    let observed = WorkbenchBriefObservation::new(
        WorkbenchBriefObservationKind::File,
        ControlOperationId::new([9; 16]).expect("attachment"),
        Some(ControlOperationId::new([10; 16]).expect("version")),
        "src/lib.rs".to_owned(),
        peritus_codec::sha256(b"bytes"),
        5,
        true,
    )
    .expect("observation");
    let brief =
        WorkbenchBrief::with_sources(query(), 1, Vec::new(), vec![proposal], vec![observed], 2)
            .expect("brief");
    assert!(brief.entries().is_empty());
    assert_eq!(brief.proposals()[0].digest(), digest);
    assert_eq!(brief.observations()[0].label(), "src/lib.rs");
    assert_eq!(brief.excluded_proposals(), 2);
}
#[test]
fn brief_fixtures_roundtrip_every_field_and_require_the_independent_feature() {
    use crate::{
        AppMessage, AppProtocolLimits, AppRequestPayload, WellKnownProtocolFeature,
        decode_app_message, encode_app_message,
    };
    let cases = crate::schema::generated_fixture_cases().expect("fixtures");
    let mut count = 0;
    for case in cases.iter().filter(|case| case.case.contains("workbench-brief")) {
        let message =
            decode_app_message(&case.payload, AppProtocolLimits::PRODUCTION).expect("decode");
        assert_eq!(
            encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("encode"),
            case.payload
        );
        if let AppMessage::Request(request) = message {
            assert_eq!(
                request.payload().required_workbench_feature(),
                Some(WellKnownProtocolFeature::WorkbenchBrief)
            );
            if let AppRequestPayload::WorkbenchCommand(command) = request.payload() {
                assert_eq!(
                    AppRequestPayload::QueryWorkbenchReceipt(command.clone())
                        .required_workbench_feature(),
                    Some(WellKnownProtocolFeature::WorkbenchBrief)
                );
            }
        }
        count += 1;
    }
    assert_eq!(count, 6);
}
