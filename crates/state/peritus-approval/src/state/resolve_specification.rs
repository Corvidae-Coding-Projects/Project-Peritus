//! Closed total exact relation for authenticated approval resolution.

#[cfg(verus_only)]
use peritus_policy::AuthorityInstant;
use vstd::prelude::*;

verus! {

pub(super) open spec fn earliest_result(
    left: AuthorityInstant,
    right: AuthorityInstant,
) -> Result<AuthorityInstant, crate::ApprovalError> {
    if left.spec_epoch() != right.spec_epoch() {
        Err(crate::ApprovalError::ClockEpochMismatch)
    } else if left.spec_tick_millis() <= right.spec_tick_millis() {
        Ok(left)
    } else {
        Ok(right)
    }
}

pub(super) open spec fn observation_expiry_result(
    request: &crate::ApprovalRequest,
    observation: &crate::AuthenticatedApprovalObservation,
) -> Result<AuthorityInstant, crate::ApprovalError> {
    match earliest_result(
        request.spec_validity().spec_expires_at(),
        request.spec_scope().spec_validity().spec_expires_at(),
    ) {
        Err(error) => Err(error),
        Ok(first) => match earliest_result(
            first,
            request.spec_requirement().spec_validity().spec_expires_at(),
        ) {
            Err(error) => Err(error),
            Ok(second) => match earliest_result(
                second,
                observation.credential_validity.spec_expires_at(),
            ) {
                Err(error) => Err(error),
                Ok(third) => earliest_result(third, observation.decision_expires_at),
            },
        },
    }
}

pub(super) open spec fn retained_resolution(
    state: super::ApprovalState,
) -> Option<super::Resolution> {
    match state {
        super::ApprovalState::ApprovedOnce(value)
        | super::ApprovalState::AmendmentAuthorized(value)
        | super::ApprovalState::Consumed(value)
        | super::ApprovalState::Amended(value)
        | super::ApprovalState::Denied(value) => Some(value),
        super::ApprovalState::Expired(value) => value,
        super::ApprovalState::Pending | super::ApprovalState::Cancelled => None,
    }
}

pub(super) open spec fn pending_resolve_error(
    before: &super::ApprovalAggregate,
    observation: &crate::AuthenticatedApprovalObservation,
    registry: &crate::CredentialRegistrySnapshot,
) -> Option<crate::ApprovalError> {
    match super::validation::checked_observation_error(&before.request, observation, registry) {
        Some(error) => Some(error),
        None => match observation_expiry_result(&before.request, observation) {
            Err(error) => Some(error),
            Ok(expiry) => match before.request.spec_observation_time_error(observation.observed_at) {
                Some(error) => Some(error),
                None => {
                    if observation.observed_at.spec_tick_millis()
                        >= expiry.spec_tick_millis()
                    {
                        Some(crate::ApprovalError::Expired)
                    } else {
                        None
                    }
                }
            },
        },
    }
}

pub(super) open spec fn resolve_expected_error(
    before: &super::ApprovalAggregate,
    observation: &crate::AuthenticatedApprovalObservation,
    registry: &crate::CredentialRegistrySnapshot,
) -> Option<crate::ApprovalError> {
    match before.state {
        super::ApprovalState::Pending => pending_resolve_error(before, observation, registry),
        _ => match retained_resolution(before.state) {
            None => Some(crate::ApprovalError::AlreadyResolved),
            Some(resolution) => {
                if !super::exact::same_digest_from(
                    resolution.decision_digest.spec_bytes(),
                    observation.decision_digest.spec_bytes(),
                    0,
                ) {
                    Some(crate::ApprovalError::AlreadyResolved)
                } else {
                    super::validation::checked_observation_error(
                        &before.request,
                        observation,
                        registry,
                    )
                }
            }
        },
    }
}

pub(super) open spec fn exact_resolution(
    observation: &crate::AuthenticatedApprovalObservation,
    valid_until: AuthorityInstant,
) -> super::Resolution {
    super::Resolution {
        decision_digest: observation.decision_digest,
        command_id: observation.command_id,
        choice: observation.choice,
        registry_revision: observation.registry_revision,
        credential_generation: observation.credential_generation,
        valid_until,
    }
}

pub(super) open spec fn state_from_resolution(
    resolution: super::Resolution,
) -> super::ApprovalState {
    match resolution.choice {
        crate::ApprovalChoice::Deny => super::ApprovalState::Denied(resolution),
        crate::ApprovalChoice::ApproveOnce => super::ApprovalState::ApprovedOnce(resolution),
        crate::ApprovalChoice::Amend(_) => {
            super::ApprovalState::AmendmentAuthorized(resolution)
        }
    }
}

pub(super) open spec fn pending_success_is_exact(
    before: &super::ApprovalAggregate,
    observation: &crate::AuthenticatedApprovalObservation,
    outcome: &super::ApprovalTransitionOutcome,
) -> bool {
    match observation_expiry_result(&before.request, observation) {
        Err(_) => false,
        Ok(valid_until) => {
            let resolution = exact_resolution(observation, valid_until);
            super::exact::request_is_exact_advance(
                &outcome.aggregate.request,
                &before.request,
                observation.observed_at,
            )
                && outcome.aggregate.state == state_from_resolution(resolution)
                && outcome.transition.kind == super::ApprovalTransitionKind::Resolved
                && outcome.transition.from == crate::ApprovalPhase::Pending
                && outcome.transition.to == super::exact::state_phase(
                    state_from_resolution(resolution),
                )
                && outcome.transition.decision_digest == Some(observation.decision_digest)
                && outcome.transition.registry_revision == Some(observation.registry_revision)
                && crate::model::inv_009(outcome.spec_model())
        }
    }
}

pub(super) open spec fn replay_success_is_exact(
    before: &super::ApprovalAggregate,
    observation: &crate::AuthenticatedApprovalObservation,
    outcome: &super::ApprovalTransitionOutcome,
) -> bool {
    outcome.aggregate == *before
        && outcome.transition.kind == super::ApprovalTransitionKind::Idempotent
        && outcome.transition.from == before.spec_phase()
        && outcome.transition.to == before.spec_phase()
        && outcome.transition.decision_digest == Some(observation.decision_digest)
        && outcome.transition.registry_revision == Some(observation.registry_revision)
        && crate::model::inv_009(outcome.spec_model())
}

pub(super) open spec fn resolve_success_is_exact(
    before: &super::ApprovalAggregate,
    observation: &crate::AuthenticatedApprovalObservation,
    outcome: &super::ApprovalTransitionOutcome,
) -> bool {
    match before.state {
        super::ApprovalState::Pending => pending_success_is_exact(before, observation, outcome),
        _ => replay_success_is_exact(before, observation, outcome),
    }
}

pub(super) open spec fn resolve_result_relation(
    before: &super::ApprovalAggregate,
    observation: &crate::AuthenticatedApprovalObservation,
    registry: &crate::CredentialRegistrySnapshot,
    result: &Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>,
) -> bool {
    match resolve_expected_error(before, observation, registry) {
        Some(expected) => match result {
            Err(failure) => {
                failure.error == expected
                    && failure.aggregate == *before
                    && failure.observation == Some(*observation)
            }
            Ok(_) => false,
        },
        None => match result {
            Ok(outcome) => resolve_success_is_exact(before, observation, outcome),
            Err(_) => false,
        },
    }
}

impl super::ApprovalAggregate {
    /// Closed total exact contract for authenticated resolution and digest-semantic replay.
    pub closed spec fn spec_resolve_result_is_exact(
        &self,
        observation: &crate::AuthenticatedApprovalObservation,
        registry: &crate::CredentialRegistrySnapshot,
        result: &Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>,
    ) -> bool {
        resolve_result_relation(self, observation, registry, result)
    }
}

pub(super) proof fn close_resolve_relation(
    before: &super::ApprovalAggregate,
    observation: &crate::AuthenticatedApprovalObservation,
    registry: &crate::CredentialRegistrySnapshot,
    result: &Result<super::ApprovalTransitionOutcome, crate::ApprovalTransitionFailure>,
)
    requires resolve_result_relation(before, observation, registry, result),
    ensures before.spec_resolve_result_is_exact(observation, registry, result),
{
}

} // verus!
