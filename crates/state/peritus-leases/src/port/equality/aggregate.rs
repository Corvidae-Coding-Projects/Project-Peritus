//! Full aggregate semantic equality.

use super::bytes16_equal;
#[cfg(verus_only)]
use super::bytes16_match;
use crate::state::{LeaseState, QuarantinedState, ReconciliationState};
use crate::{
    FenceCause, LeaseAggregate, LeaseClaim, LeaseHolder, LeaseScope, ReconciliationCorrelation,
    ReconciliationDisposition, RetirementReason,
};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn scope_fields_match(left: LeaseScope, right: LeaseScope) -> bool {
    bytes16_match(left.workspace.spec_bytes(), right.workspace.spec_bytes())
        && bytes16_match(left.resource.spec_bytes(), right.resource.spec_bytes())
        && bytes16_match(
            left.environment.spec_bytes(),
            right.environment.spec_bytes(),
        )
}

pub(in crate::port) fn scopes_equal(left: LeaseScope, right: LeaseScope) -> (equal: bool)
    ensures equal == scope_fields_match(left, right),
{
    bytes16_equal(*left.workspace.as_bytes(), *right.workspace.as_bytes())
        && bytes16_equal(*left.resource.as_bytes(), *right.resource.as_bytes())
        && bytes16_equal(*left.environment.as_bytes(), *right.environment.as_bytes())
}

pub(crate) open spec fn holder_fields_match(left: LeaseHolder, right: LeaseHolder) -> bool {
    bytes16_match(left.actor_id.spec_bytes(), right.actor_id.spec_bytes())
        && bytes16_match(left.session_id.spec_bytes(), right.session_id.spec_bytes())
}

pub(in crate::port) fn holders_equal(left: LeaseHolder, right: LeaseHolder) -> (equal: bool)
    ensures equal == holder_fields_match(left, right),
{
    bytes16_equal(*left.actor_id.as_bytes(), *right.actor_id.as_bytes())
        && bytes16_equal(*left.session_id.as_bytes(), *right.session_id.as_bytes())
}

pub(crate) open spec fn claim_fields_match(left: LeaseClaim, right: LeaseClaim) -> bool {
    scope_fields_match(left.scope, right.scope)
        && holder_fields_match(left.holder, right.holder)
        && left.generation.spec_value() == right.generation.spec_value()
        && left.claim_version.spec_value() == right.claim_version.spec_value()
        && crate::model::concrete_instant_matches(left.issued_at, right.issued_at)
        && crate::model::concrete_instant_matches(left.expires_at, right.expires_at)
}

pub(in crate::port) fn claims_equal(left: LeaseClaim, right: LeaseClaim) -> (equal: bool)
    ensures equal == claim_fields_match(left, right),
{
    scopes_equal(left.scope, right.scope)
        && holders_equal(left.holder, right.holder)
        && left.generation.get() == right.generation.get()
        && left.claim_version.get() == right.claim_version.get()
        && left.issued_at.epoch().get() == right.issued_at.epoch().get()
        && left.issued_at.tick_millis() == right.issued_at.tick_millis()
        && left.expires_at.epoch().get() == right.expires_at.epoch().get()
        && left.expires_at.tick_millis() == right.expires_at.tick_millis()
}

pub(crate) open spec fn correlation_fields_match(
    left: ReconciliationCorrelation,
    right: ReconciliationCorrelation,
) -> bool {
    scope_fields_match(left.scope, right.scope)
        && left.fenced_generation.spec_value() == right.fenced_generation.spec_value()
        && holder_fields_match(left.prior_holder, right.prior_holder)
}

pub(in crate::port) fn correlations_equal(
    left: ReconciliationCorrelation,
    right: ReconciliationCorrelation,
) -> (equal: bool)
    ensures equal == correlation_fields_match(left, right),
{
    scopes_equal(left.scope, right.scope)
        && left.fenced_generation.get() == right.fenced_generation.get()
        && holders_equal(left.prior_holder, right.prior_holder)
}

