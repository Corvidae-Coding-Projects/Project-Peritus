//! Client-chosen external text becomes scoped immutable data, never daemon path authority.
use super::super::images::daemon_error;
use super::{
    ActorId, AppProtocolError, AppResponsePayload, Code, ControlError, Error, ProductRunService,
    ValidatedFileText, WorkbenchCommand, WorkbenchIntent, app_error, domain_operation, error_value,
};
use crate::AuthorityHandle;
use peritus_app_protocol::{
    WorkbenchFileImportPreview, WorkbenchFileImportRequest, WorkbenchFileRange,
    WorkbenchFileUpload, WorkbenchReceipt,
};
use peritus_product_runner::control::{
    ControlText, FileAttachment, FileObservation, FileRange, FileSource, FileVersion, OperationId,
};
use peritus_types::{ArtifactId, SessionId};

impl ProductRunService {
    pub(crate) async fn begin_workbench_file_upload(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        session: SessionId,
        request: &WorkbenchFileUpload,
        maximum_chunk_bytes: usize,
    ) -> Result<(), AppProtocolError> {
        let scope = self.image_scope(actor, request.query(), request.revision())?;
        authority
            .begin_scoped_artifact_upload(
                actor,
                session,
                request.metadata().clone(),
                maximum_chunk_bytes,
                scope,
            )
            .await
            .map_err(daemon_error)
    }
    pub(crate) async fn preview_workbench_file_import(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        request: &WorkbenchFileImportRequest,
    ) -> AppResponsePayload {
        self.prepare_file_import(authority, actor, request)
            .await
            .map_or_else(AppResponsePayload::Error, |(preview, _)| {
                AppResponsePayload::WorkbenchFileImportPreview(preview)
            })
    }
    async fn prepare_file_import(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        request: &WorkbenchFileImportRequest,
    ) -> Result<(WorkbenchFileImportPreview, ValidatedFileText), AppProtocolError> {
        let selection = request.selection();
        self.require_workspace_permissions(
            actor,
            selection.query(),
            &[peritus_product_runner::control::PermissionCapability::Read],
        )
        .map_err(error_value)?;
        let scope = self.image_scope(actor, selection.query(), selection.revision())?;
        let provider = self
            .select_provider(selection.provider(), selection.model())
            .map_err(|_| app_error(Code::MissingRequiredFeature))?;
        let (catalog, bytes) = authority
            .read_scoped_artifact(scope, request.artifact(), u64::MAX)
            .await
            .map_err(daemon_error)?;
        if !matches!(catalog.media_type(), "text/plain" | "application/octet-stream") {
            return Err(app_error(Code::MalformedFrame));
        }
        let text = ValidatedFileText::new(bytes).map_err(|_| app_error(Code::MalformedFrame))?;
        if text.digest() != request.file().digest() || text.bytes() != request.file().bytes() {
            return Err(app_error(Code::MalformedFrame));
        }
        let profile = provider.profile();
        let preview = WorkbenchFileImportPreview::new(
            request.clone(),
            profile.revision(),
            profile.model().as_str().to_owned(),
        )?;
        self.image_scope(actor, selection.query(), selection.revision())?;
        Ok((preview, text))
    }
    pub(crate) async fn confirm_workbench_file_import(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        self.confirm_import(authority, actor, command)
            .await
            .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchReceipt)
    }
    async fn confirm_import(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        let previews: Vec<(&WorkbenchFileImportPreview, bool)> = match command.intent() {
            WorkbenchIntent::AttachFileImport { preview, .. } => vec![(preview, false)],
            WorkbenchIntent::EnqueueMessage { preview } => vec![(preview, true)],
            WorkbenchIntent::EnqueueMessageBundle { message, attachments, .. } => message
                .iter()
                .map(|preview| (preview, true))
                .chain(attachments.iter().map(|preview| (preview, false)))
                .collect(),
            _ => return Err(app_error(Code::MalformedFrame)),
        };
        self.control_workspace(command.query()).map_err(error_value)?;
        let operation = domain_operation(actor, command).map_err(error_value)?;
        let prior =
            self.with_controls(false, |store| store.resolve(&operation)).map_err(error_value)?;
        let receipt = if let Some(receipt) = prior {
            receipt
        } else {
            let mut prepared = Vec::new();
            for (preview, user_message) in previews {
                let (current, text) =
                    self.prepare_file_import(authority, actor, preview.request()).await?;
                if user_message
                    && (text.text().trim().is_empty()
                        || text
                            .text()
                            .chars()
                            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t')))
                {
                    return Err(app_error(Code::MalformedFrame));
                }
                if current != *preview {
                    return Err(app_error(Code::StaleRevision));
                }
                let consent =
                    current.canonical_bytes().map_err(|_| app_error(Code::MalformedFrame))?;
                prepared.push((text, consent));
            }
            if !authority.status().await.map_err(daemon_error)?.mutation_ready() {
                return Err(app_error(Code::ReadOnly));
            }
            let refs = prepared
                .iter()
                .map(|(text, consent)| (text, consent.as_slice()))
                .collect::<Vec<_>>();
            self.with_controls(false, |store| store.accept_files(&operation, &refs))
                .map_err(error_value)?
        };
        WorkbenchReceipt::new(
            command.operation(),
            command.query(),
            receipt.accepted_revision(),
            receipt.payload_digest(),
        )
    }
}

pub(in crate::product_run::workbench) fn domain_import(
    command: &WorkbenchCommand,
    preview: &WorkbenchFileImportPreview,
) -> Result<FileAttachment, Error> {
    domain_import_part(
        command,
        preview,
        command.operation().into_bytes(),
        matches!(command.intent(), WorkbenchIntent::EnqueueMessage { .. }),
        matches!(command.intent(), WorkbenchIntent::EnqueueMessage { .. }),
    )
}

pub(in crate::product_run::workbench) fn domain_import_part(
    command: &WorkbenchCommand,
    preview: &WorkbenchFileImportPreview,
    operation: [u8; 16],
    user_message: bool,
    shared: bool,
) -> Result<FileAttachment, Error> {
    let request = preview.request();
    let selection = request.selection();
    if command.query() != selection.query() || command.expected_revision() != selection.revision() {
        return Err(ControlError::ScopeMismatch.into());
    }
    let range = match selection.range() {
        WorkbenchFileRange::All => FileRange::All,
        WorkbenchFileRange::Bytes { start, end } => FileRange::Bytes { start, end },
        WorkbenchFileRange::Lines { first, last } => FileRange::Lines { first, last },
    };
    let source = if user_message {
        if range != FileRange::All || request.file().bytes() == 0 {
            return Err(ControlError::InvalidInput.into());
        }
        FileSource::user_message()
    } else {
        FileSource::imported(ControlText::new(selection.path().to_owned())?, range)?
    };
    let file = request.file();
    let observation = FileObservation::new(
        file.source_digest(),
        file.source_bytes(),
        file.range(),
        file.digest(),
    )?;
    let version = FileVersion::new(
        OperationId::new(operation)?,
        ArtifactId::new(operation).map_err(|_| ControlError::InvalidInput)?,
        observation,
        preview.fingerprint().map_err(|_| ControlError::InvalidInput)?,
    )?;
    Ok(if shared {
        FileAttachment::for_message(
            source,
            version,
            peritus_product_runner::control::InputId::new(command.operation().into_bytes())?,
        )?
    } else {
        FileAttachment::new(source, version)?
    })
}
