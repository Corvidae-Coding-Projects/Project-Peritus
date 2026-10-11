//! Exact reducer-side revalidation of checked authentication observations.

use peritus_policy::ActorRole;

use super::independence::has_violation;

const fn validity_values_equal(
    left: peritus_policy::ValidityWindow,
    right: peritus_policy::ValidityWindow,
) -> bool {
    let left_start = left.not_before();
    let right_start = right.not_before();
    let left_end = left.expires_at();
    let right_end = right.expires_at();
    left_start.epoch().get() == right_start.epoch().get()
        && left_start.tick_millis() == right_start.tick_millis()
        && left_end.epoch().get() == right_end.epoch().get()
        && left_end.tick_millis() == right_end.tick_millis()
}
const fn is_human_authority(role: ActorRole) -> bool {
    match role {
        ActorRole::HumanAuthority => true,
        ActorRole::Writer
        | ActorRole::Fixer
        | ActorRole::Reviewer
        | ActorRole::Evaluator
        | ActorRole::GateRunner
        | ActorRole::Orchestrator
        | ActorRole::EvolutionAgent
        | ActorRole::DaemonService
        | ActorRole::ProviderToolWorker
        | ActorRole::Plugin => false,
    }
}
fn checked_credential(
    request: &crate::ApprovalRequest,
    observation: &crate::AuthenticatedApprovalObservation,
    credential: &crate::ApproverCredential,
) -> Result<(), crate::ApprovalError> {
    let status = credential.status();
    match status {
        crate::CredentialStatus::Enabled => {}
        crate::CredentialStatus::Disabled => {
            return Err(crate::ApprovalError::CredentialMismatch(
                crate::CredentialDimension::Status,
            ));
        }
    }
    if credential.generation().get() != observation.credential_generation.get() {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::Generation,
        ));
    }
    if !super::exact::identifier_bytes_equal(
        *credential.actor().as_bytes(),
        *observation.responder.as_bytes(),
    ) {
        return Err(crate::ApprovalError::CredentialMismatch(crate::CredentialDimension::Actor));
    }
    if !is_human_authority(credential.principal_role()) {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::PrincipalRole,
        ));
    }
    let role = observation.approver_role;
    if !crate::authentication::contains_role(request.requirement().approver_roles(), role)
        || !crate::authentication::contains_role(credential.allowed_approval_roles(), role)
    {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::ApprovalRole,
        ));
    }
    if !super::exact::identifier_bytes_equal(
        *credential.environment().as_bytes(),
        *request.scope().environment_id().as_bytes(),
    ) {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::Environment,
        ));
    }
    if !super::exact::identifier_bytes_equal(
        *credential.workspace().as_bytes(),
        *request.scope().revision().workspace_id().as_bytes(),
    ) {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::Workspace,
        ));
    }
    if !credential.maximum_tier().at_least(request.requirement().minimum_tier()) {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::AuthorityTier,
        ));
    }
    if !validity_values_equal(credential.validity(), observation.credential_validity) {
        return Err(crate::ApprovalError::CredentialMismatch(crate::CredentialDimension::Validity));
    }
    if has_violation(request, observation.responder) {
        return Err(crate::ApprovalError::IndependenceViolation);
    }
    Ok(())
}
pub(super) fn checked_observation(
    request: &crate::ApprovalRequest,
    observation: &crate::AuthenticatedApprovalObservation,
    registry: &crate::CredentialRegistrySnapshot,
) -> Result<(), crate::ApprovalError> {
    if !super::exact::identifier_bytes_equal(
        *observation.request_id.as_bytes(),
        *request.request_id.as_bytes(),
    ) || !super::exact::digest_bytes_equal(
        *observation.request_digest.sha256().as_bytes(),
        *request.digest.sha256().as_bytes(),
    ) {
        return Err(crate::ApprovalError::BindingMismatch(crate::ScopeDimension::Request));
    }
    if observation.registry_revision.get() != registry.revision().get() {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::RegistryRevision,
        ));
    }
    let Some(credential) = registry.credential(observation.key_id) else {
        return Err(crate::ApprovalError::CredentialMissing);
    };
    checked_credential(request, observation, credential)
}
