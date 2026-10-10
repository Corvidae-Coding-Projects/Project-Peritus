//! Complete metadata and exact body fixtures for additive brief paging.
use super::super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    WorkbenchBriefObservation, WorkbenchBriefObservationKind, WorkbenchBriefPage,
    WorkbenchBriefPageRequest, WorkbenchBriefProposalPage, WorkbenchBriefProposalReference,
    WorkbenchBriefProposalRequest, WorkbenchInvocationId, WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query =
        WorkbenchQuery::new(id(41, ConversationId::new), id(32, peritus_types::WorkspaceId::new));
    let page_request = WorkbenchBriefPageRequest::new(query, 0, 0, 0).expect("initial page");
    let text = "Exact reply 🦉\nControl bytes remain source data: \u{1b}[31m";
    let reference = WorkbenchBriefProposalReference::new(
        id(61, ControlOperationId::new),
        id(62, WorkbenchInvocationId::new),
        peritus_codec::sha256(text.as_bytes()),
        text.len() as u64,
    )
    .expect("reference");
    let body_request =
        WorkbenchBriefProposalRequest::new(query, 9, reference, 0).expect("body request");
    let empty_file = WorkbenchBriefObservation::new(
        WorkbenchBriefObservationKind::File,
        id(63, ControlOperationId::new),
        Some(id(64, ControlOperationId::new)),
        "empty.txt".into(),
        peritus_codec::sha256(b""),
        0,
        true,
    )
    .expect("valid empty file");
    let page = WorkbenchBriefPage::new(
        page_request,
        9,
        Vec::new(),
        1,
        vec![reference],
        1,
        vec![empty_file],
    )
    .expect("page");
    let body = WorkbenchBriefProposalPage::new(body_request, text.into()).expect("exact body");
    let response = |payload| {
        AppResponseEnvelope::new(
            context(),
            id(10, crate::RequestId::new),
            id(11, crate::CorrelationId::new),
            payload,
        )
    };
    Ok(vec![
        encoded(
            "minimal-workbench-brief-page-query",
            FixtureClass::Minimal,
            &request(AppRequestPayload::QueryWorkbenchBriefPage(page_request)),
            limits,
        )?,
        encoded(
            "realistic-workbench-brief-proposal-query",
            FixtureClass::Realistic,
            &request(AppRequestPayload::QueryWorkbenchBriefProposal(body_request)),
            limits,
        )?,
        encoded(
            "realistic-workbench-brief-page",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchBriefPage(page)),
            limits,
        )?,
        encoded(
            "realistic-workbench-brief-proposal",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchBriefProposal(body)),
            limits,
        )?,
    ])
}
