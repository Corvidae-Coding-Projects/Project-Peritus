use super::*;
use crate::{ControlOperationId, WorkbenchInputText};
use peritus_codec::sha256;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

#[test]
fn parser_binds_file_and_hunks_to_content_not_line_numbers() {
    let raw = "metadata\n\ndiff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,3 @@\n same\n-old\n+new\n+more\n";
    let run = RunId::new([1; 16]).expect("run");
    let workspace = WorkspaceId::new([2; 16]).expect("workspace");
    let candidate = Sha256Digest::new([3; 32]);
    let (digest, files) = parse_workbench_diff(run, workspace, candidate, raw).expect("parse");
    assert_eq!(digest, sha256(raw.as_bytes()));
    assert_eq!(files.len(), 1);
    let file = &files[0];
    assert_eq!(file.anchor().path(), "src/lib.rs");
    assert_eq!(file.hunks().len(), 1);
    let hunk = &file.hunks()[0];
    assert_eq!(hunk.anchor().range(), WorkbenchReviewRange::hunk(1, 2, 1, 3).unwrap());
    assert_ne!(hunk.anchor().before_blob_digest(), hunk.anchor().after_blob_digest());

    let changed = raw.replace("+more", "+different");
    let (_, changed_files) =
        parse_workbench_diff(run, workspace, candidate, &changed).expect("parse changed");
    assert_ne!(hunk.anchor(), changed_files[0].hunks()[0].anchor());
}

#[test]
fn parser_rejects_traversal_and_malformed_line_only_ranges() {
    let raw = "diff --git a/../secret b/../secret\n--- a/../secret\n+++ b/../secret\n@@ -1 +1 @@\n-old\n+new\n";
    assert!(
        parse_workbench_diff(
            RunId::new([1; 16]).unwrap(),
            WorkspaceId::new([2; 16]).unwrap(),
            Sha256Digest::new([3; 32]),
            raw,
        )
        .is_err()
    );
    assert!(WorkbenchReviewRange::hunk(0, 1, 4, 1).is_err());
}

#[test]
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
    let response = AppMessage::Response(AppResponseEnvelope::new(
        context,
        request_id,
        correlation,
        AppResponsePayload::WorkbenchReview(page),
    ));
    let bytes = encode_app_message(&response, AppProtocolLimits::PRODUCTION).expect("encode");
    assert_eq!(
        decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).expect("decode"),
        response
    );
}
