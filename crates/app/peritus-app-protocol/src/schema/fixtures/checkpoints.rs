//! Covered-path checkpoint, preview-bound rewind, and durable restore fixtures.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    WorkbenchCheckpointCoveragePage, WorkbenchCheckpointFileMode, WorkbenchCheckpointName,
    WorkbenchCheckpointPageRequest, WorkbenchCheckpointPath, WorkbenchCheckpointReceipt,
    WorkbenchCheckpointReferences, WorkbenchCheckpointVersion, WorkbenchCommand,
    WorkbenchCoverageCursor, WorkbenchCoverageSection, WorkbenchIntent, WorkbenchQuery,
    WorkbenchRestoreReceipt, WorkbenchRestoreStatus, WorkbenchRestoreSummary,
    WorkbenchRewindConfirmation, WorkbenchRewindCoveragePage, WorkbenchRewindDisposition,
    WorkbenchRewindPageRequest, WorkbenchRewindPath, WorkbenchRewindPreview,
    WorkbenchRewindRequest,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::Sha256Digest;

struct CheckpointFixture {
    query: WorkbenchQuery,
    checkpoint: ControlOperationId,
    minimal: WorkbenchRewindRequest,
    rewind: WorkbenchRewindRequest,
    create: WorkbenchCommand,
    apply: WorkbenchCommand,
    compact_apply: WorkbenchCommand,
    receipt: WorkbenchCheckpointReceipt,
    preview: WorkbenchRewindPreview,
    confirmation: WorkbenchRewindConfirmation,
    restored: WorkbenchRestoreReceipt,
    summary: WorkbenchRestoreSummary,
}

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let fixture = checkpoint_fixture();
    let mut cases = branch_cases(fixture.minimal, limits)?;
    cases.extend(basic_request_cases(&fixture, limits)?);
    cases.extend(receipt_response_cases(&fixture, limits)?);
    cases.extend(rewind_command_cases(&fixture, limits)?);
    cases.extend(page_request_cases(&fixture, limits)?);
    cases.extend(page_cases(&fixture, limits)?);
    cases.extend(terminal_result_cases(&fixture, limits)?);
    Ok(cases)
}

fn checkpoint_fixture() -> CheckpointFixture {
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
    let checkpoint_receipt =
        checkpoint_receipt(query, checkpoint, before, owned, &exclusions, &external);
    let rewind = WorkbenchRewindRequest::new(query, 9, checkpoint).expect("request");
    let preview = rewind_preview(rewind, before, owned, exclusions, external.clone());
    let confirmation =
        WorkbenchRewindConfirmation::for_preview(rewind, &checkpoint_receipt, &preview)
            .expect("full confirmation");
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
    let compact_apply =
        WorkbenchCommand::new(restore, query, 9, WorkbenchIntent::ConfirmRewind(confirmation));
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
    let summary = WorkbenchRestoreSummary::new(
        restore,
        checkpoint,
        recovery,
        query,
        11,
        WorkbenchRestoreStatus::Applied,
        1,
        0,
        confirmation.preview_digest(),
    )
    .expect("summary");
    CheckpointFixture {
        query,
        checkpoint,
        minimal,
        rewind,
        create,
        apply,
        compact_apply,
        receipt: checkpoint_receipt,
        preview,
        confirmation,
        restored,
        summary,
    }
}

fn basic_request_cases(
    fixture: &CheckpointFixture,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let CheckpointFixture { minimal, create, .. } = fixture;
    Ok(vec![
        encoded(
            "minimal-workbench-rewind-preview-request",
            FixtureClass::Minimal,
            &request(AppRequestPayload::PreviewWorkbenchRewind(*minimal)),
            limits,
        )?,
        encoded(
            "minimal-workbench-checkpoint-inspection",
            FixtureClass::Minimal,
            &request(AppRequestPayload::InspectWorkbenchCheckpoint(*minimal)),
            limits,
        )?,
        encoded(
            "realistic-workbench-checkpoint-create",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(create.clone())),
            limits,
        )?,
    ])
}

