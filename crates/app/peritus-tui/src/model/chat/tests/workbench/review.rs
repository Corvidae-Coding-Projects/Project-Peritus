use super::*;
use peritus_app_protocol::{
    AppErrorCode, ProductRunPhase, WorkbenchInputId, WorkbenchInputSelection, WorkbenchInputText,
    WorkbenchIntent, WorkbenchQuery, WorkbenchReceipt, WorkbenchReviewComment,
    WorkbenchReviewCommentState, WorkbenchReviewEvidence, WorkbenchReviewEvidenceKind,
    WorkbenchReviewEvidenceState, WorkbenchReviewFeedback, WorkbenchReviewPage,
    WorkbenchReviewQuery, parse_workbench_diff,
};
use peritus_types::{RunId, Sha256Digest};

mod navigation;

fn review_model() -> (AppModel, WorkbenchQuery, RunId) {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchReview)
            .expect("review feature"),
    );
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchReviewPages)
            .expect("review pages feature"),
    );
    let query = WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([71; 16]).expect("conversation"),
        model.product.as_ref().expect("product").launch.workspace_id(),
    );
    model.chat.workbench.selected = Some(query);
    let run = RunId::new([72; 16]).expect("run");
    let providers = model.chat_providers().expect("providers");
    let snapshot = peritus_app_protocol::ProductRunSnapshot::new(
        run,
        query.workspace(),
        providers,
        ProductRunPhase::Complete,
        1,
        "fixture".to_owned(),
        "complete".to_owned(),
        raw_diff("old", "new"),
        "checks passed".to_owned(),
        "review passed".to_owned(),
        "done".to_owned(),
        crate::test_support::run_operation(run, ProductRunPhase::Complete),
    )
    .expect("snapshot");
    model.accept_product_run(snapshot);
    model.chat.run_id = Some(run);
    (model, query, run)
}

#[test]
fn review_page_request_is_consumed_and_render_state_uses_bounded_diff_page() {
    let (mut model, query, run) = review_model();
    model.chat.buffer = "/diff".to_owned();
    let old_page_request = request(&key(&mut model, KeyCode::Enter));
    let legacy = page(query, run, 7, 10, "old", "new", &[]);
    let diff = legacy.diff_digest();
    let candidate = legacy.candidate_digest();
    let files = legacy.files().to_vec();
    let effects =
        respond(&mut model, &old_page_request, AppResponsePayload::WorkbenchReview(legacy));
    let diff_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiff(cursor) = diff_request.payload() else {
        panic!("bounded diff page query")
    };
    let projection =
        peritus_app_protocol::WorkbenchReviewDiffPage::project(*cursor, candidate, diff, &files)
            .expect("diff projection");
    respond(&mut model, &diff_request, AppResponsePayload::WorkbenchReviewDiff(projection));
    let review = &model.product.as_ref().expect("product").review;
    assert!(review.diff_page.is_some());
    assert!(review.pending_diff.is_none());
}

#[test]
fn legacy_review_peer_is_not_sent_the_new_diff_page_request() {
    let (mut model, query, run) = review_model();
    model.features.retain(|feature| {
        feature.as_str() != WellKnownProtocolFeature::WorkbenchReviewPages.as_str()
    });
    let requested = WorkbenchReviewQuery::new(query, run, 7, 0);
    let effects = model.accept_review_page(requested, page(query, run, 7, 10, "old", "new", &[]));
    assert!(effects.is_empty());
    let review = &model.product.as_ref().expect("product").review;
    assert!(review.diff_page.is_none());
    assert!(review.pending_diff.is_none());
}