pub(crate) open spec fn cause_fields_match(left: FenceCause, right: FenceCause) -> bool {
    left == right
}

const fn causes_equal(left: FenceCause, right: FenceCause) -> (equal: bool)
    ensures equal == cause_fields_match(left, right),
{
    matches!(
        (left, right),
        (FenceCause::ReleasedWithoutQuiescence, FenceCause::ReleasedWithoutQuiescence)
            | (FenceCause::Expired, FenceCause::Expired)
            | (FenceCause::HolderLost, FenceCause::HolderLost)
            | (FenceCause::ClockDiscontinuity, FenceCause::ClockDiscontinuity)
            | (FenceCause::Revoked, FenceCause::Revoked)
    )
}

pub(crate) open spec fn disposition_fields_match(
    left: ReconciliationDisposition,
    right: ReconciliationDisposition,
) -> bool {
    match (left, right) {
        (
            ReconciliationDisposition::SafeToAcquire {
                holder_quiescence: left_holder,
                resource_safety: left_resource,
            },
            ReconciliationDisposition::SafeToAcquire {
                holder_quiescence: right_holder,
                resource_safety: right_resource,
            },
        ) => {
            bytes16_match(left_holder.spec_bytes(), right_holder.spec_bytes())
                && bytes16_match(left_resource.spec_bytes(), right_resource.spec_bytes())
        }
        (
            ReconciliationDisposition::Dirty { evidence_id: left_id },
            ReconciliationDisposition::Dirty { evidence_id: right_id },
        )
        | (
            ReconciliationDisposition::Indeterminate { evidence_id: left_id },
            ReconciliationDisposition::Indeterminate { evidence_id: right_id },
        ) => bytes16_match(left_id.spec_bytes(), right_id.spec_bytes()),
        _ => false,
    }
}

pub(in crate::port) fn dispositions_equal(
    left: ReconciliationDisposition,
    right: ReconciliationDisposition,
) -> (equal: bool)
    ensures equal == disposition_fields_match(left, right),
{
    match (left, right) {
        (
            ReconciliationDisposition::SafeToAcquire {
                holder_quiescence: left_holder,
                resource_safety: left_resource,
            },
            ReconciliationDisposition::SafeToAcquire {
                holder_quiescence: right_holder,
                resource_safety: right_resource,
            },
        ) => {
            bytes16_equal(*left_holder.as_bytes(), *right_holder.as_bytes())
                && bytes16_equal(*left_resource.as_bytes(), *right_resource.as_bytes())
        }
        (
            ReconciliationDisposition::Dirty { evidence_id: left_id },
            ReconciliationDisposition::Dirty { evidence_id: right_id },
        )
        | (
            ReconciliationDisposition::Indeterminate { evidence_id: left_id },
            ReconciliationDisposition::Indeterminate { evidence_id: right_id },
        ) => bytes16_equal(*left_id.as_bytes(), *right_id.as_bytes()),
        _ => false,
    }
}

pub(crate) open spec fn reconciliation_fields_match(
    left: ReconciliationState,
    right: ReconciliationState,
) -> bool {
    correlation_fields_match(left.correlation, right.correlation)
        && cause_fields_match(left.cause, right.cause)
}

fn reconciliation_states_equal(
    left: ReconciliationState,
    right: ReconciliationState,
) -> (equal: bool)
    ensures equal == reconciliation_fields_match(left, right),
{
    correlations_equal(left.correlation, right.correlation)
        && causes_equal(left.cause, right.cause)
}

pub(crate) open spec fn quarantined_fields_match(left: QuarantinedState, right: QuarantinedState) -> bool {
    correlation_fields_match(left.correlation, right.correlation)
        && cause_fields_match(left.cause, right.cause)
        && disposition_fields_match(left.disposition, right.disposition)
}

fn quarantined_states_equal(
    left: QuarantinedState,
    right: QuarantinedState,
) -> (equal: bool)
    ensures equal == quarantined_fields_match(left, right),
{
    correlations_equal(left.correlation, right.correlation)
        && causes_equal(left.cause, right.cause)
        && dispositions_equal(left.disposition, right.disposition)
}