fn receipt_response_cases(
    fixture: &CheckpointFixture,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    Ok(vec![
        encoded(
            "realistic-workbench-checkpoint",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchCheckpoint(fixture.receipt.clone())),
            limits,
        )?,
        encoded(
            "realistic-workbench-rewind-preview",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchRewindPreview(fixture.preview.clone())),
            limits,
        )?,
    ])
}

fn rewind_command_cases(
    fixture: &CheckpointFixture,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let CheckpointFixture { apply, compact_apply, .. } = fixture;
    Ok(vec![
        encoded(
            "realistic-workbench-rewind-apply",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(apply.clone())),
            limits,
        )?,
        encoded(
            "realistic-workbench-rewind-confirmation",
            FixtureClass::Realistic,
            &request(AppRequestPayload::WorkbenchCommand(compact_apply.clone())),
            limits,
        )?,
    ])
}

fn page_request_cases(
    fixture: &CheckpointFixture,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let CheckpointFixture { query, checkpoint, rewind, .. } = fixture;
    Ok(vec![
        encoded(
            "realistic-workbench-checkpoint-page-request",
            FixtureClass::Realistic,
            &request(AppRequestPayload::QueryWorkbenchCheckpointPage(
                WorkbenchCheckpointPageRequest::new(*query, 9, *checkpoint, None)
                    .expect("page request"),
            )),
            limits,
        )?,
        encoded(
            "realistic-workbench-rewind-page-request",
            FixtureClass::Realistic,
            &request(AppRequestPayload::QueryWorkbenchRewindPage(WorkbenchRewindPageRequest::new(
                *rewind, None,
            ))),
            limits,
        )?,
    ])
}

fn page_cases(
    fixture: &CheckpointFixture,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let checkpoint_page_fingerprint = fixture.receipt.page_fingerprint(9)?;
    let checkpoint_page = WorkbenchCheckpointCoveragePage::new(
        &fixture.receipt,
        9,
        checkpoint_page_fingerprint,
        1,
        1,
        2,
        WorkbenchCoverageSection::Paths,
        0,
        vec![fixture.receipt.paths()[0].clone()],
        Vec::new(),
        Vec::new(),
        Some(WorkbenchCoverageCursor::new(
            WorkbenchCoverageSection::Exclusions,
            0,
            checkpoint_page_fingerprint,
        )),
    )
    .expect("checkpoint page");
    let rewind_page = WorkbenchRewindCoveragePage::new(
        fixture.confirmation,
        1,
        1,
        2,
        WorkbenchCoverageSection::Paths,
        0,
        fixture.preview.paths().to_vec(),
        Vec::new(),
        Vec::new(),
        Some(WorkbenchCoverageCursor::new(
            WorkbenchCoverageSection::Exclusions,
            0,
            fixture.confirmation.preview_digest(),
        )),
    )
    .expect("rewind page");

    Ok(vec![
        encoded(
            "realistic-workbench-checkpoint-page",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchCheckpointPage(checkpoint_page)),
            limits,
        )?,
        encoded(
            "realistic-workbench-rewind-page",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchRewindPage(rewind_page)),
            limits,
        )?,
    ])
}

fn terminal_result_cases(
    fixture: &CheckpointFixture,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    Ok(vec![
        encoded(
            "realistic-workbench-restore-summary",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchRestoreSummary(fixture.summary.clone())),
            limits,
        )?,
        encoded(
            "realistic-workbench-restore",
            FixtureClass::Realistic,
            &response(AppResponsePayload::WorkbenchRestore(fixture.restored.clone())),
            limits,
        )?,
    ])
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
                    .with_branch(crate::WorkbenchRewindMode::Combined, id(93, ConversationId::new))
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
    owned: WorkbenchCheckpointVersion,
    exclusions: &[String],
    external: &[String],
) -> WorkbenchCheckpointReceipt {
    WorkbenchCheckpointReceipt::new(
        checkpoint,
        query,
        8,
        WorkbenchCheckpointName::new("before owned edit".to_owned()).expect("name"),
        WorkbenchCheckpointReferences::new(7, 3, 2, Some(4)),
        vec![
            WorkbenchCheckpointPath::new("src/note.txt".to_owned(), before, Some(owned))
                .expect("path"),
        ],
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
