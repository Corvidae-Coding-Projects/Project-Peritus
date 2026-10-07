//! Authenticated image preview and confirmed atomic import; no ambient files or provider sends.

use super::{ProductRunService, domain_operation, error_value};
use crate::{AuthorityHandle, artifact::ArtifactScope};
use peritus_app_protocol::{
    AppErrorCode as Code, AppProtocolError, AppResponsePayload, WorkbenchCommand,
    WorkbenchImagePreview, WorkbenchImageRequest, WorkbenchImageUpload, WorkbenchIntent,
    WorkbenchQuery, WorkbenchReceipt,
};
use peritus_product_runner::{
    attachment::ValidatedImage,
    control::{ControlError, ConversationId},
};
use peritus_types::{ActorId, SessionId};
use std::io::{Read, Seek, SeekFrom};

mod mapping;
mod page;
pub(super) use mapping::domain_image;

const fn error(code: Code) -> AppProtocolError {
    AppProtocolError::new(code, None)
}
pub(super) fn daemon_error(value: crate::DaemonError) -> AppProtocolError {
    error(match value.code_kind() {
        crate::DaemonErrorCode::Unauthorized => Code::ReadOnly,
        crate::DaemonErrorCode::ResourceLimit => Code::LimitExceeded,
        crate::DaemonErrorCode::InvalidInput => Code::InvalidIdentifier,
        _ => Code::NotReady,
    })
}

impl ProductRunService {
    pub(in crate::product_run::workbench) fn image_scope(
        &self,
        actor: ActorId,
        query: WorkbenchQuery,
        revision: u64,
    ) -> Result<ArtifactScope, AppProtocolError> {
        self.control_workspace(query).map_err(error_value)?;
        let conversation = ConversationId::new(query.conversation().into_bytes())
            .map_err(|_| error(Code::MalformedFrame))?;
        self.with_control_conversation(conversation, |store| {
            let record = store
                .load(conversation)?
                .ok_or(ControlError::NotFound)?;
            if record.owner_bytes() != actor.as_bytes()
                || record.workspace_bytes() != query.workspace().as_bytes()
            {
                return Err(ControlError::ScopeMismatch.into());
            }
            if record.revision() != revision {
                return Err(ControlError::StaleRevision.into());
            }
            Ok(ArtifactScope::new(actor, query))
        })
        .map_err(error_value)
    }

