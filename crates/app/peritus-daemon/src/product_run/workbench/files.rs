//! Authorized workspace-file preview, confirmation and retained selection inspection.

use super::{Error, ProductRunService, domain_operation, error_response, error_value};
use peritus_app_protocol::{
    AppErrorCode as Code, AppProtocolError, AppResponsePayload, WorkbenchCommand,
    WorkbenchFileMetadata, WorkbenchFilePreview, WorkbenchFileRequest, WorkbenchIntent,
    WorkbenchQuery,
};
use peritus_product_runner::{
    attachment::ValidatedFileText,
    control::{ControlError, ConversationId, ConversationRecord},
};
use peritus_types::ActorId;
use peritus_workspace::{FileReadSelection, FolderIdentity, FolderInspection};

mod import;
mod mapping;
mod page;
pub(in crate::product_run) mod refresh;
pub(super) use import::{domain_import, domain_import_part};
pub(super) use mapping::domain_file;

pub(in crate::product_run) type FilePreviewCache =
    std::collections::BTreeMap<([u8; 16], [u8; 16]), FilePreviewSnapshot>;

const fn app_error(code: Code) -> AppProtocolError {
    AppProtocolError::new(code, None)
}

impl ProductRunService {
    fn file_record(
        &self,
        actor: ActorId,
        query: WorkbenchQuery,
        revision: u64,
    ) -> Result<ConversationRecord, Error> {
        self.control_workspace(query)?;
        let record = self
            .with_controls(false, |store| {
                store.load(ConversationId::new(query.conversation().into_bytes())?)
            })?
            .ok_or(ControlError::NotFound)?;
        if record.owner_bytes() != actor.as_bytes()
            || record.workspace_bytes() != query.workspace().as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        if record.revision() != revision {
            return Err(ControlError::StaleRevision.into());
        }
        Ok(record)
    }

