//! Total capability-use outcome relation, including deterministic rejected precedence.

#![cfg(verus_only)]

use crate::{
    Capability, CapabilityUseFailure, CapabilityUseRequest, CapabilityUseTransition, PolicyErrorKind,
    ScopeDimension,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Returns the first mismatch between a request and an explicit scope value.
pub open spec fn first_scope_mismatch_value(
    request: &CapabilityUseRequest,
    scope: &crate::CapabilityScope,
) -> Option<ScopeDimension> {
    if !crate::model::same_identifier(request.spec_actor_id(), scope.spec_actor_id()) {
        Some(ScopeDimension::Actor)
    } else if request.spec_role() != scope.spec_role() {
        Some(ScopeDimension::Role)
    } else if !crate::model::same_identifier(
        request.spec_environment_id(),
        scope.spec_environment_id(),
    ) {
        Some(ScopeDimension::Environment)
    } else if !crate::model::same_revision(request.spec_revision(), scope.spec_revision()) {
        Some(ScopeDimension::Revision)
    } else if !scope.spec_contains_permission(&request.spec_permission()) {
        Some(ScopeDimension::Permissions)
    } else {
        None
    }
}

/// Returns the first mismatching scope dimension in executable validation order.
pub open spec fn first_scope_mismatch(
    prior: &Capability,
    request: &CapabilityUseRequest,
) -> Option<ScopeDimension> {
    if !crate::model::same_identifier(request.spec_actor_id(), prior.spec_scope_actor_id()) {
        Some(ScopeDimension::Actor)
    } else if request.spec_role() != prior.spec_scope_role() {
        Some(ScopeDimension::Role)
    } else if !crate::model::same_identifier(
        request.spec_environment_id(),
        prior.spec_scope_environment_id(),
    ) {
        Some(ScopeDimension::Environment)
    } else if !crate::model::same_revision(
        request.spec_revision(),
        prior.spec_scope_revision(),
    ) {
        Some(ScopeDimension::Revision)
    } else if !prior.spec_scope_contains_permission(&request.spec_permission()) {
        Some(ScopeDimension::Permissions)
    } else {
        None
    }
}

/// Returns the exact time or validity error after scope validation succeeds.
pub open spec fn time_error(
    prior: &Capability,
    observed: crate::AuthorityInstant,
) -> Option<PolicyErrorKind> {
    let validity = prior.spec_scope_validity();
    if observed.spec_epoch() != prior.spec_time_epoch() {
        Some(PolicyErrorKind::ClockEpochMismatch)
    } else if observed.spec_tick_millis() < prior.spec_greatest_tick() {
        Some(PolicyErrorKind::ClockRegression)
    } else if observed.spec_epoch() != validity.spec_not_before().spec_epoch() {
        Some(PolicyErrorKind::ClockEpochMismatch)
    } else if observed.spec_tick_millis() < validity.spec_not_before().spec_tick_millis() {
        Some(PolicyErrorKind::CapabilityNotYetValid)
    } else if observed.spec_tick_millis() >= validity.spec_expires_at().spec_tick_millis() {
        Some(PolicyErrorKind::CapabilityExpired)
    } else {
        None
    }
}

/// Returns the exact time, validity, or exhaustion error after scope validation succeeds.
pub open spec fn post_scope_error(
    prior: &Capability,
    request: &CapabilityUseRequest,
) -> Option<PolicyErrorKind> {
    match time_error(prior, request.spec_observed_at()) {
        Some(kind) => Some(kind),
        None if prior.spec_remaining_uses() == Some(0) => {
            Some(PolicyErrorKind::CapabilityExhausted)
        }
        None => None,
    }
}

/// Returns the exact error kind and detail selected by total use validation.
pub open spec fn expected_error(
    prior: &Capability,
    request: &CapabilityUseRequest,
) -> Option<(PolicyErrorKind, Option<ScopeDimension>)> {
    match first_scope_mismatch(prior, request) {
        Some(dimension) => Some((PolicyErrorKind::CapabilityScopeMismatch, Some(dimension))),
        None => match post_scope_error(prior, request) {
            Some(kind) => Some((kind, None)),
            None => None,
        },
    }
}

/// Exact preservation of every authority-bearing capability field on rejection.
pub open spec fn failure_preserves_prior(
    prior: &Capability,
    failure: &CapabilityUseFailure,
) -> bool {
    failure.spec_scope_actor_id() == prior.spec_scope_actor_id()
        && failure.spec_scope_role() == prior.spec_scope_role()
        && failure.spec_scope_environment_id() == prior.spec_scope_environment_id()
        && failure.spec_scope_permissions() == prior.spec_scope_permissions()
        && failure.spec_scope_revision() == prior.spec_scope_revision()
        && failure.spec_scope_validity() == prior.spec_scope_validity()
        && failure.spec_scope_use_limit() == prior.spec_scope_use_limit()
        && failure.spec_remaining_uses() == prior.spec_remaining_uses()
        && failure.spec_issued_at() == prior.spec_issued_at()
        && failure.spec_issuance_digest() == prior.spec_issuance_digest()
        && failure.spec_issuance_command_id() == prior.spec_issuance_command_id()
        && failure.spec_time_epoch() == prior.spec_time_epoch()
        && failure.spec_greatest_tick() == prior.spec_greatest_tick()
}

/// Closed total result relation for one public capability-use attempt.
pub open spec fn result_is_exact(
    prior: &Capability,
    request: &CapabilityUseRequest,
    transition_digest: Sha256Digest,
    result: &Result<CapabilityUseTransition, CapabilityUseFailure>,
) -> bool {
    match (expected_error(prior, request), result) {
        (None, Ok(transition)) => crate::model::capability_use_success(
            prior,
            request,
            transition_digest,
            transition,
        ),
        (Some((kind, dimension)), Err(failure)) => {
            failure.spec_error_kind() == kind
                && failure.spec_error_dimension() == dimension
                && failure.spec_error_collection().is_none()
                && failure_preserves_prior(prior, failure)
        }
        _ => false,
    }
}

} // verus!
