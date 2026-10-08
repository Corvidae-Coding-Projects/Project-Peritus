use super::*;
use crate::{ControlOperationId, WorkbenchInputText, WorkbenchQuery};
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
fn parser_decodes_git_c_quoted_paths_with_spaces_and_octets() {
    let raw = "diff --git \"a/space\\040name.rs\" \"b/space\\040name.rs\"\n--- \"a/space\\040name.rs\"\n+++ \"b/space\\040name.rs\"\n@@ -1 +1 @@\n-old\n+new\ndiff --git \"a/caf\\303\\251.rs\" \"b/caf\\303\\251.rs\"\n--- \"a/caf\\303\\251.rs\"\n+++ \"b/caf\\303\\251.rs\"\n@@ -1 +1 @@\n-old\n+new\n";
    let (_, files) = parse_workbench_diff(
        RunId::new([1; 16]).unwrap(),
        WorkspaceId::new([2; 16]).unwrap(),
        Sha256Digest::new([3; 32]),
        raw,
    )
    .expect("quoted Git paths parse");
    assert_eq!(
        files.iter().map(|file| file.anchor().path()).collect::<Vec<_>>(),
        ["space name.rs", "café.rs",]
    );
}

#[test]
fn parser_keeps_oversized_control_line_as_safe_preview_with_exact_raw_range() {
    let source = format!("{}\u{1b}tail", "x".repeat(1_200));
    let raw = format!("diff --git a/file b/file\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n+{source}\n");
    let (_, files) = parse_workbench_diff(
        RunId::new([1; 16]).unwrap(),
        WorkspaceId::new([2; 16]).unwrap(),
        Sha256Digest::new([3; 32]),
        &raw,
    )
    .expect("long lines remain reviewable");
    let line = &files[0].hunks()[0].lines()[0];
    let source_offset = u32::try_from(raw.find(&source).expect("source bytes"))
        .expect("test offset fits protocol range");
    assert_eq!(line.raw_offset(), source_offset);
    assert_eq!(
        line.raw_length(),
        u32::try_from(source.len()).expect("test line fits protocol range")
    );
    assert!(line.is_truncated());
    assert!(!line.text().contains('\u{1b}'));
}

#[test]
fn diff_projection_bounds_lines_and_returns_digest_bound_cursor() {
    use std::fmt::Write as _;

    let body = (0..40).fold(String::new(), |mut output, index| {
        writeln!(output, "+line {index}").expect("write line into string");
        output
    });
    let raw = format!("diff --git a/file b/file\n--- a/file\n+++ b/file\n@@ -0,0 +1,40 @@\n{body}");
    let run = RunId::new([1; 16]).unwrap();
    let workspace = WorkspaceId::new([2; 16]).unwrap();
    let query = WorkbenchQuery::new(crate::ConversationId::new([3; 16]).unwrap(), workspace);
    let candidate = Sha256Digest::new([4; 32]);
    let (diff, files) = parse_workbench_diff(run, workspace, candidate, &raw).unwrap();
    let page = WorkbenchReviewDiffPage::project(
        WorkbenchReviewDiffQuery::new(query, run, 9, 0, 0, 0),
        candidate,
        diff,
        &files,
    )
    .expect("project first bounded page");
    assert_eq!(page.lines().len(), MAX_WORKBENCH_DIFF_PAGE_LINES);
    let next = page.next().expect("continuation");
    assert_eq!(next.file_offset(), 0);
    assert_eq!(next.hunk_offset(), 0);
    assert_eq!(usize::try_from(next.line_offset()).unwrap(), MAX_WORKBENCH_DIFF_PAGE_LINES);
    assert_eq!(page.diff_digest(), diff);
}

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
