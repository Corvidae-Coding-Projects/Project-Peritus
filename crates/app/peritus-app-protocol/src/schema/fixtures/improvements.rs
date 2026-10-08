//! Paged improvement fixtures cover revision binding and non-ASCII text slices.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, CorrelationId,
    ImprovementCandidateSummary, ImprovementEvidencePage, ImprovementEvidenceSummary,
    ImprovementPage, ImprovementPageCursor, ImprovementRequest, ImprovementTextPage,
    ImprovementTextQuery, ImprovementTextReference, RequestId,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let workspace = id(71, WorkspaceId::new);
    let run = id(72, RunId::new);
    let candidate = Sha256Digest::new([73; 32]);
    let source =
        ImprovementTextReference::new(Sha256Digest::new([74; 32]), 9).expect("source");
    let cursor =
        ImprovementPageCursor::new(workspace, None, 12, 44, false).expect("cursor");
    let evidence_cursor = ImprovementPageCursor::new(workspace, Some(candidate), 12, 45, false)
        .expect("evidence cursor");
    let page = ImprovementPage::new(
        workspace,
        12,
        vec![ImprovementCandidateSummary::new(candidate, source, 1, None, false)],
        Some(cursor),
    )
    .expect("improvement page");
    let evidence = ImprovementEvidencePage::new(
        workspace,
        candidate,
        12,
        vec![ImprovementEvidenceSummary::new(run, source)],
        Some(evidence_cursor),
    )
    .expect("evidence page");
    let query = ImprovementTextQuery::new(workspace, candidate, Some(run), source, 0)
        .expect("text query");
    let text = ImprovementTextPage::new(query, "évidence".to_owned(), None).expect("text page");
    Ok(vec![
        encoded(
            "minimal-improvement-page-request",
            FixtureClass::Minimal,
            &request(AppRequestPayload::Improvements(ImprovementRequest::ListPage {
                workspace,
                after: None,
            })),
            limits,
        )?,
        encoded(
            "realistic-improvement-evidence-page-request",
            FixtureClass::Realistic,
            &request(AppRequestPayload::Improvements(ImprovementRequest::EvidencePage {
                workspace,
                candidate,
                revision: 12,
                after: Some(evidence_cursor),
            })),
            limits,
        )?,
        encoded(
            "realistic-improvement-text-page-request",
            FixtureClass::Realistic,
            &request(AppRequestPayload::Improvements(ImprovementRequest::ReadText(query))),
            limits,
        )?,
        encoded(
            "realistic-improvement-page",
            FixtureClass::Realistic,
            &response(AppResponsePayload::ImprovementPage(page)),
            limits,
        )?,
        encoded(
            "realistic-improvement-evidence-page",
            FixtureClass::Realistic,
            &response(AppResponsePayload::ImprovementEvidencePage(evidence)),
            limits,
        )?,
        encoded(
            "realistic-improvement-text-page",
            FixtureClass::Realistic,
            &response(AppResponsePayload::ImprovementTextPage(text)),
            limits,
        )?,
    ])
}

fn response(payload: AppResponsePayload) -> AppResponseEnvelope {
    AppResponseEnvelope::new(
        context(),
        id(10, RequestId::new),
        id(11, CorrelationId::new),
        payload,
    )
}
