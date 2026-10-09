use super::super::*;
use crate::{
    AppMessage, AppProtocolLimits, AppResponseEnvelope, AppResponsePayload, CorrelationId,
    ProtocolContext, ProtocolId, ProtocolVersion, RequestId, WorkbenchQuery, decode_app_message,
    encode_app_message,
};
use peritus_codec::sha256;
use peritus_types::{RunId, SessionId, Sha256Digest, WorkspaceId};

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
fn parser_keeps_added_content_that_looks_like_a_path_header_inside_the_hunk() {
    let raw = "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -0,0 +1 @@\n+++ b/other.rs\n";
    let run = RunId::new([8; 16]).unwrap();
    let workspace = WorkspaceId::new([9; 16]).unwrap();
    let candidate = Sha256Digest::new([10; 32]);
    let (diff, files) =
        parse_workbench_diff(run, workspace, candidate, raw).expect("legacy diff parse");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].hunks().len(), 1);
    assert_eq!(files[0].hunks()[0].lines()[0].text(), "++ b/other.rs");

    let query = WorkbenchQuery::new(crate::ConversationId::new([11; 16]).unwrap(), workspace);
    let page = parse_workbench_diff_page(
        WorkbenchReviewDiffQuery::new(query, run, 12, 0, 0, 0),
        candidate,
        raw,
    )
    .expect("streamed diff parse");
    assert_eq!(page.diff_digest(), diff);
    assert_eq!(page.total_files(), 1);
    assert_eq!(page.total_hunks(), 1);
    assert_eq!(page.lines()[0].preview(), "++ b/other.rs");
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
fn expanding_control_sanitization_still_marks_truncated_source_in_pages_and_wire() {
    use crate::{
        AppMessage, AppProtocolLimits, AppResponseEnvelope, AppResponsePayload, CorrelationId,
        ProtocolContext, ProtocolId, ProtocolVersion, RequestId, decode_app_message,
        encode_app_message,
    };
    use peritus_types::SessionId;

    let run = RunId::new([85; 16]).unwrap();
    let workspace = WorkspaceId::new([86; 16]).unwrap();
    let query = WorkbenchQuery::new(crate::ConversationId::new([87; 16]).unwrap(), workspace);
    let candidate = Sha256Digest::new([88; 32]);
    let controls = "\u{1b}".repeat(500);
    let raw = format!(
        "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -0,0 +1 @@\n+{controls}\n"
    );
    let page = parse_workbench_diff_page(
        WorkbenchReviewDiffQuery::new(query, run, 3, 0, 0, 0),
        candidate,
        &raw,
    )
    .expect("control-containing source remains pageable");
    let line = &page.lines()[0];
    assert!(line.is_truncated());
    assert_eq!(line.raw_length() as usize, controls.len());
    assert!(line.preview().ends_with('…'));
    assert!(!line.preview().contains('\u{1b}'));

    let response = AppMessage::Response(AppResponseEnvelope::new(
        ProtocolContext::new(
            ProtocolId::new([89; 16]).unwrap(),
            ProtocolVersion::new(1, 0).unwrap(),
            SessionId::new([90; 16]).unwrap(),
        ),
        RequestId::new([91; 16]).unwrap(),
        CorrelationId::new([92; 16]).unwrap(),
        AppResponsePayload::WorkbenchReviewDiff(page),
    ));
    let bytes = encode_app_message(&response, AppProtocolLimits::PRODUCTION).unwrap();
    assert_eq!(decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).unwrap(), response);
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
fn streaming_page_anchors_match_legacy_parser_and_reaches_beyond_legacy_caps() {
    let run = RunId::new([51; 16]).expect("run");
    let workspace = WorkspaceId::new([52; 16]).expect("workspace");
    let candidate = Sha256Digest::new([53; 32]);
    let query =
        WorkbenchQuery::new(crate::ConversationId::new([54; 16]).expect("conversation"), workspace);
    let small = "preamble\r\ndiff --git a/a.rs b/a.rs\r\nindex 1..2\r\n--- a/a.rs\r\n+++ b/a.rs\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n\\ No newline\r\ndiff --git a/b.rs b/b.rs\r\n--- a/b.rs\r\n+++ b/b.rs\r\n@@ -3 +3 @@\r\n same";
    let (_, files) = parse_workbench_diff(run, workspace, candidate, small).expect("legacy parse");
    let (page, matches) = parse_workbench_diff_page_with_anchors(
        WorkbenchReviewDiffQuery::new(query, run, 7, 1, 0, 0),
        candidate,
        small,
        &[files[1].anchor().clone(), files[1].hunks()[0].anchor().clone()],
    )
    .expect("bounded parse");
    assert_eq!(page.file_anchor(), files[1].anchor());
    assert_eq!(page.hunk().expect("hunk").anchor(), files[1].hunks()[0].anchor());
    assert_eq!(page.total_files(), 2);
    assert_eq!(page.total_hunks(), 2);
    assert_eq!(page.total_lines(), 4);
    assert_eq!(matches, [true, true]);

    let mut large =
        String::from("diff --git a/large.rs b/large.rs\n--- a/large.rs\n+++ b/large.rs\n");
    for _ in 0..4100 {
        large.push_str("@@ -1,9 +1,9 @@\n");
        large.push_str(" same\n same\n same\n same\n same\n same\n same\n same\n same\n");
    }
    let cursor = WorkbenchReviewDiffQuery::new(query, run, 8, 0, 4099, 0);
    let (later, _) = parse_workbench_diff_page_with_anchors(cursor, candidate, &large, &[])
        .expect("later page beyond legacy caps");
    assert_eq!(later.total_hunks(), 4100);
    assert_eq!(later.total_lines(), 36_900);
    assert_eq!(later.hunk().expect("selected hunk").lines().len(), 9);
    assert!(later.next().is_none());

    let mut huge_hunk = String::from(
        "diff --git a/huge.rs b/huge.rs\n--- a/huge.rs\n+++ b/huge.rs\n@@ -1,33000 +1,33000 @@\n",
    );
    for _ in 0..33_000 {
        huge_hunk.push_str(" same\n");
    }
    let cursor = WorkbenchReviewDiffQuery::new(query, run, 9, 0, 0, 32_768);
    let (range, _) = parse_workbench_diff_page_with_anchors(cursor, candidate, &huge_hunk, &[])
        .expect("bounded line page beyond legacy line cap");
    assert_eq!(range.total_lines(), 33_000);
    assert_eq!(range.lines().len(), 32);
    assert_eq!(range.next().expect("line continuation").line_offset(), 32_800);
}

