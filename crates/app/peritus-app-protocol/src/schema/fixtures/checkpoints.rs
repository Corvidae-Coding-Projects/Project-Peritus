//! Covered-path checkpoint, preview-bound rewind, and durable restore fixtures.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    WorkbenchCheckpointFileMode, WorkbenchCheckpointName, WorkbenchCheckpointPath,
    WorkbenchCheckpointReceipt, WorkbenchCheckpointReferences, WorkbenchCheckpointVersion,
    WorkbenchCommand, WorkbenchIntent, WorkbenchQuery, WorkbenchRestoreReceipt,
    WorkbenchRestoreStatus, WorkbenchRewindDisposition, WorkbenchRewindPath,
    WorkbenchRewindPreview, WorkbenchRewindRequest,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::Sha256Digest;

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let query =
        WorkbenchQuery::new(id(41, ConversationId::new), id(32, peritus_types::WorkspaceId::new));
    let checkpoint = id(90, ControlOperationId::new);
    let minimal = WorkbenchRewindRequest::new(query, 8, checkpoint).expect("request");
    let before = version(60, 18);
    let owned = version(61, 22);
    let exclusions =
        vec!["excerpt.txt: partial selection is not a whole-file restore target".to_owned()];
    let external = vec![
        "Conversation history and cumulative goal and accounting state are preserved.".to_owned(),
        "Credentials, approvals, process state, and external side effects are excluded.".to_owned(),
    ];
    let checkpoint_receipt = checkpoint_receipt(query, checkpoint, before, &exclusions, &external);
    let rewind = WorkbenchRewindRequest::new(query, 9, checkpoint).expect("request");
    let preview = rewind_preview(rewind, before, owned, exclusions, external.clone());
    let create = WorkbenchCommand::new(
        checkpoint,
        query,
        7,
        WorkbenchIntent::CreateCheckpoint(
            WorkbenchCheckpointName::new("before owned edit".to_owned()).expect("name"),
        ),
    );
    let restore = id(91, ControlOperationId::new);
    let recovery = id(92, ControlOperationId::new);
    let apply =
        WorkbenchCommand::new(restore, query, 9, WorkbenchIntent::ApplyRewind(preview.clone()));
    let restored = WorkbenchRestoreReceipt::new(
        restore,
        checkpoint,
        recovery,
        query,
        11,
        WorkbenchRestoreStatus::Applied,
        vec!["src/note.txt".to_owned()],
        Vec::new(),
        external,
    )
    .expect("receipt");

    let mut cases = branch_cases(minimal, limits)?;
    cases.extend([
        encoded(
            "minimal-workbench-rewind-preview-request",
            FixtureClass::Minimal,
            &request(AppRequestPayload::PreviewWorkbenchRewind(minimal)),
            limits,
        )?,
        encoded(
            "minimal-workbench-checkpoint-inspection",
            FixtureClass::Minimal,
            &request(AppRequestPayload::InspectWorkbenchCheckpoint(minimal)),
            limits,
        )?,
        encoded(
            "realistic-workbench-checkpoint-create",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(create)),
            limits,
        )?,
        encoded(
            "realistic-workbench-checkpoint",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchCheckpoint(checkpoint_receipt)),
            limits,
        )?,
        encoded(
            "realistic-workbench-rewind-preview",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchRewindPreview(preview)),
            limits,
        )?,
        encoded(
            "realistic-workbench-rewind-apply",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(apply)),
            limits,
        )?,
        encoded(
            "realistic-workbench-restore",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchRestore(restored)),
            limits,
        )?,
    ]);
    Ok(cases)
}

fn branch_cases(
    minimal: WorkbenchRewindRequest,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    Ok(vec![
        encoded(
            "realistic-workbench-conversation-rewind-request",
            FixtureClass::Realistic,
            &request(AppRequestPayload::PreviewWorkbenchRewind(
                minimal
                    .with_branch(
                        crate::WorkbenchRewindMode::ConversationOnly,
                        id(93, ConversationId::new),
                        None,
                    )
                    .expect("logical selection"),
            )),
            limits,
        )?,
        encoded(
            "realistic-workbench-combined-rewind-request",
            FixtureClass::Realistic,
            &request(AppRequestPayload::PreviewWorkbenchRewind(
                minimal
                    .with_branch(
                        crate::WorkbenchRewindMode::Combined,
                        id(93, ConversationId::new),
                        Some(crate::WorkbenchForkBudget::new(1000, 2, 3, 400).expect("allocation")),
                    )
                    .expect("combined selection"),
            )),
            limits,
        )?,
    ])
}

fn checkpoint_receipt(
    query: WorkbenchQuery,
    checkpoint: ControlOperationId,
    before: WorkbenchCheckpointVersion,
    exclusions: &[String],
    external: &[String],
) -> WorkbenchCheckpointReceipt {
    WorkbenchCheckpointReceipt::new(
        checkpoint,
        query,
        8,
        WorkbenchCheckpointName::new("before owned edit".to_owned()).expect("name"),
        WorkbenchCheckpointReferences::new(7, 3, 2, Some(4)),
        vec![WorkbenchCheckpointPath::new("src/note.txt".to_owned(), before, None).expect("path")],
        exclusions.to_vec(),
        external.to_vec(),
    )
    .expect("receipt")
}

fn rewind_preview(
    rewind: WorkbenchRewindRequest,
    before: WorkbenchCheckpointVersion,
    owned: WorkbenchCheckpointVersion,
    exclusions: Vec<String>,
    external: Vec<String>,
) -> WorkbenchRewindPreview {
    let path = WorkbenchRewindPath::new(
        "src/note.txt".to_owned(),
        before,
        Some(owned),
        owned,
        WorkbenchRewindDisposition::Restore,
    )
    .expect("path");
    WorkbenchRewindPreview::new(rewind, vec![path], exclusions, external).expect("preview")
}

fn response(payload: AppResponsePayload) -> AppResponseEnvelope {
    AppResponseEnvelope::new(
        context(),
        id(10, crate::RequestId::new),
        id(11, crate::CorrelationId::new),
        payload,
    )
}

const fn version(byte: u8, bytes: u64) -> WorkbenchCheckpointVersion {
    WorkbenchCheckpointVersion::Present {
        digest: Sha256Digest::new([byte; 32]),
        bytes,
        mode: WorkbenchCheckpointFileMode::Regular,
    }
}
