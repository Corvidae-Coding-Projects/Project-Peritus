use super::*;

fn query(view: WorkbenchContextView, revision: u64, offset: u32) -> WorkbenchContextQuery {
    WorkbenchContextQuery::new(
        WorkbenchQuery::new(
            crate::ConversationId::new([1; 16]).expect("conversation"),
            peritus_types::WorkspaceId::new([2; 16]).expect("workspace"),
        ),
        revision,
        offset,
        view,
    )
    .expect("query")
}
fn input() -> WorkbenchContextRow {
    WorkbenchContextRow::new(
        WorkbenchContextSource::Input(
            WorkbenchInputSelection::new(crate::WorkbenchInputId::new([3; 16]).expect("input"), 1)
                .expect("selection"),
        ),
        Sha256Digest::new([4; 32]),
        10,
        WorkbenchContextDisposition::Eligible,
    )
    .expect("row")
}

#[test]
fn context_pages_reject_false_seals_wrong_views_gaps_and_duplicate_sources() {
    let id = WorkbenchInvocationId::new([5; 16]).expect("invocation");
    let seal =
        WorkbenchContextSeal::new(id, Sha256Digest::new([6; 32]), Sha256Digest::new([7; 32]), 1);
    let next = query(WorkbenchContextView::Next, 1, 0);
    assert!(WorkbenchContextQuery::new(next.query(), 0, 1, next.view()).is_err());
    assert!(WorkbenchContextQuery::new(next.query(), 1, 8193, next.view()).is_err());
    assert!(WorkbenchContextPage::new(query(next.view(), 0, 0), 0, None, Vec::new()).is_err());
    assert!(WorkbenchContextPage::new(next, 2, None, vec![input()]).is_err());
    assert!(WorkbenchContextPage::new(next, 2, None, vec![input(), input()]).is_err());
    assert!(WorkbenchContextPage::new(next, 1, Some(seal), vec![input()]).is_err());
    assert!(
        WorkbenchContextPage::new(
            query(WorkbenchContextView::History, 1, 0),
            1,
            None,
            vec![input()]
        )
        .is_err()
    );
    assert!(
        WorkbenchContextPage::new(
            query(WorkbenchContextView::Invocation(id), 1, 0),
            0,
            None,
            Vec::new()
        )
        .is_err()
    );
    assert!(
        WorkbenchContextPage::new(
            query(WorkbenchContextView::Invocation(id), 1, 0),
            1,
            Some(seal),
            vec![input()]
        )
        .is_err()
    );
    assert!(
        WorkbenchContextRow::new(input().source(), input().digest(), 0, input().disposition())
            .is_err()
    );
    assert!(
        WorkbenchContextRow::new(
            input().source(),
            input().digest(),
            64 * 1024 * 1024 + 1,
            input().disposition()
        )
        .is_err()
    );
    assert!(
        WorkbenchContextRow::new(
            WorkbenchContextSource::Message { ordinal: 0, role: WorkbenchMessageRole::Assistant },
            input().digest(),
            10,
            WorkbenchContextDisposition::Eligible
        )
        .is_err()
    );
}

#[test]
fn context_fixtures_roundtrip_every_source_role_view_and_disposition_without_content() {
    use crate::{
        AppMessage, AppProtocolLimits, AppRequestPayload, WellKnownProtocolFeature,
        decode_app_message, encode_app_message,
    };
    let cases = crate::schema::generated_fixture_cases().expect("fixtures");
    let mut seen = 0;
    for case in cases.iter().filter(|case| case.case.contains("context-")) {
        let message =
            decode_app_message(&case.payload, AppProtocolLimits::PRODUCTION).expect("decode");
        assert_eq!(
            encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("encode"),
            case.payload
        );
        if let AppMessage::Request(request) = &message {
            assert!(matches!(request.payload(), AppRequestPayload::QueryWorkbenchContext(_)));
            assert_eq!(
                request.payload().required_workbench_feature(),
                Some(WellKnownProtocolFeature::WorkbenchContext)
            );
        }
        seen += 1;
    }
    assert_eq!(seen, 6);
}
