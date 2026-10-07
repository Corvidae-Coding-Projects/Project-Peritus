//! Client-chosen external text becomes scoped immutable data, never daemon path authority.
use super::super::images::daemon_error;
use super::{
    ActorId, AppProtocolError, AppResponsePayload, Code, ControlError, Error, ProductRunService,
    WorkbenchCommand, WorkbenchIntent, app_error, domain_operation_with_store, error_value,
};
use crate::AuthorityHandle;
use peritus_app_protocol::{
    WorkbenchFileImportPreview, WorkbenchFileImportRequest, WorkbenchFileRange,
    WorkbenchFileUpload, WorkbenchReceipt,
};
use peritus_product_runner::control::{
    ConversationId, FileAttachment, FileObservation, FileRange, FileSource, FileSourceLabel,
    FileVersion, OperationId,
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
        let service = self.clone();
        let authority = authority.clone();
        let request = request.clone();
        Self::await_blocking_future("begin durable workbench file upload", move || {
            async move {
                service
                    .begin_workbench_file_upload_owned(
                        authority,
                        actor,
                        session,
                        request,
                        maximum_chunk_bytes,
                    )
                    .await
            }
        })
        .await
        .unwrap_or_else(|_| Err(app_error(Code::Internal)))
    }

    async fn begin_workbench_file_upload_owned(
        &self,
        authority: AuthorityHandle,
        actor: ActorId,
        session: SessionId,
        request: WorkbenchFileUpload,
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
        let service = self.clone();
        let authority = authority.clone();
        let request = request.clone();
        Self::await_blocking_future("preview durable workbench file import", move || {
            async move { service.preview_workbench_file_import_owned(authority, actor, request).await }
        })
        .await
        .unwrap_or_else(|_| AppResponsePayload::Error(app_error(Code::Internal)))
    }

    async fn preview_workbench_file_import_owned(
        &self,
        authority: AuthorityHandle,
        actor: ActorId,
        request: WorkbenchFileImportRequest,
    ) -> AppResponsePayload {
        self.prepare_file_source(&authority, actor, &request)
            .await
            .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchFileImportPreview)
    }
    async fn prepare_file_source(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        request: &WorkbenchFileImportRequest,
    ) -> Result<WorkbenchFileImportPreview, AppProtocolError> {
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
        let catalog = authority
            .authorize_scoped_text(scope, request.artifact())
            .await
            .map_err(daemon_error)?;
        if catalog.digest() != request.file().digest()
            || catalog.byte_size() != request.file().bytes()
        {
            return Err(app_error(Code::MalformedFrame));
        }
        super::super::sources::validate_file_source_artifact(
            &self.inner.request_source_artifacts,
            &catalog,
        )?;
        let profile = provider.profile();
        let preview = WorkbenchFileImportPreview::new(
            request.clone(),
            profile.revision(),
            profile.model().as_str().to_owned(),
        )?;
        self.image_scope(actor, selection.query(), selection.revision())?;
        Ok(preview)
    }
    pub(crate) async fn confirm_workbench_file_import(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let service = self.clone();
        let authority = authority.clone();
        let command = command.clone();
        Self::await_blocking_future("confirm durable workbench file import", move || {
            async move { service.confirm_workbench_file_import_owned(authority, actor, command).await }
        })
        .await
        .unwrap_or_else(|_| AppResponsePayload::Error(app_error(Code::Internal)))
    }

    async fn confirm_workbench_file_import_owned(
        &self,
        authority: AuthorityHandle,
        actor: ActorId,
        command: WorkbenchCommand,
    ) -> AppResponsePayload {
        self.confirm_import(&authority, actor, &command)
            .await
            .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchReceipt)
    }
    async fn confirm_import(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        let preview = match command.intent() {
            WorkbenchIntent::AttachFileImport { preview, .. }
            | WorkbenchIntent::AttachFileSource { preview } => preview,
            _ => return Err(app_error(Code::MalformedFrame)),
        };
        self.control_workspace(command.query()).map_err(error_value)?;
        let conversation = ConversationId::new(command.query().conversation().into_bytes())
            .map_err(|_| app_error(Code::MalformedFrame))?;
        let (operation, prior) = self
            .with_control_conversation(conversation, |store| {
                let operation = domain_operation_with_store(store, actor, command)?;
                let prior = store.resolve(&operation)?;
                Ok((operation, prior))
            })
            .map_err(error_value)?;
        let receipt = if let Some(receipt) = prior {
            receipt
        } else {
            let current = self.prepare_file_source(authority, actor, preview.request()).await?;
            if current != *preview {
                return Err(app_error(Code::StaleRevision));
            }
            if !authority.status().await.map_err(daemon_error)?.mutation_ready() {
                return Err(app_error(Code::ReadOnly));
            }
            let consent = current.canonical_bytes().map_err(|_| app_error(Code::MalformedFrame))?;
            self.with_control_conversation(operation.conversation(), |store| {
                store.accept_file_source(&operation, consent)
            })
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
    let source =
        FileSource::imported_label(FileSourceLabel::new(selection.path().to_owned())?, range)?;
    let file = request.file();
    let observation = FileObservation::new(
        file.source_digest(),
        file.source_bytes(),
        file.range(),
        file.digest(),
    )?;
    let version = FileVersion::external(
        OperationId::new(command.operation().into_bytes())?,
        request.artifact(),
        observation,
        preview.fingerprint().map_err(|_| ControlError::InvalidInput)?,
    )?;
    Ok(FileAttachment::new(source, version)?)
}

/// Reconstructs the exact canonical body emitted for accepted captioned imports before
/// descriptor-backed external artifact identity was introduced.
pub(in crate::product_run::workbench) fn domain_legacy_import(
    command: &WorkbenchCommand,
    preview: &WorkbenchFileImportPreview,
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
    let source =
        FileSource::imported_label(FileSourceLabel::new(selection.path().to_owned())?, range)?;
    let file = request.file();
    let observation = FileObservation::new(
        file.source_digest(),
        file.source_bytes(),
        file.range(),
        file.digest(),
    )?;
    let version = FileVersion::new(
        OperationId::new(command.operation().into_bytes())?,
        ArtifactId::new(command.operation().into_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        observation,
        preview.fingerprint().map_err(|_| ControlError::InvalidInput)?,
    )?;
    Ok(FileAttachment::new(source, version)?)
}

pub(in crate::product_run::workbench) fn domain_file_source(
    command: &WorkbenchCommand,
    preview: &WorkbenchFileImportPreview,
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
    let source =
        FileSource::imported_label(FileSourceLabel::new(selection.path().to_owned())?, range)?;
    let file = request.file();
    let observation = FileObservation::new(
        file.source_digest(),
        file.source_bytes(),
        file.range(),
        file.digest(),
    )?;
    let version = FileVersion::external(
        OperationId::new(command.operation().into_bytes())?,
        request.artifact(),
        observation,
        preview.fingerprint().map_err(|_| ControlError::InvalidInput)?,
    )?;
    Ok(FileAttachment::new(source, version)?)
}
