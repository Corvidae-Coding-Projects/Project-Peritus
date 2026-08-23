//! Ordered first-error and total-decision model for policy/lease authority intersection.

#[cfg(verus_only)]
use crate::state::LeaseState;
#[cfg(verus_only)]
use crate::{LeaseAggregate, LeaseError, PolicyIntersectionDimension, UseLease};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn policy_intersection_error(
    aggregate: &LeaseAggregate,
    command: &UseLease,
) -> Option<LeaseError> {
    let policy = &command.capability_use;
    if !super::super::identity::concrete_instant_matches(
        policy.spec_used_at(),
        command.observed_at,
    ) {
        Some(LeaseError::PolicyIntersectionMismatch(
            PolicyIntersectionDimension::ClockEpoch,
        ))
    } else if !super::super::identity::concrete_identifier_matches(
        policy.spec_scope_actor_id(),
        command.claim.holder.actor_id.spec_bytes(),
    ) {
        Some(LeaseError::PolicyIntersectionMismatch(
            PolicyIntersectionDimension::Actor,
        ))
    } else if !super::super::identity::concrete_identifier_matches(
        policy.spec_scope_environment_id(),
        aggregate.scope.environment.spec_bytes(),
    ) {
        Some(LeaseError::PolicyIntersectionMismatch(
            PolicyIntersectionDimension::Environment,
        ))
    } else if !super::super::identity::concrete_identifier_matches(
        policy.spec_scope_revision().spec_workspace_id().spec_bytes(),
        aggregate.scope.workspace.spec_bytes(),
    ) {
        Some(LeaseError::PolicyIntersectionMismatch(
            PolicyIntersectionDimension::Workspace,
        ))
    } else if policy.spec_scope_revision().spec_workspace_generation().spec_value()
        != aggregate.generation.spec_value()
    {
        Some(LeaseError::PolicyIntersectionMismatch(
            PolicyIntersectionDimension::Generation,
        ))
    } else if !super::super::identity::concrete_identifier_matches(
        policy.spec_permission_resource_id(),
        aggregate.scope.resource.spec_bytes(),
    ) {
        Some(LeaseError::PolicyIntersectionMismatch(
            PolicyIntersectionDimension::ResourcePermission,
        ))
    } else if policy.spec_scope_validity().spec_expires_at().spec_epoch()
        != aggregate.authority_time.spec_epoch()
    {
        Some(LeaseError::PolicyIntersectionMismatch(
            PolicyIntersectionDimension::ClockEpoch,
        ))
    } else if !policy.spec_scope_validity().spec_contains(command.observed_at) {
        Some(LeaseError::PolicyUseInvalid)
    } else {
        None
    }
}

pub(crate) open spec fn use_after_claim_error(
    aggregate: &LeaseAggregate,
    active: crate::state::ActiveLease,
    command: &UseLease,
) -> Option<LeaseError> {
    let observation = crate::transition::observation_error(
        &aggregate.authority_time,
        command.observed_at,
    );
    if observation.is_some() {
        observation
    } else {
        let expiry = crate::transition::before_expiry_error(
            active.expires_at,
            command.observed_at,
        );
        if expiry.is_some() {
            expiry
        } else {
            let intersection = policy_intersection_error(aggregate, command);
            if intersection.is_some() {
                intersection
            } else {
                let version = super::lifecycle::active_version_error(aggregate);
                if version.is_some() {
                    version
                } else if !aggregate.internal_is_valid() {
                    Some(LeaseError::CorruptState)
                } else {
                    None
                }
            }
        }
    }
}

pub(crate) open spec fn use_error(
    aggregate: &LeaseAggregate,
    command: &UseLease,
) -> Option<LeaseError> {
    let claim = crate::transition::active_claim_error(aggregate, command.claim);
    if claim.is_some() {
        claim
    } else {
        match aggregate.state {
            LeaseState::Active(active) => use_after_claim_error(aggregate, active, command),
            _ => claim,
        }
    }
}

pub closed spec fn concrete_use_decision(
    aggregate: &LeaseAggregate,
    result: crate::LeaseUseOutcome,
    command: &UseLease,
) -> bool {
    match (use_error(aggregate, command), result) {
        (None, crate::LeaseUseOutcome::Accepted(accepted)) => {
            super::super::authority::concrete_use_transition(aggregate, &accepted, command)
        }
        (Some(error), crate::LeaseUseOutcome::Rejected(failure)) => {
            super::super::authority::concrete_use_rejection(
                aggregate,
                &failure,
                command,
                error,
            )
        }
        _ => false,
    }
}

pub(crate) proof fn establish_use_rejection(
    aggregate: &LeaseAggregate,
    failure: &crate::LeaseUseFailure,
    command: &UseLease,
    error: LeaseError,
)
    requires
        use_error(aggregate, command) == Some(error),
        super::super::authority::concrete_use_rejection(
            aggregate,
            failure,
            command,
            error,
        ),
    ensures concrete_use_decision(
        aggregate,
        crate::LeaseUseOutcome::Rejected(*failure),
        command,
    ),
{
}

pub(crate) proof fn establish_use_acceptance(
    aggregate: &LeaseAggregate,
    accepted: &crate::LeaseUseTransition,
    command: &UseLease,
)
    requires
        use_error(aggregate, command).is_none(),
        super::super::authority::concrete_use_transition(aggregate, accepted, command),
    ensures concrete_use_decision(
        aggregate,
        crate::LeaseUseOutcome::Accepted(*accepted),
        command,
    ),
{
}

} // verus!
