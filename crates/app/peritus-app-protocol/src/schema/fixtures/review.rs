//! Structured-review query, page, and anchored mutation fixtures.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    CorrelationId, RequestId, WorkbenchCommand, WorkbenchInputText, WorkbenchIntent,
    WorkbenchQuery, WorkbenchReviewFeedback, WorkbenchReviewPage, WorkbenchReviewQuery,
    parse_workbench_diff,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let workspace = id(32, WorkspaceId::new);
    let query = WorkbenchQuery::new(id(41, ConversationId::new), workspace);
    let run = id(70, RunId::new);
    let review_query = WorkbenchReviewQuery::new(query, run, 4, 0);
    let candidate = Sha256Digest::new([71; 32]);
    let raw = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let (diff, files) = parse_workbench_diff(run, workspace, candidate, raw).expect("review diff");
    let anchor = files[0].hunks()[0].anchor().clone();
    let comment = id(72, ControlOperationId::new);

    let mut cases = vec![encoded(
        "minimal-workbench-review-query",
        FixtureClass::Minimal,
        &request(AppRequestPayload::QueryWorkbenchReview(review_query)),
        limits,
    )?];
    for (name, operation, intent) in [
        (
            "minimal-workbench-review-add",
            id(73, ControlOperationId::new),
            WorkbenchIntent::AddReview {
                anchor: anchor.clone(),
                feedback: WorkbenchReviewFeedback::RequestRevision,
                message: WorkbenchInputText::new("Make the new behavior explicit.".to_owned())
                    .expect("review message"),
            },
        ),
        (
            "minimal-workbench-review-rebind",
            id(74, ControlOperationId::new),
            WorkbenchIntent::RebindReview { comment, anchor },
        ),
        (
            "minimal-workbench-review-dismiss",
            id(75, ControlOperationId::new),
            WorkbenchIntent::DismissReview { comment },
        ),
    ] {
        cases.push(encoded(
            name,
            FixtureClass::Minimal,
            &request(AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
                operation, query, 4, intent,
            ))),
            limits,
        )?);
    }
    cases.push(encoded(
        "realistic-workbench-review",
        FixtureClass::Realistic,
        &response(AppResponsePayload::WorkbenchReview(
            WorkbenchReviewPage::new(
                review_query,
                candidate,
                diff,
                files,
                Vec::new(),
                0,
                Vec::new(),
            )
            .expect("review page"),
        )),
        limits,
    )?);
    Ok(cases)
}

fn response(payload: AppResponsePayload) -> AppResponseEnvelope {
    AppResponseEnvelope::new(context(), id(10, RequestId::new), id(11, CorrelationId::new), payload)
}
