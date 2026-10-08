//! User messages and selected file data retain distinct authority and immutable bindings.

mod upload;
use super::{App, PreparedChat, Result, command, input_id, operation_id, problem, receipts, stage};
use crate::{daemon, files::attachments};
use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, WellKnownProtocolFeature, WorkbenchCommand, WorkbenchFileImportRequest,
    WorkbenchFileMetadata, WorkbenchFileMode, WorkbenchFileRange, WorkbenchFileRequest,
    WorkbenchInputOrder, WorkbenchInputSource, WorkbenchInputText, WorkbenchIntent,
    WorkbenchNewInput, WorkbenchQueueIntent,
};
use peritus_types::{ArtifactId, Sha256Digest};
use sha2::{Digest as _, Sha256};

pub(super) fn queue_intent(operation: &str, prepared: &PreparedChat) -> Result<WorkbenchQueueIntent> {
    let dependencies = WorkbenchInputOrder::new(Vec::new()).map_err(problem)?;
    if prepared.text.len() <= peritus_app_protocol::MAX_WORKBENCH_INPUT_BYTES {
        return Ok(WorkbenchQueueIntent::Enqueue(WorkbenchNewInput::new(
            input_id(operation)?, WorkbenchInputText::new(prepared.text.clone()).map_err(problem)?,
            dependencies,
        ).map_err(problem)?));
    }
    let artifact = ArtifactId::new(*operation_id(operation, "queue")?.as_bytes())
        .map_err(|error| problem(format!("{error:?}")))?;
    Ok(WorkbenchQueueIntent::EnqueueSource {
        id: input_id(operation)?,
        source: WorkbenchInputSource::new(
            artifact, Sha256Digest::new(Sha256::digest(prepared.text.as_bytes()).into()),
            u64::try_from(prepared.text.len()).map_err(problem)?,
        ).map_err(problem)?,
        dependencies,
    })
}

pub(super) async fn upload_message(
    app: &App, prepared: &PreparedChat, revision: u64, source: WorkbenchInputSource,
) -> Result<()> {
    upload::complete(
        app, prepared, revision, source.artifact(), source.digest(), source.bytes(),
        WellKnownProtocolFeature::WorkbenchRequestSources,
        upload::Body::Memory(prepared.text.as_bytes()),
    ).await
}

pub(super) async fn attach(
    app: &App, operation: &str, prepared: &PreparedChat, mut revision: u64,
) -> Result<u64> {
    for attachment in &prepared.attachments {
        let name = format!("attach:{}", attachment.id);
        let operation_id = operation_id(operation, &name)?;
        let proposed = if let Some(retained) = receipts::retained_workbench_command(app, &stage(operation, &name))? {
            validate_attachment(&retained, prepared, attachment, operation_id)?;
            retained
        } else {
            let artifact = ArtifactId::new(*operation_id.as_bytes())
                .map_err(|error| problem(format!("{error:?}")))?;
            let digest = upload::digest(&attachment.digest)?;
            let file = tokio::fs::File::open(attachments::path(app, attachment)?).await?;
            let metadata = file.metadata().await?;
            if !metadata.is_file() || metadata.len() != attachment.bytes {
                return Err(problem("The retained attachment snapshot is unavailable or changed"));
            }
            upload::complete(app, prepared, revision, artifact, digest, attachment.bytes, WellKnownProtocolFeature::WorkbenchFileSources, upload::Body::File(file)).await?;
            let selection = WorkbenchFileRequest::new(
                prepared.query, revision, attachment.path.clone(), WorkbenchFileRange::All,
                WorkbenchFileMode::Snapshot, prepared.providers.writer(), prepared.models.writer().clone(),
            ).map_err(problem)?;
            let request = WorkbenchFileImportRequest::new(
                selection, artifact,
                WorkbenchFileMetadata::new(digest, attachment.bytes, (0, attachment.bytes), digest).map_err(problem)?,
            ).map_err(problem)?;
            let preview = match daemon::request_owned(app, prepared.owner()?, AppRequestPayload::PreviewWorkbenchFileImport(request.clone())).await? {
                AppResponsePayload::WorkbenchFileImportPreview(preview) if preview.request() == &request => preview,
                _ => return Err(problem("The daemon returned another attachment preview")),
            };
            WorkbenchCommand::new(operation_id, prepared.query, revision, WorkbenchIntent::AttachFileSource { preview })
        };
        let receipt = command(app, prepared.owner()?, operation, &name, proposed).await?;
        revision = revision.max(receipt.accepted_revision());
    }
    Ok(revision)
}

fn validate_attachment(
    command: &WorkbenchCommand, prepared: &PreparedChat, attachment: &attachments::Attachment,
    operation: peritus_app_protocol::ControlOperationId,
) -> Result<()> {
    let WorkbenchIntent::AttachFileSource { preview } = command.intent() else {
        return Err(problem("The retained attachment stage is not the original snapshot admission"));
    };
    let request = preview.request();
    let digest = upload::digest(&attachment.digest)?;
    if command.operation() != operation || command.query() != prepared.query
        || request.selection().query() != prepared.query || request.selection().path() != attachment.path
        || request.selection().range() != WorkbenchFileRange::All
        || request.selection().mode() != WorkbenchFileMode::Snapshot
        || request.selection().provider() != prepared.providers.writer()
        || request.selection().model() != prepared.models.writer()
        || request.artifact().as_bytes() != operation.as_bytes()
        || request.file().source_bytes() != attachment.bytes
        || request.file().source_digest() != digest || request.file().digest() != digest
    { return Err(problem("The retained attachment stage differs from the original snapshot")); }
    Ok(())
}