#[test]
fn truncated_review_line_requests_and_retains_the_exact_safe_raw_range() {
    let (mut model, query, run) = review_model();
    model.chat.buffer = "/diff".to_owned();
    let first = request(&key(&mut model, KeyCode::Enter));
    let long = "x".repeat(5_000);
    let legacy = page(query, run, 8, 11, &long, &long, &[]);
    let candidate = legacy.candidate_digest();
    let diff = legacy.diff_digest();
    let files = legacy.files().to_vec();
    let effects = respond(&mut model, &first, AppResponsePayload::WorkbenchReview(legacy));
    let page_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiff(cursor) = page_request.payload() else {
        panic!("diff page request")
    };
    let bounded =
        peritus_app_protocol::WorkbenchReviewDiffPage::project(*cursor, candidate, diff, &files)
            .expect("bounded page");
    let effects =
        respond(&mut model, &page_request, AppResponsePayload::WorkbenchReviewDiff(bounded));
    let bytes_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(raw_query) = bytes_request.payload()
    else {
        panic!("raw range request")
    };
    let raw = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        *raw_query,
        raw_query.offset() + raw_query.maximum_bytes(),
        b"verified source range".to_vec(),
    )
    .expect("raw range");
    respond(&mut model, &bytes_request, AppResponsePayload::WorkbenchReviewDiffBytes(raw));
    let review = &model.product.as_ref().expect("product").review;
    assert_eq!(review.raw_line.as_ref().map(|(offset, _)| *offset), Some(raw_query.offset()));
}

#[test]
fn raw_review_bytes_page_across_large_lines_and_navigate_back_by_line() {
    let (mut model, query, run) = review_model();
    model.chat.buffer = "/diff".to_owned();
    let first = request(&key(&mut model, KeyCode::Enter));
    let long = "x".repeat(70_000);
    let legacy = page(query, run, 9, 12, &long, &long, &[]);
    let candidate = legacy.candidate_digest();
    let diff = legacy.diff_digest();
    let files = legacy.files().to_vec();
    let effects = respond(&mut model, &first, AppResponsePayload::WorkbenchReview(legacy));
    let page_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiff(cursor) = page_request.payload() else {
        panic!("diff page request")
    };
    let bounded =
        peritus_app_protocol::WorkbenchReviewDiffPage::project(*cursor, candidate, diff, &files)
            .expect("bounded page");
    let effects =
        respond(&mut model, &page_request, AppResponsePayload::WorkbenchReviewDiff(bounded));
    let mut bytes_request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(mut raw_query) =
        bytes_request.payload().clone()
    else {
        panic!("first raw range request")
    };
    let lines = model.product.as_ref().unwrap().review.raw_lines.clone();
    assert!(lines.len() >= 2);
    assert!(lines[0].1 > 32 * 1024);
    assert!(lines[1].1 > 32 * 1024);

    let first_line_start = lines[0].0;
    let first_line_end = first_line_start + lines[0].1;
    assert_eq!(raw_query.offset(), first_line_start);
    assert!(key(&mut model, KeyCode::Char(']')).is_empty());
    assert_eq!(model.product.as_ref().unwrap().review.raw_index, 0);
    while raw_query.offset() + raw_query.maximum_bytes() < first_line_end {
        let length = raw_query.maximum_bytes();
        let raw = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
            raw_query,
            first_line_end,
            vec![b'x'; length as usize],
        )
        .expect("first raw chunk");
        let _ =
            respond(&mut model, &bytes_request, AppResponsePayload::WorkbenchReviewDiffBytes(raw));
        bytes_request = request(&key(&mut model, KeyCode::Char(']')));
        let AppRequestPayload::QueryWorkbenchReviewDiffBytes(next) = bytes_request.payload() else {
            panic!("continued raw range request")
        };
        raw_query = *next;
    }
    let remaining = first_line_end - raw_query.offset();
    let raw = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        raw_query,
        first_line_end,
        vec![b'x'; remaining as usize],
    )
    .expect("final first-line chunk");
    let _ = respond(&mut model, &bytes_request, AppResponsePayload::WorkbenchReviewDiffBytes(raw));

    bytes_request = request(&key(&mut model, KeyCode::Char(']')));
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(second_line) = bytes_request.payload()
    else {
        panic!("next truncated line request")
    };
    assert_eq!(second_line.offset(), lines[1].0);
    let raw = peritus_app_protocol::WorkbenchReviewDiffBytes::new(
        *second_line,
        second_line.offset() + second_line.maximum_bytes(),
        vec![b'y'; second_line.maximum_bytes() as usize],
    )
    .expect("second-line chunk");
    let _ = respond(&mut model, &bytes_request, AppResponsePayload::WorkbenchReviewDiffBytes(raw));

    bytes_request = request(&key(&mut model, KeyCode::Char('[')));
    let AppRequestPayload::QueryWorkbenchReviewDiffBytes(previous_line) = bytes_request.payload()
    else {
        panic!("previous truncated line request")
    };
    assert_eq!(previous_line.offset(), first_line_end - 32 * 1024);
}

