//! Exact active-claim matching shared by lease reducers.

#[cfg(verus_only)]
use super::active_error;
use super::require_active;
use crate::model::concrete::identity::identifier_values_equal;
use crate::state::ActiveLease;
#[cfg(verus_only)]
use crate::state::LeaseState;
use crate::{LeaseAggregate, LeaseClaim, LeaseError, ScopeDimension};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn expected_active_claim(
    aggregate: &LeaseAggregate,
    active: ActiveLease,
) -> LeaseClaim {
    LeaseClaim {
        scope: aggregate.scope,
        holder: active.holder,
        generation: aggregate.generation,
        claim_version: active.claim_version,
        issued_at: active.issued_at,
        expires_at: active.expires_at,
    }
}

pub(crate) open spec fn claim_identity_error(
    expected: LeaseClaim,
    actual: LeaseClaim,
) -> Option<LeaseError> {
    if !crate::model::concrete_identifier_matches(
        actual.scope.workspace.spec_bytes(),
        expected.scope.workspace.spec_bytes(),
    ) {
        Some(LeaseError::ClaimScopeMismatch(ScopeDimension::Workspace))
    } else if !crate::model::concrete_identifier_matches(
        actual.scope.resource.spec_bytes(),
        expected.scope.resource.spec_bytes(),
    ) {
        Some(LeaseError::ClaimScopeMismatch(ScopeDimension::Resource))
    } else if !crate::model::concrete_identifier_matches(
        actual.scope.environment.spec_bytes(),
        expected.scope.environment.spec_bytes(),
    ) {
        Some(LeaseError::ClaimScopeMismatch(ScopeDimension::Environment))
    } else if !crate::model::concrete_identifier_matches(
        actual.holder.actor_id.spec_bytes(),
        expected.holder.actor_id.spec_bytes(),
    ) || !crate::model::concrete_identifier_matches(
        actual.holder.session_id.spec_bytes(),
        expected.holder.session_id.spec_bytes(),
    ) {
        Some(LeaseError::ClaimHolderMismatch)
    } else if actual.generation.spec_value() != expected.generation.spec_value() {
        Some(LeaseError::ClaimGenerationMismatch)
    } else if actual.claim_version.spec_value() != expected.claim_version.spec_value()
        || !crate::model::concrete_instant_matches(actual.issued_at, expected.issued_at)
        || !crate::model::concrete_instant_matches(actual.expires_at, expected.expires_at)
    {
        Some(LeaseError::ClaimVersionMismatch)
    } else {
        None
    }
}

pub(crate) open spec fn active_claim_error(
    aggregate: &LeaseAggregate,
    claim: LeaseClaim,
) -> Option<LeaseError> {
    match aggregate.state {
        LeaseState::Active(active) => {
            claim_identity_error(expected_active_claim(aggregate, active), claim)
        }
        _ => active_error(aggregate),
    }
}

pub(in crate::transition) fn require_active_claim(
    aggregate: &LeaseAggregate,
    claim: LeaseClaim,
) -> (result: Result<ActiveLease, LeaseError>)
    ensures
        match result {
            Ok(active) => {
                aggregate.state == LeaseState::Active(active)
                    && crate::model::concrete_claim_is_current(aggregate, claim)
                    && active_claim_error(aggregate, claim).is_none()
            }
            Err(error) => active_claim_error(aggregate, claim) == Some(error),
        },
{
    let active = require_active(aggregate)?;
    let expected = LeaseClaim::new(
        aggregate.scope,
        active.holder,
        aggregate.generation,
        active.claim_version,
        active.issued_at,
        active.expires_at,
    );
    validate_claim_identity(expected, claim)?;
    Ok(active)
}

pub(in crate::transition) const fn validate_claim_identity(
    expected: LeaseClaim,
    actual: LeaseClaim,
) -> (result: Result<(), LeaseError>)
    ensures
        match result {
            Ok(()) => {
                claim_identity_error(expected, actual).is_none()
                    && crate::model::concrete_claim_matches(actual, expected)
            }
            Err(error) => claim_identity_error(expected, actual) == Some(error),
        },
{
    let actual_workspace = *actual.scope.workspace.as_bytes();
    let expected_workspace = *expected.scope.workspace.as_bytes();
    let actual_resource = *actual.scope.resource.as_bytes();
    let expected_resource = *expected.scope.resource.as_bytes();
    let actual_environment = *actual.scope.environment.as_bytes();
    let expected_environment = *expected.scope.environment.as_bytes();
    let actual_actor = *actual.holder.actor_id.as_bytes();
    let expected_actor = *expected.holder.actor_id.as_bytes();
    let actual_session = *actual.holder.session_id.as_bytes();
    let expected_session = *expected.holder.session_id.as_bytes();
    assert(actual_workspace == actual.scope.workspace.spec_bytes());
    assert(expected_workspace == expected.scope.workspace.spec_bytes());
    assert(actual_resource == actual.scope.resource.spec_bytes());
    assert(expected_resource == expected.scope.resource.spec_bytes());
    assert(actual_environment == actual.scope.environment.spec_bytes());
    assert(expected_environment == expected.scope.environment.spec_bytes());
    assert(actual_actor == actual.holder.actor_id.spec_bytes());
    assert(expected_actor == expected.holder.actor_id.spec_bytes());
    assert(actual_session == actual.holder.session_id.spec_bytes());
    assert(expected_session == expected.holder.session_id.spec_bytes());
    if !identifier_values_equal(actual_workspace, expected_workspace) {
        Err(LeaseError::ClaimScopeMismatch(ScopeDimension::Workspace))
    } else if !identifier_values_equal(actual_resource, expected_resource) {
        Err(LeaseError::ClaimScopeMismatch(ScopeDimension::Resource))
    } else if !identifier_values_equal(actual_environment, expected_environment) {
        Err(LeaseError::ClaimScopeMismatch(ScopeDimension::Environment))
    } else if !identifier_values_equal(actual_actor, expected_actor)
        || !identifier_values_equal(actual_session, expected_session)
    {
        Err(LeaseError::ClaimHolderMismatch)
    } else if actual.generation.get() != expected.generation.get() {
        Err(LeaseError::ClaimGenerationMismatch)
    } else if actual.claim_version.get() != expected.claim_version.get()
        || actual.issued_at.epoch().get() != expected.issued_at.epoch().get()
        || actual.issued_at.tick_millis() != expected.issued_at.tick_millis()
        || actual.expires_at.epoch().get() != expected.expires_at.epoch().get()
        || actual.expires_at.tick_millis() != expected.expires_at.tick_millis()
    {
        Err(LeaseError::ClaimVersionMismatch)
    } else {
        assert(crate::model::concrete_scope_matches(actual.scope, expected.scope));
        assert(crate::model::concrete_holder_matches(actual.holder, expected.holder));
        assert(expected.generation.spec_value() == actual.generation.spec_value());
        assert(expected.claim_version.spec_value() == actual.claim_version.spec_value());
        assert(crate::model::concrete_instant_matches(
            expected.issued_at,
            actual.issued_at,
        ));
        assert(crate::model::concrete_instant_matches(
            expected.expires_at,
            actual.expires_at,
        ));
        Ok(())
    }
}

} // verus!
