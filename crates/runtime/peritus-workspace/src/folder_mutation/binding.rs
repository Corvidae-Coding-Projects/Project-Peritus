//! Registered-folder identity, revision, and canonical action-payload binding.

use peritus_codec::CodecLimits;
use peritus_patch::PatchSet;
use peritus_policy::{ActorRole, OperationClass};
use peritus_protocol::ActionIntentDto;
use peritus_types::{ActionId, ActorId, CapabilityName, EnvironmentId, ResourceId, RevisionTuple};

use crate::{
    ErrorCode, FolderIdentity, RecoveryClass, WorkspaceError, WorkspaceOperation,
    consumption::ActionConsumptionBinding,
};

use super::FolderMutationGateway;

const FOLDER_PATCH_CAPABILITY: &str = "workspace.folder.patch";
const FOLDER_PATCH_MEDIA_TYPE: &str = "application/vnd.peritus.workspace-operation.v1";

pub fn folder_patch_payload(gateway: &FolderMutationGateway, patch: &PatchSet) -> Vec<u8> {
    folder_patch_payload_for(
        gateway.workspace_id(),
        gateway.resource_id,
        gateway.environment_id,
        gateway.identity.digest(),
        gateway.generation(),
        gateway.revision(),
        patch,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "the canonical payload binds each independently authenticated target fact"
)]
pub fn folder_patch_payload_for(
    workspace_id: peritus_types::WorkspaceId,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
    folder_identity: peritus_types::Sha256Digest,
    generation: peritus_types::Generation,
    revision: peritus_types::RevisionNumber,
    patch: &PatchSet,
) -> Vec<u8> {
    let mut bytes = b"PERITUS-FOLDER-PATCH-V1\0".to_vec();
    bytes.extend_from_slice(workspace_id.as_bytes());
    bytes.extend_from_slice(resource_id.as_bytes());
    bytes.extend_from_slice(environment_id.as_bytes());
    bytes.extend_from_slice(folder_identity.as_bytes());
    bytes.extend_from_slice(&generation.get().to_be_bytes());
    bytes.extend_from_slice(&revision.get().to_be_bytes());
    bytes.extend_from_slice(patch.identity().as_bytes());
    bytes
}

/// Reconstructs the one canonical action-intent shape accepted for a registered-folder patch.
///
/// # Errors
/// Returns an indeterminate C1 error if the compiled capability cannot be represented.
pub fn folder_patch_action_intent(
    actor_id: ActorId,
    action_id: ActionId,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
    payload: Vec<u8>,
) -> Result<ActionIntentDto, WorkspaceError> {
    let capability_name = CapabilityName::new(FOLDER_PATCH_CAPABILITY.to_owned())
        .map_err(|_| canonical_intent_error())?;
    Ok(ActionIntentDto {
        action_id,
        actor_id,
        role: ActorRole::ProviderToolWorker,
        environment_id,
        resource_id,
        capability_name,
        operation_class: OperationClass::WorkspaceMutation,
        media_type: FOLDER_PATCH_MEDIA_TYPE.to_owned(),
        payload,
    })
}

pub fn folder_patch_action_digest(
    actor_id: ActorId,
    action_id: ActionId,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
    payload: Vec<u8>,
) -> Result<peritus_types::Sha256Digest, WorkspaceError> {
    folder_patch_action_intent(actor_id, action_id, environment_id, resource_id, payload)?
        .digest(CodecLimits::PRODUCTION)
        .map_err(|_| canonical_intent_error())
}

const fn canonical_intent_error() -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Indeterminate,
        WorkspaceOperation::Authorize,
        RecoveryClass::Quarantine,
        "canonical folder-patch action intent cannot be reconstructed",
    )
}

pub fn revalidate_identity(
    identity: &FolderIdentity,
    operation: WorkspaceOperation,
) -> Result<(), WorkspaceError> {
    crate::FolderInspection::open(identity).map(|_| ()).map_err(|_| {
        WorkspaceError::new(
            ErrorCode::ResourceMismatch,
            operation,
            RecoveryClass::Reobserve,
            "registered folder identity changed",
        )
    })
}

pub const fn action_binding(
    revision: RevisionTuple,
    resource_id: ResourceId,
    environment_id: EnvironmentId,
) -> ActionConsumptionBinding {
    ActionConsumptionBinding::new(
        revision.workspace_id(),
        resource_id,
        environment_id,
        revision.workspace_generation(),
        revision.workspace_revision(),
    )
}
