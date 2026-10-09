use super::*;
use peritus_app_protocol::{
    AppErrorCode, ProductRunPhase, WorkbenchInputId, WorkbenchInputSelection, WorkbenchInputText,
    WorkbenchIntent, WorkbenchQuery, WorkbenchReceipt, WorkbenchReviewComment,
    WorkbenchReviewCommentState, WorkbenchReviewEvidence, WorkbenchReviewEvidenceKind,
    WorkbenchReviewEvidenceState, WorkbenchReviewFeedback, WorkbenchReviewPage,
    WorkbenchReviewQuery, parse_workbench_diff,
};
use peritus_types::{RunId, Sha256Digest};

mod editor;
mod navigation;
mod raw_pages;

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
fn negotiated_summary_opens_large_review_without_legacy_full_page_request() {
    let (mut model, query, run) = review_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchReviewSummary)
            .expect("summary feature"),
    );
    model.chat.buffer = "/diff".to_owned();
    let summary_request = request(&model.refresh_review());
    assert!(matches!(summary_request.payload(), AppRequestPayload::QueryWorkbenchReviewSummary(_)));
    let base = page(query, run, 7, 10, "old", "new", &[]);
    let files = base.files();
    let summary = peritus_app_protocol::WorkbenchReviewSummary::new(
        base.query(),
        base.candidate_digest(),
        base.diff_digest(),
        u32::try_from(files.len()).expect("files"),
        files.iter().map(|file| u64::try_from(file.hunks().len()).expect("hunks")).sum(),
        files
            .iter()
            .flat_map(peritus_app_protocol::WorkbenchDiffFile::hunks)
            .map(|hunk| u64::try_from(hunk.lines().len()).expect("lines"))
            .sum(),
        true,
        Vec::new(),
        0,
        base.evidence().to_vec(),
    )
    .expect("summary");
    let effects =
        respond(&mut model, &summary_request, AppResponsePayload::WorkbenchReviewSummary(summary));
    assert!(
        !effects.is_empty(),
        "summary response did not request diff: {:?}; page={:?}; features={:?}",
        model.product.as_ref().unwrap().review.message,
        model.product.as_ref().unwrap().review.page.as_ref().map(WorkbenchReviewPage::query),
        model.features
    );
    let diff_request = request(&effects);
    assert!(matches!(diff_request.payload(), AppRequestPayload::QueryWorkbenchReviewDiff(_)));
}

#[test]
fn negotiated_summary_is_used_for_comment_page_continuations() {
    let (mut model, query, run) = review_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchReviewSummary)
            .expect("summary feature"),
    );
    let base = page(query, run, 17, 22, "old", "new", &[]);
    model.product.as_mut().unwrap().review.page = Some(
        WorkbenchReviewPage::new(
            WorkbenchReviewQuery::new(query, run, 17, 256),
            base.candidate_digest(),
            base.diff_digest(),
            base.files().to_vec(),
            base.comments().to_vec(),
            base.total_comments(),
            base.evidence().to_vec(),
        )
        .expect("comment continuation page"),
    );
    let effects = model.refresh_review();
    let request = request(&effects);
    let AppRequestPayload::QueryWorkbenchReviewSummary(requested) = request.payload() else {
        panic!("comment continuation uses negotiated summary")
    };
    assert_eq!(requested.offset(), 256);
}

#[test]
fn stale_diff_response_releases_pending_cursor_for_a_fresh_summary_retry() {
    let (mut model, query, run) = review_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchReviewSummary)
            .expect("summary feature"),
    );
    let old_page = page(query, run, 19, 31, "old", "old result", &[]);
    let old_cursor = peritus_app_protocol::WorkbenchReviewDiffQuery::new(query, run, 19, 0, 0, 0);
    let stale_diff = peritus_app_protocol::WorkbenchReviewDiffPage::project(
        old_cursor,
        old_page.candidate_digest(),
        old_page.diff_digest(),
        old_page.files(),
    )
    .expect("stale page");
    model.product.as_mut().unwrap().review.page = Some(old_page);
    model.product.as_mut().unwrap().review.pending_diff = Some(old_cursor);

    let requested = WorkbenchReviewQuery::new(query, run, 0, 0);
    let current = page(query, run, 19, 31, "old", "new result", &[]);
    let summary = peritus_app_protocol::WorkbenchReviewSummary::new(
        current.query(),
        current.candidate_digest(),
        current.diff_digest(),
        u32::try_from(current.files().len()).unwrap(),
        current.files().iter().map(|file| u64::try_from(file.hunks().len()).unwrap()).sum(),
        current
            .files()
            .iter()
            .flat_map(peritus_app_protocol::WorkbenchDiffFile::hunks)
            .map(|hunk| u64::try_from(hunk.lines().len()).unwrap())
            .sum(),
        true,
        Vec::new(),
        0,
        current.evidence().to_vec(),
    )
    .expect("updated summary");
    model.product.as_mut().unwrap().review.pending = Some(requested);
    let replacement = model.accept_review_summary(requested, &summary);
    assert!(matches!(
        request(&replacement).payload(),
        AppRequestPayload::QueryWorkbenchReviewDiff(_)
    ));
    assert_eq!(model.product.as_ref().unwrap().review.pending_diff, Some(old_cursor));

    assert!(model.accept_review_diff_page(old_cursor, stale_diff).is_empty());
    assert!(model.product.as_ref().unwrap().review.pending_diff.is_none());
    let retry = model.refresh_review();
    assert!(matches!(request(&retry).payload(), AppRequestPayload::QueryWorkbenchReviewSummary(_)));
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
