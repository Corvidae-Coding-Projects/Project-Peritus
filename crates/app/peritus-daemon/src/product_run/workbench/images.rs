//! Authenticated image preview and confirmed atomic import; no ambient files or provider sends.

use super::{ProductRunService, domain_operation, error_value};
use crate::{AuthorityHandle, artifact::ArtifactScope};
use peritus_app_protocol::{
    AppErrorCode as Code, AppProtocolError, AppResponsePayload, WorkbenchCommand,
    WorkbenchImagePreview, WorkbenchImageRequest, WorkbenchImageUpload, WorkbenchIntent,
    WorkbenchQuery, WorkbenchReceipt,
};
use peritus_product_runner::{
    attachment::{MAX_IMAGE_BYTES, ValidatedImage},
    control::{ControlError, ConversationId},
};
use peritus_types::{ActorId, SessionId};

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
        self.with_controls(false, |store| {
            let record = store
                .load(ConversationId::new(query.conversation().into_bytes())?)?
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
        self.prepare_image(authority, actor, request)
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
        let (catalog, bytes) = authority
            .read_scoped_artifact(
                scope,
                request.artifact(),
                MAX_IMAGE_BYTES.min(profile.limits().max_inline_media_bytes()),
            )
            .await
            .map_err(daemon_error)?;
        let decode_profile = profile.clone();
        let image = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            ValidatedImage::decode(bytes, &decode_profile)
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
        self.confirm_image(authority, actor, command)
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
        let prior =
            self.with_controls(false, |store| store.resolve(&operation)).map_err(error_value)?;
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
            self.with_controls(false, |store| {
                store.accept_previewed_image(&operation, &validated, bytes)
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