fn raw_diff(before: &str, after: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-{before}\n+{after}\n"
    )
}

fn page(
    query: WorkbenchQuery,
    run: RunId,
    revision: u64,
    candidate: u8,
    before: &str,
    after: &str,
    comments: &[WorkbenchReviewComment],
) -> WorkbenchReviewPage {
    let candidate = Sha256Digest::new([candidate; 32]);
    let (diff, files) =
        parse_workbench_diff(run, query.workspace(), candidate, &raw_diff(before, after))
            .expect("structured diff");
    WorkbenchReviewPage::new(
        WorkbenchReviewQuery::new(query, run, revision, 0),
        candidate,
        diff,
        files,
        comments.to_owned(),
        u32::try_from(comments.len()).expect("bounded review comment count"),
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
    .expect("page")
}

#[test]
fn selected_hunk_comment_retains_rejected_draft_and_stale_comment_rebinds_explicitly() {
    let (mut model, query, run) = review_model();
    model.chat.buffer = "/diff".to_owned();
    let inspect = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::QueryWorkbenchReview(_requested) = inspect.payload() else {
        panic!("structured review query")
    };
    let first = page(query, run, 7, 10, "old", "new", &[]);
    respond(&mut model, &inspect, AppResponsePayload::WorkbenchReview(first));
    assert_eq!(model.view, View::Diff);

    key(&mut model, KeyCode::Tab); // file -> hunk
    key(&mut model, KeyCode::Char('e'));
    let editor = model.editor.as_mut().expect("review editor");
    editor.buffer = "Why is this necessary?".to_owned();
    editor.cursor = editor.buffer.len();
    let submit = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(add) = submit.payload() else { panic!("add") };
    let (comment_id, stale_anchor) = match add.intent() {
        WorkbenchIntent::AddReview { anchor, feedback, message } => {
            assert_eq!(*feedback, WorkbenchReviewFeedback::Explain);
            assert_eq!(message.as_str(), "Why is this necessary?");
            assert_eq!(anchor.target(), peritus_app_protocol::WorkbenchReviewTarget::Hunk);
            (add.operation(), anchor.clone())
        }
        _ => panic!("anchored add"),
    };
    respond(
        &mut model,
        &submit,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert_eq!(model.editor.as_ref().expect("retained editor").buffer, "Why is this necessary?");
    model.editor = None;

    let refresh = request(&model.refresh_review());
    let stale = WorkbenchReviewComment::new(
        comment_id,
        1,
        stale_anchor,
        WorkbenchReviewFeedback::Explain,
        WorkbenchInputText::new("Why is this necessary?".to_owned()).expect("message"),
        WorkbenchInputSelection::new(
            WorkbenchInputId::new(*comment_id.as_bytes()).expect("input"),
            1,
        )
        .expect("selection"),
        WorkbenchReviewCommentState::Stale,
    )
    .expect("comment");
    let current = page(query, run, 8, 11, "new", "newer", &[stale]);
    respond(&mut model, &refresh, AppResponsePayload::WorkbenchReview(current));
    key(&mut model, KeyCode::Tab); // hunk -> comment
    let rebind_request = request(&key(&mut model, KeyCode::Char('b')));
    let AppRequestPayload::WorkbenchCommand(rebind) = rebind_request.payload() else {
        panic!("rebind")
    };
    assert!(matches!(
        rebind.intent(),
        WorkbenchIntent::RebindReview { comment, anchor }
            if *comment == comment_id
                && anchor.candidate_digest() == Sha256Digest::new([11; 32])
                && anchor.target() == peritus_app_protocol::WorkbenchReviewTarget::Hunk
    ));
    let receipt = WorkbenchReceipt::new(
        rebind.operation(),
        rebind.query(),
        rebind.expected_revision() + 1,
        Sha256Digest::new([12; 32]),
    )
    .expect("receipt");
    respond(&mut model, &rebind_request, AppResponsePayload::WorkbenchReceipt(receipt));
    assert!(
        model
            .product
            .as_ref()
            .expect("product")
            .review
            .message
            .contains("Read-only explanation started.")
    );
}

#[test]
fn refreshed_review_never_silently_retargets_an_open_draft() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    let requested = WorkbenchReviewQuery::new(query, run, 0, 0);
    model.accept_review_page(requested, page(query, run, 7, 10, "old", "new", &[]));
    key(&mut model, KeyCode::Char('e'));
    let editor = model.editor.as_mut().unwrap();
    editor.buffer = "Explain this exact change".to_owned();
    editor.cursor = editor.buffer.len();
    model.accept_review_page(requested, page(query, run, 8, 11, "new", "newer", &[]));
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.editor.as_ref().unwrap().buffer, "Explain this exact change");
    let refresh = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('f'),
        KeyModifiers::CONTROL,
    ))));
    let refresh = request(&refresh);
    respond(
        &mut model,
        &refresh,
        AppResponsePayload::WorkbenchReview(page(query, run, 8, 11, "new", "newer", &[])),
    );
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('b'),
        KeyModifiers::CONTROL,
    ))));
    let submit = request(&key(&mut model, KeyCode::Enter));
    assert!(matches!(submit.payload(), AppRequestPayload::WorkbenchCommand(command)
        if matches!(command.intent(), WorkbenchIntent::AddReview { anchor, .. }
            if anchor.candidate_digest() == Sha256Digest::new([11; 32]))));
}

