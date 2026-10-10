use super::*;
use crate::{ControlOperationId, WorkbenchInputText};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

mod pages;

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the legacy review request fixture is a single round-trip contract"
)]
fn tag_80_page_and_tag_50_comment_round_trip_canonically() {
    use crate::{
        AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
        AppResponsePayload, ConversationId, CorrelationId, ProtocolContext, ProtocolId,
        ProtocolVersion, RequestId, WorkbenchCommand, WorkbenchIntent, WorkbenchQuery,
        decode_app_message, encode_app_message,
    };
    use peritus_types::SessionId;
    let run = RunId::new([11; 16]).expect("run");
    let workspace = WorkspaceId::new([12; 16]).expect("workspace");
    let query =
        WorkbenchQuery::new(ConversationId::new([13; 16]).expect("conversation"), workspace);
    let candidate = Sha256Digest::new([14; 32]);
    let raw = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let (diff, files) = parse_workbench_diff(run, workspace, candidate, raw).expect("diff");
    let anchor = files[0].hunks()[0].anchor().clone();
    let command = WorkbenchCommand::new(
        ControlOperationId::new([15; 16]).expect("operation"),
        query,
        4,
        WorkbenchIntent::AddReview {
            anchor,
            feedback: WorkbenchReviewFeedback::RequestRevision,
            message: WorkbenchInputText::new("Make the new behavior explicit.".to_owned())
                .expect("message"),
        },
    );
    let context = ProtocolContext::new(
        ProtocolId::new([16; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        SessionId::new([17; 16]).expect("session"),
    );
    let request_id = RequestId::new([18; 16]).expect("request");
    let correlation = CorrelationId::new([19; 16]).expect("correlation");
    let request = AppMessage::Request(
        AppRequestEnvelope::new(
            context,
            request_id,
            correlation,
            AppRequestPayload::WorkbenchCommand(command),
        )
        .expect("request"),
    );
    let bytes = encode_app_message(&request, AppProtocolLimits::PRODUCTION).expect("encode");
    assert_eq!(decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).expect("decode"), request);

    let page = WorkbenchReviewPage::new(
        WorkbenchReviewQuery::new(query, run, 4, 0),
        candidate,
        diff,
        files,
        Vec::new(),
        0,
        vec![
            WorkbenchReviewEvidence::new(
                WorkbenchReviewEvidenceKind::Checks,
                WorkbenchReviewEvidenceState::Missing,
                None,
                None,
                None,
            )
            .expect("evidence"),
        ],
    )
    .expect("page");
    let diff_query = WorkbenchReviewDiffQuery::new(query, run, 4, 0, 0, 0);
    let summary = WorkbenchReviewSummary::new(
        page.query(),
        candidate,
        diff,
        1,
        1,
        2,
        true,
        Vec::new(),
        0,
        page.evidence().to_vec(),
    )
    .expect("review summary");
    let summary_request = AppMessage::Request(
        AppRequestEnvelope::new(
            context,
            RequestId::new([24; 16]).expect("summary request"),
            CorrelationId::new([25; 16]).expect("summary correlation"),
            AppRequestPayload::QueryWorkbenchReviewSummary(page.query()),
        )
        .expect("summary request envelope"),
    );
    let summary_request_bytes = encode_app_message(&summary_request, AppProtocolLimits::PRODUCTION)
        .expect("encode summary request");
    assert_eq!(
        decode_app_message(&summary_request_bytes, AppProtocolLimits::PRODUCTION)
            .expect("decode summary request"),
        summary_request
    );
    let summary_response = AppMessage::Response(AppResponseEnvelope::new(
        context,
        RequestId::new([24; 16]).expect("summary request"),
        CorrelationId::new([25; 16]).expect("summary correlation"),
        AppResponsePayload::WorkbenchReviewSummary(summary),
    ));
    let summary_response_bytes =
        encode_app_message(&summary_response, AppProtocolLimits::PRODUCTION)
            .expect("encode summary");
    assert_eq!(
        decode_app_message(&summary_response_bytes, AppProtocolLimits::PRODUCTION)
            .expect("decode summary"),
        summary_response
    );
    let diff_page = WorkbenchReviewDiffPage::project(diff_query, candidate, diff, page.files())
        .expect("diff page");
    let diff_request = AppMessage::Request(
        AppRequestEnvelope::new(
            context,
            RequestId::new([20; 16]).expect("diff request"),
            CorrelationId::new([21; 16]).expect("diff correlation"),
            AppRequestPayload::QueryWorkbenchReviewDiff(diff_query),
        )
        .expect("diff request envelope"),
    );
    let diff_request_bytes = encode_app_message(&diff_request, AppProtocolLimits::PRODUCTION)
        .expect("encode diff request");
    assert_eq!(
        decode_app_message(&diff_request_bytes, AppProtocolLimits::PRODUCTION)
            .expect("decode diff request"),
        diff_request
    );
    let diff_response = AppMessage::Response(AppResponseEnvelope::new(
        context,
        RequestId::new([20; 16]).expect("diff request"),
        CorrelationId::new([21; 16]).expect("diff correlation"),
        AppResponsePayload::WorkbenchReviewDiff(diff_page),
    ));
    let diff_response_bytes = encode_app_message(&diff_response, AppProtocolLimits::PRODUCTION)
        .expect("encode diff page");
    assert_eq!(
        decode_app_message(&diff_response_bytes, AppProtocolLimits::PRODUCTION)
            .expect("decode diff page"),
        diff_response
    );
    let raw_query = WorkbenchReviewDiffBytesQuery::new(query, run, 4, candidate, diff, 0, 16);
    let raw_request = AppMessage::Request(
        AppRequestEnvelope::new(
            context,
            RequestId::new([22; 16]).expect("raw request"),
            CorrelationId::new([23; 16]).expect("raw correlation"),
            AppRequestPayload::QueryWorkbenchReviewDiffBytes(raw_query),
        )
        .expect("raw request envelope"),
    );
    let raw_request_bytes = encode_app_message(&raw_request, AppProtocolLimits::PRODUCTION)
        .expect("encode raw request");
    assert_eq!(
        decode_app_message(&raw_request_bytes, AppProtocolLimits::PRODUCTION)
            .expect("decode raw request"),
        raw_request
    );
    let raw_response = AppMessage::Response(AppResponseEnvelope::new(
        context,
        RequestId::new([22; 16]).expect("raw request"),
        CorrelationId::new([23; 16]).expect("raw correlation"),
        AppResponsePayload::WorkbenchReviewDiffBytes(
            WorkbenchReviewDiffBytes::new(raw_query, 64, b"diff --git a/fil".to_vec())
                .expect("raw bytes response"),
        ),
    ));
    let raw_response_bytes =
        encode_app_message(&raw_response, AppProtocolLimits::PRODUCTION).expect("encode raw bytes");
    assert_eq!(
        decode_app_message(&raw_response_bytes, AppProtocolLimits::PRODUCTION)
            .expect("decode raw bytes"),
        raw_response
    );
    let expected_query = page.query();
    let expected_candidate = page.candidate_digest();
    let expected_diff = page.diff_digest();
    let expected_files = page.files().len();
    let response = AppMessage::Response(AppResponseEnvelope::new(
        context,
        request_id,
        correlation,
        AppResponsePayload::WorkbenchReview(page),
    ));
    let bytes = encode_app_message(&response, AppProtocolLimits::PRODUCTION).expect("encode");
    let decoded = decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).expect("decode");
    let AppMessage::Response(decoded) = decoded else { panic!("tag 80 response") };
    let AppResponsePayload::WorkbenchReview(decoded_page) = decoded.payload() else {
        panic!("tag 80 review page")
    };
    assert_eq!(decoded_page.query(), expected_query);
    assert_eq!(decoded_page.candidate_digest(), expected_candidate);
    assert_eq!(decoded_page.diff_digest(), expected_diff);
    assert_eq!(decoded_page.files().len(), expected_files);
}
