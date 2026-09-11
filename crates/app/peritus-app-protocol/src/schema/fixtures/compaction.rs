//! Deterministic local compaction preview and explicit confirmation fixtures.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    WorkbenchCommand, WorkbenchCompactionEntry, WorkbenchCompactionFocus,
    WorkbenchCompactionPreview, WorkbenchCompactionRequest, WorkbenchIntent, WorkbenchInvocationId,
    WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::Sha256Digest;

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query =
        WorkbenchQuery::new(id(41, ConversationId::new), id(32, peritus_types::WorkspaceId::new));
    let minimal = WorkbenchCompactionRequest::new(query, 7, None).expect("request");
    let focused = WorkbenchCompactionRequest::new(
        query,
        7,
        Some(WorkbenchCompactionFocus::new("preserve decisions".to_owned()).expect("focus")),
    )
    .expect("request");
    let entries = [51_u8, 52]
        .into_iter()
        .map(|byte| {
            WorkbenchCompactionEntry::new(
                id(byte, WorkbenchInvocationId::new),
                Sha256Digest::new([byte + 1; 32]),
                800,
                Sha256Digest::new([byte + 2; 32]),
                240,
            )
            .expect("entry")
        })
        .collect();
    let preview =
        WorkbenchCompactionPreview::new(focused, 2, 2, 1, 0, 1, entries).expect("preview");
    let response = AppResponseEnvelope::new(
        context(),
        id(10, crate::RequestId::new),
        id(11, crate::CorrelationId::new),
        AppResponsePayload::WorkbenchCompactionPreview(preview.clone()),
    );
    let command = WorkbenchCommand::new(
        id(53, ControlOperationId::new),
        query,
        7,
        WorkbenchIntent::ApplyCompaction(preview),
    );
    Ok(vec![
        encoded(
            "minimal-workbench-compaction-preview-request",
            FixtureClass::Minimal,
            &request(AppRequestPayload::PreviewWorkbenchCompaction(minimal)),
            limits,
        )?,
        encoded(
            "realistic-workbench-compaction-preview",
            FixtureClass::Realistic,
            &response,
            limits,
        )?,
        encoded(
            "realistic-workbench-compaction-apply",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(command)),
            limits,
        )?,
    ])
}