pub(crate) open spec fn retirement_fields_match(left: RetirementReason, right: RetirementReason) -> bool {
    left == right
}

const fn retirements_equal(left: RetirementReason, right: RetirementReason) -> (equal: bool)
    ensures equal == retirement_fields_match(left, right),
{
    matches!(
        (left, right),
        (RetirementReason::GenerationExhausted, RetirementReason::GenerationExhausted)
            | (RetirementReason::VersionExhausted, RetirementReason::VersionExhausted)
    )
}

pub(crate) open spec fn state_fields_match(left: LeaseState, right: LeaseState) -> bool {
    match (left, right) {
        (LeaseState::Available, LeaseState::Available) => true,
        (LeaseState::Active(left_active), LeaseState::Active(right_active)) => {
            holder_fields_match(left_active.holder, right_active.holder)
                && left_active.claim_version.spec_value()
                    == right_active.claim_version.spec_value()
                && crate::model::concrete_instant_matches(
                    left_active.issued_at,
                    right_active.issued_at,
                )
                && crate::model::concrete_instant_matches(
                    left_active.expires_at,
                    right_active.expires_at,
                )
        }
        (LeaseState::Reconciling(left_state), LeaseState::Reconciling(right_state)) => {
            reconciliation_fields_match(left_state, right_state)
        }
        (LeaseState::Quarantined(left_state), LeaseState::Quarantined(right_state)) => {
            quarantined_fields_match(left_state, right_state)
        }
        (LeaseState::Retired(left_reason), LeaseState::Retired(right_reason)) => {
            retirement_fields_match(left_reason, right_reason)
        }
        _ => false,
    }
}

fn states_equal(left: LeaseState, right: LeaseState) -> (equal: bool)
    ensures equal == state_fields_match(left, right),
{
    match (left, right) {
        (LeaseState::Available, LeaseState::Available) => true,
        (LeaseState::Active(left_active), LeaseState::Active(right_active)) => {
            holders_equal(left_active.holder, right_active.holder)
                && left_active.claim_version.get() == right_active.claim_version.get()
                && left_active.issued_at.epoch().get() == right_active.issued_at.epoch().get()
                && left_active.issued_at.tick_millis() == right_active.issued_at.tick_millis()
                && left_active.expires_at.epoch().get() == right_active.expires_at.epoch().get()
                && left_active.expires_at.tick_millis() == right_active.expires_at.tick_millis()
        }
        (LeaseState::Reconciling(left_state), LeaseState::Reconciling(right_state)) => {
            reconciliation_states_equal(left_state, right_state)
        }
        (LeaseState::Quarantined(left_state), LeaseState::Quarantined(right_state)) => {
            quarantined_states_equal(left_state, right_state)
        }
        (LeaseState::Retired(left_reason), LeaseState::Retired(right_reason)) => {
            retirements_equal(left_reason, right_reason)
        }
        _ => false,
    }
}

pub(crate) open spec fn aggregate_fields_match(
    left: &LeaseAggregate,
    right: &LeaseAggregate,
) -> bool {
    scope_fields_match(left.scope, right.scope)
        && left.generation.spec_value() == right.generation.spec_value()
        && left.version.spec_value() == right.version.spec_value()
        && left.authority_time.spec_epoch() == right.authority_time.spec_epoch()
        && left.authority_time.spec_greatest_tick_millis()
            == right.authority_time.spec_greatest_tick_millis()
        && state_fields_match(left.state, right.state)
}

pub(in crate::port) fn aggregates_equal(
    left: &LeaseAggregate,
    right: &LeaseAggregate,
) -> (equal: bool)
    ensures equal == aggregate_fields_match(left, right),
{
    scopes_equal(left.scope, right.scope)
        && left.generation.get() == right.generation.get()
        && left.version.get() == right.version.get()
        && left.authority_time.epoch().get() == right.authority_time.epoch().get()
        && left.authority_time.greatest_tick_millis()
            == right.authority_time.greatest_tick_millis()
        && states_equal(left.state, right.state)
}

} // verus!