#[test]
fn streamed_oversized_line_keeps_bounded_preview_and_exact_raw_range() {
    let run = RunId::new([61; 16]).unwrap();
    let workspace = WorkspaceId::new([62; 16]).unwrap();
    let query = WorkbenchQuery::new(crate::ConversationId::new([63; 16]).unwrap(), workspace);
    let candidate = Sha256Digest::new([64; 32]);
    let long = "x".repeat(40_000);
    let raw = format!(
        "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -0,0 +1 @@\n+{long}\n"
    );
    let page = parse_workbench_diff_page(
        WorkbenchReviewDiffQuery::new(query, run, 14, 0, 0, 0),
        candidate,
        &raw,
    )
    .expect("bounded parser accepts an oversized source line");
    let line = &page.lines()[0];
    assert!(line.is_truncated());
    assert_eq!(line.preview().len(), 1027);
    assert_eq!(line.raw_offset() as usize, raw.find(&long).expect("raw source line"));
    assert_eq!(line.raw_length() as usize, long.len());

    let response = AppMessage::Response(AppResponseEnvelope::new(
        ProtocolContext::new(
            ProtocolId::new([65; 16]).unwrap(),
            ProtocolVersion::new(1, 0).unwrap(),
            SessionId::new([66; 16]).unwrap(),
        ),
        RequestId::new([67; 16]).unwrap(),
        CorrelationId::new([68; 16]).unwrap(),
        AppResponsePayload::WorkbenchReviewDiff(page),
    ));
    let bytes = encode_app_message(&response, AppProtocolLimits::PRODUCTION)
        .expect("oversized bounded preview encodes");
    assert_eq!(
        decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).expect("decode"),
        response,
    );
}

#[test]
fn streamed_line_budget_never_skips_a_line_before_its_continuation() {
    let run = RunId::new([81; 16]).unwrap();
    let workspace = WorkspaceId::new([82; 16]).unwrap();
    let query = WorkbenchQuery::new(crate::ConversationId::new([83; 16]).unwrap(), workspace);
    let candidate = Sha256Digest::new([84; 32]);
    let long = "x".repeat(2_000);
    let mut raw = String::from(
        "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -0,0 +1,33 @@\n",
    );
    for _ in 0..32 {
        raw.push('+');
        raw.push_str(&long);
        raw.push('\n');
    }
    raw.push_str("+tail\n");
    let first = parse_workbench_diff_page(
        WorkbenchReviewDiffQuery::new(query, run, 16, 0, 0, 0),
        candidate,
        &raw,
    )
    .expect("first contiguous line page");
    assert_eq!(first.lines().len(), 31);
    let next = first.next().expect("continuation after retained prefix");
    assert_eq!(next.line_offset(), 31);
    let second = parse_workbench_diff_page(next, candidate, &raw).expect("second line page");
    let expected = raw
        .match_indices(&format!("+{long}\n"))
        .nth(31)
        .map(|(offset, _)| offset + 1)
        .expect("32nd long source line");
    assert_eq!(second.lines()[0].raw_offset() as usize, expected);
    assert_eq!(second.lines()[0].raw_length() as usize, long.len());
}

