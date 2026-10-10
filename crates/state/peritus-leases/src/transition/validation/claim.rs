//! Exact active-claim matching shared by lease reducers.

use super::require_active;
use crate::model::concrete::identity::identifier_values_equal;
use crate::state::ActiveLease;

use crate::{LeaseAggregate, LeaseClaim, LeaseError, ScopeDimension};

pub(in crate::transition) fn require_active_claim(
    aggregate: &LeaseAggregate,
    claim: LeaseClaim,
) -> Result<ActiveLease, LeaseError> {
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
) -> Result<(), LeaseError> {
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
        Ok(())
    }
}
