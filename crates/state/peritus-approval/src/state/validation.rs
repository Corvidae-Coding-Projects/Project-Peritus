//! Exact reducer-side revalidation of checked authentication observations.

use peritus_policy::ActorRole;
use vstd::prelude::*;

use super::independence::has_violation;

verus! {

pub(super) open spec fn same_validity(
    left: peritus_policy::ValidityWindow,
    right: peritus_policy::ValidityWindow,
) -> bool {
    left.spec_not_before().spec_epoch() == right.spec_not_before().spec_epoch()
        && left.spec_not_before().spec_tick_millis()
            == right.spec_not_before().spec_tick_millis()
        && left.spec_expires_at().spec_epoch() == right.spec_expires_at().spec_epoch()
        && left.spec_expires_at().spec_tick_millis()
            == right.spec_expires_at().spec_tick_millis()
}

const fn validity_values_equal(
    left: peritus_policy::ValidityWindow,
    right: peritus_policy::ValidityWindow,
) -> (result: bool)
    ensures result == same_validity(left, right),
{
    let left_start = left.not_before();
    let right_start = right.not_before();
    let left_end = left.expires_at();
    let right_end = right.expires_at();
    left_start.epoch().get() == right_start.epoch().get()
        && left_start.tick_millis() == right_start.tick_millis()
        && left_end.epoch().get() == right_end.epoch().get()
        && left_end.tick_millis() == right_end.tick_millis()
}

const fn is_human_authority(role: ActorRole) -> (result: bool)
    ensures result == (role == ActorRole::HumanAuthority),
{
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

pub(super) open spec fn checked_observation_error(
    request: &crate::ApprovalRequest,
    observation: &crate::AuthenticatedApprovalObservation,
    registry: &crate::CredentialRegistrySnapshot,
) -> Option<crate::ApprovalError> {
    if !super::exact::same_identifier_from(
        observation.request_id.spec_bytes(),
        request.request_id.spec_bytes(),
        0,
    ) || !super::exact::same_digest_from(
        observation.request_digest.spec_bytes(),
        request.digest.spec_bytes(),
        0,
    ) {
        Some(crate::ApprovalError::BindingMismatch(crate::ScopeDimension::Request))
    } else if observation.registry_revision.spec_value() != registry.spec_revision().spec_value() {
        Some(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::RegistryRevision,
        ))
    } else {
        match registry.spec_credential(observation.key_id) {
            None => Some(crate::ApprovalError::CredentialMissing),
            Some(credential) => credential_observation_error(request, observation, &credential),
        }
    }
}

pub(super) open spec fn credential_observation_error(
    request: &crate::ApprovalRequest,
    observation: &crate::AuthenticatedApprovalObservation,
    credential: &crate::ApproverCredential,
) -> Option<crate::ApprovalError> {
    if credential.spec_status() != crate::CredentialStatus::Enabled {
                    Some(crate::ApprovalError::CredentialMismatch(
                        crate::CredentialDimension::Status,
                    ))
                } else if credential.spec_generation().spec_value()
                    != observation.credential_generation.spec_value()
                {
                    Some(crate::ApprovalError::CredentialMismatch(
                        crate::CredentialDimension::Generation,
                    ))
                } else if !super::exact::same_identifier_from(
                    credential.spec_actor().spec_bytes(),
                    observation.responder.spec_bytes(),
                    0,
                ) {
                    Some(crate::ApprovalError::CredentialMismatch(
                        crate::CredentialDimension::Actor,
                    ))
                } else if credential.spec_principal_role() != ActorRole::HumanAuthority {
                    Some(crate::ApprovalError::CredentialMismatch(
                        crate::CredentialDimension::PrincipalRole,
                    ))
                } else if !crate::authentication::spec_contains_role(
                    request.spec_requirement().spec_approver_roles(),
                    observation.approver_role,
                ) || !crate::authentication::spec_contains_role(
                    credential.spec_allowed_approval_roles(),
                    observation.approver_role,
                ) {
                    Some(crate::ApprovalError::CredentialMismatch(
                        crate::CredentialDimension::ApprovalRole,
                    ))
                } else if !super::exact::same_identifier_from(
                    credential.spec_environment().spec_bytes(),
                    request.spec_scope().spec_environment_id(),
                    0,
                ) {
                    Some(crate::ApprovalError::CredentialMismatch(
                        crate::CredentialDimension::Environment,
                    ))
                } else if !super::exact::same_identifier_from(
                    credential.spec_workspace().spec_bytes(),
                    request.spec_scope().spec_revision().spec_workspace_id().spec_bytes(),
                    0,
                ) {
                    Some(crate::ApprovalError::CredentialMismatch(
                        crate::CredentialDimension::Workspace,
                    ))
                } else if !credential
                    .spec_maximum_tier()
                    .spec_at_least(request.spec_requirement().spec_minimum_tier())
                {
                    Some(crate::ApprovalError::CredentialMismatch(
                        crate::CredentialDimension::AuthorityTier,
                    ))
                } else if !same_validity(
                    credential.spec_validity(),
                    observation.credential_validity,
                ) {
                    Some(crate::ApprovalError::CredentialMismatch(
                        crate::CredentialDimension::Validity,
                    ))
                } else if first_violation(
                    request,
                    observation.responder,
                    request.spec_requirement().spec_independence(),
                    0,
                ) {
                    Some(crate::ApprovalError::IndependenceViolation)
                } else {
                    None
                }
}

fn checked_credential(
    request: &crate::ApprovalRequest,
    observation: &crate::AuthenticatedApprovalObservation,
    credential: &crate::ApproverCredential,
) -> (result: Result<(), crate::ApprovalError>)
    ensures
        match result {
            Ok(()) => credential_observation_error(request, observation, credential).is_none(),
            Err(error) => {
                credential_observation_error(request, observation, credential) == Some(error)
            }
        },
{
    proof { reveal_with_fuel(credential_observation_error, 1); }
    let status = credential.status();
    proof { assert(status == credential.spec_status()); }
    match status {
        crate::CredentialStatus::Enabled => {},
        crate::CredentialStatus::Disabled => {
            return Err(crate::ApprovalError::CredentialMismatch(
                crate::CredentialDimension::Status,
            ));
        }
    }
    if credential.generation().get() != observation.credential_generation.get() {
        return Err(crate::ApprovalError::CredentialMismatch(crate::CredentialDimension::Generation));
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
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::Validity,
        ));
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
) -> (result: Result<(), crate::ApprovalError>)
    ensures
        match result {
            Ok(()) => checked_observation_error(request, observation, registry).is_none(),
            Err(error) => checked_observation_error(request, observation, registry) == Some(error),
        },
{
    if !super::exact::identifier_bytes_equal(
        *observation.request_id.as_bytes(),
        *request.request_id.as_bytes(),
    ) || !super::exact::digest_bytes_equal(
        *observation.request_digest.sha256().as_bytes(),
        *request.digest.sha256().as_bytes(),
    )
    {
        return Err(crate::ApprovalError::BindingMismatch(crate::ScopeDimension::Request));
    }
    if observation.registry_revision.get() != registry.revision().get() {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::RegistryRevision,
        ));
    }
    let credential = match registry.credential(observation.key_id) {
        Some(credential) => {
            proof {
                assert(registry.spec_credential(observation.key_id) == Some(*credential));
            }
            credential
        }
        None => {
            proof { assert(registry.spec_credential(observation.key_id).is_none()); }
            return Err(crate::ApprovalError::CredentialMissing);
        }
    };
    if let Err(error) = checked_credential(request, observation, credential) {
        return Err(error);
    }
    Ok(())
}

} // verus!