#[test]
fn streamed_page_rejects_an_oversized_later_hunk_header_up_front() {
    let run = RunId::new([71; 16]).unwrap();
    let workspace = WorkspaceId::new([72; 16]).unwrap();
    let query = WorkbenchQuery::new(crate::ConversationId::new([73; 16]).unwrap(), workspace);
    let candidate = Sha256Digest::new([74; 32]);
    let long_context = "x".repeat(crate::MAX_PRODUCT_DETAIL_BYTES + 1);
    let raw = format!(
        "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new\n@@ -2 +2 @@ {long_context}\n-old\n+new\n"
    );
    assert!(
        parse_workbench_diff_page(
            WorkbenchReviewDiffQuery::new(query, run, 15, 0, 0, 0),
            candidate,
            &raw,
        )
        .is_err()
    );

    let empty_hunk = "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new\n@@ -2 +2 @@\n";
    assert!(
        parse_workbench_diff_page(
            WorkbenchReviewDiffQuery::new(query, run, 15, 0, 0, 0),
            candidate,
            empty_hunk,
        )
        .is_err()
    );
}

#[test]
fn streamed_diff_pages_continue_across_more_files_than_the_legacy_review_page_allows() {
    use std::fmt::Write as _;

    let mut raw = String::new();
    for index in 0..513 {
        writeln!(raw, "diff --git a/file-{index:03}.rs b/file-{index:03}.rs").unwrap();
        writeln!(raw, "--- a/file-{index:03}.rs").unwrap();
        writeln!(raw, "+++ b/file-{index:03}.rs").unwrap();
        raw.push_str("@@ -1 +1 @@\n-old\n+new\n");
    }
    let run = RunId::new([31; 16]).unwrap();
    let workspace = WorkspaceId::new([32; 16]).unwrap();
    let query = WorkbenchQuery::new(crate::ConversationId::new([33; 16]).unwrap(), workspace);
    let candidate = Sha256Digest::new([34; 32]);
    let (diff, files) = parse_workbench_diff(run, workspace, candidate, &raw).unwrap();
    assert_eq!(files.len(), 513);
    assert!(
        WorkbenchReviewPage::new(
            WorkbenchReviewQuery::new(query, run, 9, 0),
            candidate,
            diff,
            files,
            Vec::new(),
            0,
            Vec::new(),
        )
        .is_err()
    );

    let page = parse_workbench_diff_page(
        WorkbenchReviewDiffQuery::new(query, run, 9, 511, 0, 0),
        candidate,
        &raw,
    )
    .unwrap();
    assert_eq!(page.total_files(), 513);
    assert_eq!(page.total_hunks(), 513);
    assert_eq!(page.total_lines(), 1026);
    assert_eq!(page.file_anchor().path(), "file-511.rs");
    let next = page.next().expect("file continuation");
    assert_eq!((next.file_offset(), next.hunk_offset(), next.line_offset()), (512, 0, 0));

    let last = parse_workbench_diff_page(next, candidate, &raw).unwrap();
    assert_eq!(last.file_anchor().path(), "file-512.rs");
    assert_eq!(last.total_files(), 513);
    assert!(last.next().is_none());
}

#[test]
fn streamed_diff_page_keeps_preamble_bytes_in_raw_line_offsets() {
    let preamble = "metadata preamble\n";
    let raw = format!(
        "{preamble}diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new\n"
    );
    let run = RunId::new([41; 16]).unwrap();
    let workspace = WorkspaceId::new([42; 16]).unwrap();
    let query = WorkbenchQuery::new(crate::ConversationId::new([43; 16]).unwrap(), workspace);
    let candidate = Sha256Digest::new([44; 32]);
    let (_, files) = parse_workbench_diff(run, workspace, candidate, &raw).unwrap();
    let legacy_line = &files[0].hunks()[0].lines()[0];
    assert_eq!(
        usize::try_from(legacy_line.raw_offset()).unwrap(),
        raw.find("-old").expect("line prefix") + 1,
    );
    let page = parse_workbench_diff_page(
        WorkbenchReviewDiffQuery::new(query, run, 9, 0, 0, 0),
        candidate,
        &raw,
    )
    .unwrap();
    let line = &page.lines()[0];
    assert_eq!(
        usize::try_from(line.raw_offset()).unwrap(),
        raw.find("-old").expect("line prefix") + 1,
    );
}
