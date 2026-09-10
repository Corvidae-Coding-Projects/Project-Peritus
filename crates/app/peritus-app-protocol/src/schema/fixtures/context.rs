//! Stable content-free context query and projection fixtures.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ConversationId,
    WorkbenchContextDisposition as D, WorkbenchContextPage, WorkbenchContextQuery,
    WorkbenchContextRow, WorkbenchContextSeal, WorkbenchContextSource as S,
    WorkbenchContextView as V, WorkbenchInputId, WorkbenchInputSelection, WorkbenchInvocationId,
    WorkbenchMessageRole as R, WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::Sha256Digest;

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let scope =
        WorkbenchQuery::new(id(41, ConversationId::new), id(32, peritus_types::WorkspaceId::new));
    let invocation = id(48, WorkbenchInvocationId::new);
    let digest = Sha256Digest::new([49; 32]);
    let seal = WorkbenchContextSeal::new(invocation, digest, Sha256Digest::new([50; 32]), 7);
    let mut cases = Vec::new();
    for (name, view) in [
        ("minimal-context-next-query", V::Next),
        ("minimal-context-history-query", V::History),
        ("minimal-context-invocation-query", V::Invocation(invocation)),
    ] {
        let query = WorkbenchContextQuery::new(scope, 0, 0, view).expect("query");
        cases.push(encoded(
            name,
            FixtureClass::Minimal,
            &request(AppRequestPayload::QueryWorkbenchContext(query)),
            limits,
        )?);
    }
    let input = WorkbenchInputSelection::new(id(44, WorkbenchInputId::new), 2).expect("input");
    let row = |source, disposition| {
        WorkbenchContextRow::new(source, digest, 128, disposition).expect("row")
    };
    let mut rows =
        vec![row(S::Input(input), D::Included), row(S::PublicReply(invocation), D::Included)];
    let image = S::Image {
        operation: id(51, crate::ControlOperationId::new),
        input: input.id(),
        artifact: id(52, peritus_types::ArtifactId::new),
    };
    rows.push(row(image, D::Included));
    for (ordinal, role) in (0_u32..).zip([R::System, R::Developer, R::User, R::Assistant, R::Tool])
    {
        rows.push(row(S::Message { ordinal, role }, D::Included));
    }
    let next_rows = [D::Eligible, D::Held, D::Withdrawn, D::Superseded, D::DependencyBlocked]
        .into_iter()
        .zip(1_u64..)
        .map(|(disposition, revision)| {
            row(
                S::Input(WorkbenchInputSelection::new(input.id(), revision).expect("selection")),
                disposition,
            )
        })
        .chain([row(S::PublicReply(invocation), D::AwaitingLaterInput)])
        .chain([row(image, D::Deselected)])
        .collect();
    for (name, view, seal, rows) in [
        ("realistic-context-invocation-page", V::Invocation(invocation), Some(seal), rows),
        ("realistic-context-next-page", V::Next, None, next_rows),
        (
            "realistic-context-history-page",
            V::History,
            None,
            vec![row(
                S::Invocation { id: invocation, manifest_digest: seal.manifest_digest() },
                D::Included,
            )],
        ),
    ] {
        let query = WorkbenchContextQuery::new(scope, 9, 0, view).expect("query");
        let page =
            WorkbenchContextPage::new(query, u32::try_from(rows.len()).expect("count"), seal, rows)
                .expect("page");
        let response = AppResponseEnvelope::new(
            context(),
            id(10, crate::RequestId::new),
            id(11, crate::CorrelationId::new),
            AppResponsePayload::WorkbenchContext(page),
        );
        cases.push(encoded(name, FixtureClass::Realistic, &response, limits)?);
    }
    Ok(cases)
}
