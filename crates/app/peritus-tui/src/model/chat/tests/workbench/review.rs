use super::*;
use peritus_app_protocol::{
    AppErrorCode, ProductRunPhase, WorkbenchInputId, WorkbenchInputSelection, WorkbenchInputText,
    WorkbenchIntent, WorkbenchQuery, WorkbenchReceipt, WorkbenchReviewComment,
    WorkbenchReviewCommentState, WorkbenchReviewEvidence, WorkbenchReviewEvidenceKind,
    WorkbenchReviewEvidenceState, WorkbenchReviewFeedback, WorkbenchReviewPage,
    WorkbenchReviewQuery, parse_workbench_diff,
};
use peritus_types::{RunId, Sha256Digest};

fn review_model() -> (AppModel, WorkbenchQuery, RunId) {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchReview)
            .expect("review feature"),
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
    )
    .expect("snapshot");
    model.accept_product_run(snapshot);
    model.chat.run_id = Some(run);
    (model, query, run)
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
