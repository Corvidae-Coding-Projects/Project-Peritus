//! Additive complete-manifest exchanges alongside byte-identical legacy fixtures.

use super::{
    AppRequestPayload, AppResponsePayload, CodecError, CodecLimits, ControlOperationId,
    FixtureClass, GeneratedFixtureCase, WorkbenchCheckpointName, WorkbenchCheckpointReceipt,
    WorkbenchCheckpointReferences, WorkbenchCommand, WorkbenchIntent, WorkbenchRestoreReceipt,
    WorkbenchRestoreStatus, WorkbenchRewindPreview, WorkbenchRewindRequest, encoded, id, request,
    response,
};

pub(super) fn cases(
    requested: WorkbenchRewindRequest,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query = requested.query();
    let checkpoint = requested.checkpoint();
    let name = WorkbenchCheckpointName::new("complete ".repeat(40)).expect("name");
    let receipt = WorkbenchCheckpointReceipt::new(
        checkpoint,
        query,
        8,
        name.clone(),
        WorkbenchCheckpointReferences::new(7, 3, 2, Some(4)),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("receipt");
    let preview = WorkbenchRewindPreview::new(
        requested,
        Vec::new(),
        vec!["excluded source ".repeat(40)],
        Vec::new(),
    )
    .expect("preview");
    let restore = id(91, ControlOperationId::new);
    let restored = WorkbenchRestoreReceipt::new(
        restore,
        checkpoint,
        id(92, ControlOperationId::new),
        query,
        11,
        WorkbenchRestoreStatus::Applied,
        Vec::new(),
        Vec::new(),
        vec!["external effect ".repeat(300)],
    )
    .expect("restore");
    Ok(vec![
        encoded(
            "realistic-workbench-checkpoint-manifest",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchCheckpoint(receipt)),
            limits,
        )?,
        encoded(
            "realistic-workbench-rewind-manifest",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchRewindPreview(preview.clone())),
            limits,
        )?,
        encoded(
            "realistic-workbench-restore-manifest",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchRestore(restored)),
            limits,
        )?,
        encoded(
            "realistic-workbench-checkpoint-manifest-create",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
                checkpoint,
                query,
                7,
                WorkbenchIntent::CreateCheckpoint(name),
            ))),
            limits,
        )?,
        encoded(
            "realistic-workbench-rewind-manifest-apply",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
                restore,
                query,
                8,
                WorkbenchIntent::ApplyRewind(preview),
            ))),
            limits,
        )?,
    ])
}