    pub(crate) async fn preview_workbench_file(
        &self,
        actor: ActorId,
        request: &WorkbenchFileRequest,
    ) -> AppResponsePayload {
        let service = self.clone();
        let request = request.clone();
        let permit = match self.inner.file_reads.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return AppResponsePayload::Error(app_error(Code::Backpressure)),
        };
        let cancelled = CancelRead::default();
        let flag = cancelled.0.clone();
        match tokio::task::spawn_blocking(move || {
            let _permit = permit;
            service.prepare_file_cancellable(actor, &request, || {
                flag.load(std::sync::atomic::Ordering::Acquire)
            })
        })
        .await
        {
            Ok(result) => result
                .and_then(|(preview, text)| {
                    let key =
                        (*actor.as_bytes(), *preview.request().query().conversation().as_bytes());
                    self.inner
                        .file_previews
                        .lock()
                        .map_err(|_| app_error(Code::Internal))?
                        .insert(key, FilePreviewSnapshot { preview: preview.clone(), text });
                    Ok(preview)
                })
                .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchFilePreview),
            Err(_) => AppResponsePayload::Error(app_error(Code::Internal)),
        }
    }

    fn prepare_file(
        &self,
        actor: ActorId,
        request: &WorkbenchFileRequest,
    ) -> Result<(WorkbenchFilePreview, ValidatedFileText), AppProtocolError> {
        self.prepare_file_cancellable(actor, request, || false)
    }

    fn prepare_file_cancellable(
        &self,
        actor: ActorId,
        request: &WorkbenchFileRequest,
        cancelled: impl FnMut() -> bool,
    ) -> Result<(WorkbenchFilePreview, ValidatedFileText), AppProtocolError> {
        self.require_workspace_permissions(
            actor,
            request.query(),
            &[peritus_product_runner::control::PermissionCapability::Read],
        )
        .map_err(error_value)?;
        let record =
            self.file_record(actor, request.query(), request.revision()).map_err(error_value)?;
        let provider = self
            .select_provider(request.provider(), request.model())
            .map_err(|_| app_error(Code::MissingRequiredFeature))?;
        let profile = provider.profile();
        let root = self
            .inner
            .workspaces
            .get(&request.query().workspace())
            .ok_or_else(|| app_error(Code::SessionMismatch))?;
        let identity = FolderIdentity::observe(root).map_err(|_| app_error(Code::NotReady))?;
        let protected = self.protected_paths(request.query()).map_err(error_value)?;
        if let Some(folder) = self.inner.folders.get(&request.query().workspace()) {
            folder.verify().map_err(|_| app_error(Code::NotReady))?;
        }
        let contract = self
            .with_controls(false, |store| store.user_instruction_context(&record))
            .map_err(error_value)?;
        peritus_product_runner::checked_protected_file(
            identity.root(),
            request.path(),
            &contract,
            &protected,
        )
        .map_err(|_| app_error(Code::ReadOnly))?;
        let path = peritus_patch::WorkspacePath::new(request.path())
            .map_err(|_| app_error(Code::InvalidIdentifier))?;
        let selection = match request.range() {
            peritus_app_protocol::WorkbenchFileRange::All => Ok(FileReadSelection::all()),
            peritus_app_protocol::WorkbenchFileRange::Bytes { start, end } => {
                FileReadSelection::bytes(start, end)
            }
            peritus_app_protocol::WorkbenchFileRange::Lines { first, last } => {
                FileReadSelection::lines(u64::from(first), u64::from(last))
            }
        }
        .map_err(|_| app_error(Code::MalformedFrame))?;
        let inspected = FolderInspection::open(&identity)
            .and_then(|reader| reader.read_file_cancellable(&path, selection, u64::MAX, cancelled))
            .map_err(|error| {
                AppProtocolError::new(
                    Code::InvalidIdentifier,
                    peritus_app_protocol::AppDiagnostic::new(
                        format!("{}: {error}", request.path()),
                        4096,
                    )
                    .ok(),
                )
            })?
            .ok_or_else(|| {
                AppProtocolError::new(
                    Code::NotReady,
                    peritus_app_protocol::AppDiagnostic::new(
                        format!("{}: source changed during read", request.path()),
                        4096,
                    )
                    .ok(),
                )
            })?;
        let text = ValidatedFileText::new(inspected.bytes().to_vec())
            .map_err(|_| app_error(Code::MalformedFrame))?;
        let metadata = WorkbenchFileMetadata::new(
            inspected.source_digest(),
            inspected.source_bytes(),
            inspected.range(),
            inspected.digest(),
        )?;
        let preview = WorkbenchFilePreview::new(
            request.clone(),
            identity.digest(),
            metadata,
            profile.revision(),
            profile.model().as_str().to_owned(),
        )?;
        self.file_record(actor, request.query(), request.revision()).map_err(error_value)?;
        if let Some(folder) = self.inner.folders.get(&request.query().workspace()) {
            folder.verify().map_err(|_| app_error(Code::NotReady))?;
        }
        Ok((preview, text))
    }

    pub(crate) async fn confirm_workbench_file(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let service = self.clone();
        let command = command.clone();
        let permit = match self.inner.file_reads.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return AppResponsePayload::Error(app_error(Code::Backpressure)),
        };
        let cancelled = CancelRead::default();
        let flag = cancelled.0.clone();
        match tokio::task::spawn_blocking(move || {
            let _permit = permit;
            service
                .confirm_file(actor, &command, || flag.load(std::sync::atomic::Ordering::Acquire))
        })
        .await
        {
            Ok(result) => {
                result.map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchReceipt)
            }
            Err(_) => AppResponsePayload::Error(app_error(Code::Internal)),
        }
    }
    fn confirm_file(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        cancelled: impl FnMut() -> bool,
    ) -> Result<peritus_app_protocol::WorkbenchReceipt, AppProtocolError> {
        let WorkbenchIntent::AttachFile { preview, .. } = command.intent() else {
            return Err(app_error(Code::MalformedFrame));
        };
        self.control_workspace(command.query()).map_err(error_value)?;
        let operation = domain_operation(actor, command).map_err(error_value)?;
        let prior =
            self.with_controls(false, |store| store.resolve(&operation)).map_err(error_value)?;
        let receipt = if let Some(receipt) = prior {
            receipt
        } else {
            let key = (*actor.as_bytes(), *preview.request().query().conversation().as_bytes());
            let cached = self
                .inner
                .file_previews
                .lock()
                .map_err(|_| app_error(Code::Internal))?
                .remove(&key)
                .filter(|cached| cached.preview == *preview);
            let (current, text) = if let Some(cached) = cached {
                self.validate_cached_file(actor, preview)?;
                (cached.preview, cached.text)
            } else {
                self.prepare_file_cancellable(actor, preview.request(), cancelled)?
            };
            if current != *preview {
                return Err(app_error(Code::StaleRevision));
            }
            let consent = preview.canonical_bytes().map_err(|_| app_error(Code::MalformedFrame))?;
            self.with_controls(false, |store| store.accept_file(&operation, &text, consent))
                .map_err(error_value)?
        };
        peritus_app_protocol::WorkbenchReceipt::new(
            command.operation(),
            command.query(),
            receipt.accepted_revision(),
            receipt.payload_digest(),
        )
    }
    fn validate_cached_file(
        &self,
        actor: ActorId,
        preview: &WorkbenchFilePreview,
    ) -> Result<(), AppProtocolError> {
        let request = preview.request();
        self.require_workspace_permissions(
            actor,
            request.query(),
            &[peritus_product_runner::control::PermissionCapability::Read],
        )
        .map_err(error_value)?;
        let record =
            self.file_record(actor, request.query(), request.revision()).map_err(error_value)?;
        let provider = self
            .select_provider(request.provider(), request.model())
            .map_err(|_| app_error(Code::MissingRequiredFeature))?;
        if provider.profile().revision() != preview.provider_revision()
            || provider.profile().model().as_str() != preview.resolved_model()
        {
            return Err(app_error(Code::StaleRevision));
        }
        let root = self
            .inner
            .workspaces
            .get(&request.query().workspace())
            .ok_or_else(|| app_error(Code::SessionMismatch))?;
        let identity = FolderIdentity::observe(root).map_err(|_| app_error(Code::NotReady))?;
        if identity.digest() != preview.folder() {
            return Err(app_error(Code::StaleRevision));
        }
        if let Some(folder) = self.inner.folders.get(&request.query().workspace()) {
            folder.verify().map_err(|_| app_error(Code::NotReady))?;
        }
        let protected = self.protected_paths(request.query()).map_err(error_value)?;
        let contract = self
            .with_controls(false, |store| store.user_instruction_context(&record))
            .map_err(error_value)?;
        peritus_product_runner::checked_protected_file(
            identity.root(),
            request.path(),
            &contract,
            &protected,
        )
        .map_err(|_| app_error(Code::ReadOnly))?;
        Ok(())
    }
}

/// Cancels an in-flight text read when its waiting request is dropped.
#[derive(Default)]
struct CancelRead(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for CancelRead {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
    }
}

/// Only the latest preview per actor/conversation is cached; eviction never rejects admission.
pub(in crate::product_run) struct FilePreviewSnapshot {
    preview: WorkbenchFilePreview,
    text: ValidatedFileText,
}