#[test]
fn late_review_rejection_preserves_a_newer_editor() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    model.accept_review_page(
        WorkbenchReviewQuery::new(query, run, 0, 0),
        page(query, run, 7, 10, "old", "new", &[]),
    );
    key(&mut model, KeyCode::Char('e'));
    let editor = model.editor.as_mut().unwrap();
    editor.buffer = "First review comment".to_owned();
    editor.cursor = editor.buffer.len();
    let submit = request(&key(&mut model, KeyCode::Enter));
    key(&mut model, KeyCode::Char('v'));
    let editor = model.editor.as_mut().unwrap();
    editor.buffer = "Newer draft in progress".to_owned();
    editor.cursor = editor.buffer.len();
    respond(
        &mut model,
        &submit,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert_eq!(model.editor.as_ref().unwrap().buffer, "Newer draft in progress");
    key(&mut model, KeyCode::Esc);
    model.accept_review_page(
        WorkbenchReviewQuery::new(query, run, 0, 0),
        page(query, run, 8, 11, "new", "newer", &[]),
    );
    key(&mut model, KeyCode::Char('e'));
    assert_eq!(model.editor.as_ref().unwrap().buffer, "First review comment");
    assert!(key(&mut model, KeyCode::Enter).is_empty());
}

#[test]
fn uncertain_review_request_keeps_one_operation_until_receipt_resolution() {
    let (mut model, query, run) = review_model();
    model.view = View::Diff;
    model.accept_review_page(
        WorkbenchReviewQuery::new(query, run, 0, 0),
        page(query, run, 7, 10, "old", "new", &[]),
    );
    key(&mut model, KeyCode::Char('e'));
    let editor = model.editor.as_mut().unwrap();
    editor.buffer = "Explain this change".to_owned();
    editor.cursor = editor.buffer.len();
    let submit = request(&key(&mut model, KeyCode::Enter));
    respond(
        &mut model,
        &submit,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            AppErrorCode::Backpressure,
            None,
        )),
    );
    assert!(model.editor.is_none());
    let (command, draft) = model.chat.workbench.unresolved.as_ref().unwrap();
    assert_eq!(submit.payload(), &AppRequestPayload::WorkbenchCommand(command.clone()));
    assert_eq!(draft, "Explain this change");
}