    pub(crate) async fn begin_workbench_image_upload(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        session: SessionId,
        request: &WorkbenchImageUpload,
        maximum_chunk_bytes: usize,
    ) -> Result<(), AppProtocolError> {
        let service = self.clone();
        let authority = authority.clone();
        let request = request.clone();
        Self::await_blocking_future("begin durable workbench image upload", move || {
            async move {
                service
                    .begin_workbench_image_upload_owned(
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
        .unwrap_or_else(|_| Err(error(Code::Internal)))
    }

    async fn begin_workbench_image_upload_owned(
        &self,
        authority: AuthorityHandle,
        actor: ActorId,
        session: SessionId,
        request: WorkbenchImageUpload,
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

    pub(crate) async fn preview_workbench_image(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        request: &WorkbenchImageRequest,
    ) -> AppResponsePayload {
        let service = self.clone();
        let authority = authority.clone();
        let request = request.clone();
        Self::await_blocking_future("preview durable workbench image", move || async move {
            service.preview_workbench_image_owned(authority, actor, request).await
        })
        .await
        .unwrap_or_else(|_| AppResponsePayload::Error(error(Code::Internal)))
    }

    async fn preview_workbench_image_owned(
        &self,
        authority: AuthorityHandle,
        actor: ActorId,
        request: WorkbenchImageRequest,
    ) -> AppResponsePayload {
        self.prepare_image(&authority, actor, &request)
            .await
            .map_or_else(AppResponsePayload::Error, |(preview, _)| {
                AppResponsePayload::WorkbenchImagePreview(preview)
            })
    }

    async fn prepare_image(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        request: &WorkbenchImageRequest,
    ) -> Result<(WorkbenchImagePreview, ValidatedImage), AppProtocolError> {
        self.require_workspace_permissions(
            actor,
            request.query(),
            &[peritus_product_runner::control::PermissionCapability::Read],
        )
        .map_err(error_value)?;
        let scope = self.image_scope(actor, request.query(), request.revision())?;
        let provider = self
            .select_provider(request.provider(), request.model())
            .map_err(|_| error(Code::MissingRequiredFeature))?;
        let profile = provider.profile().clone();
        if !profile.capabilities().supports(peritus_model_protocol::Capability::ImageInput) {
            return Err(error(Code::MissingRequiredFeature));
        }
        // A busy decoder rejects before reading bytes. The permit follows the bounded blocking
        // job even if the connection disappears; no detached task can publish control state.
        let permit = self
            .inner
            .image_decodes
            .clone()
            .try_acquire_owned()
            .map_err(|_| error(Code::Backpressure))?;
        let catalog = authority
            .authorize_scoped_artifact(scope, request.artifact())
            .await
            .map_err(daemon_error)?;
        if catalog.byte_size() == 0
            || catalog.byte_size() > profile.limits().max_inline_media_bytes()
        {
            return Err(error(Code::LimitExceeded));
        }
        let reader = peritus_artifact_store::ArtifactStore::open_existing(
            &self.inner.request_source_artifacts,
            peritus_artifact_store::ArtifactDigest::from_sha256(catalog.digest()),
        )
        .map_err(|_| error(Code::NotReady))?;
        if reader.metadata().size() != catalog.byte_size()
            || reader.metadata().digest()
                != peritus_artifact_store::ArtifactDigest::from_sha256(catalog.digest())
        {
            return Err(error(Code::MalformedFrame));
        }
        let decode_profile = profile.clone();
        let artifact = request.artifact();
        let digest = catalog.digest();
        let byte_size = catalog.byte_size();
        let image = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut reader = ArtifactCursor::new(reader);
            ValidatedImage::inspect_artifact(
                &mut reader,
                artifact,
                digest,
                byte_size,
                &decode_profile,
            )
        })
        .await
        .map_err(|_| error(Code::Internal))?
        .map_err(|_| error(Code::MalformedFrame))?;
        if catalog.media_type() != "application/octet-stream"
            && catalog.media_type() != image.media().media_type().as_str()
        {
            return Err(error(Code::MalformedFrame));
        }
        let preview = WorkbenchImagePreview::new(
            request.clone(),
            mapping::metadata(&image)?,
            profile.revision(),
            profile.model().as_str().to_owned(),
        )?;
        self.image_scope(actor, request.query(), request.revision())?;
        Ok((preview, image))
    }

    pub(crate) async fn confirm_workbench_image(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let service = self.clone();
        let authority = authority.clone();
        let command = command.clone();
        Self::await_blocking_future("confirm durable workbench image", move || async move {
            service.confirm_workbench_image_owned(authority, actor, command).await
        })
        .await
        .unwrap_or_else(|_| AppResponsePayload::Error(error(Code::Internal)))
    }

    async fn confirm_workbench_image_owned(
        &self,
        authority: AuthorityHandle,
        actor: ActorId,
        command: WorkbenchCommand,
    ) -> AppResponsePayload {
        self.confirm_image(&authority, actor, &command)
            .await
            .map_or_else(AppResponsePayload::Error, AppResponsePayload::WorkbenchReceipt)
    }

    async fn confirm_image(
        &self,
        authority: &AuthorityHandle,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchReceipt, AppProtocolError> {
        let WorkbenchIntent::AttachImage { preview, .. } = command.intent() else {
            return Err(error(Code::MalformedFrame));
        };
        self.control_workspace(command.query()).map_err(error_value)?;
        let operation = domain_operation(actor, command).map_err(error_value)?;
        let prior = self
            .with_control_conversation(operation.conversation(), |store| store.resolve(&operation))
            .map_err(error_value)?;
        let receipt = if let Some(receipt) = prior {
            receipt
        } else {
            let (current, validated) =
                self.prepare_image(authority, actor, preview.request()).await?;
            if current != *preview {
                return Err(error(Code::StaleRevision));
            }
            if !authority.status().await.map_err(daemon_error)?.mutation_ready() {
                return Err(error(Code::ReadOnly));
            }
            let bytes = current.canonical_bytes().map_err(|_| error(Code::MalformedFrame))?;
            let source_artifacts = self.inner.request_source_artifacts.clone();
            let prepared = self
                .inner
                .control_generation
                .prepare_previewed_image(&operation, &validated, bytes, &source_artifacts)
                .map_err(error_value)?;
            self.with_control_conversation(operation.conversation(), |store| {
                store.accept_prepared_image(prepared)
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

struct ArtifactCursor {
    reader: peritus_artifact_store::ArtifactReadHandle,
    offset: u64,
}

impl ArtifactCursor {
    const fn new(reader: peritus_artifact_store::ArtifactReadHandle) -> Self {
        Self { reader, offset: 0 }
    }
}

impl Read for ArtifactCursor {
    fn read(&mut self, destination: &mut [u8]) -> std::io::Result<usize> {
        if destination.is_empty() {
            return Ok(0);
        }
        let Some(chunk) = self
            .reader
            .read_chunk_at(self.offset, destination.len().min(64 * 1024))
            .map_err(std::io::Error::other)?
        else {
            return Ok(0);
        };
        if chunk.bytes().is_empty() || chunk.bytes().len() > destination.len() {
            return Err(std::io::Error::other(
                "image artifact reader returned an invalid chunk",
            ));
        }
        destination[..chunk.bytes().len()].copy_from_slice(chunk.bytes());
        self.offset = self
            .offset
            .checked_add(u64::try_from(chunk.bytes().len()).map_err(std::io::Error::other)?)
            .ok_or_else(|| std::io::Error::other("image artifact offset overflow"))?;
        Ok(chunk.bytes().len())
    }
}

impl Seek for ArtifactCursor {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let length = self.reader.metadata().size();
        let offset = match position {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(delta) => checked_seek(length, delta)?,
            SeekFrom::Current(delta) => checked_seek(self.offset, delta)?,
        };
        if offset > length {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "image artifact seek exceeds its immutable length",
            ));
        }
        self.offset = offset;
        Ok(offset)
    }
}

fn checked_seek(base: u64, delta: i64) -> std::io::Result<u64> {
    let value = i128::from(base) + i128::from(delta);
    u64::try_from(value).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "image artifact seek is outside its immutable length",
        )
    })
}
