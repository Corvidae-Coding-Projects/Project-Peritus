//! Full aggregate semantic equality.

use super::bytes16_equal;

use crate::state::{LeaseState, QuarantinedState, ReconciliationState};
use crate::{
    FenceCause, LeaseAggregate, LeaseClaim, LeaseHolder, LeaseScope, ReconciliationCorrelation,
    ReconciliationDisposition, RetirementReason,
};

pub(in crate::port) fn scopes_equal(left: LeaseScope, right: LeaseScope) -> bool {
    bytes16_equal(*left.workspace.as_bytes(), *right.workspace.as_bytes())
        && bytes16_equal(*left.resource.as_bytes(), *right.resource.as_bytes())
        && bytes16_equal(*left.environment.as_bytes(), *right.environment.as_bytes())
}
pub(in crate::port) fn holders_equal(left: LeaseHolder, right: LeaseHolder) -> bool {
    bytes16_equal(*left.actor_id.as_bytes(), *right.actor_id.as_bytes())
        && bytes16_equal(*left.session_id.as_bytes(), *right.session_id.as_bytes())
}
pub(in crate::port) fn claims_equal(left: LeaseClaim, right: LeaseClaim) -> bool {
    scopes_equal(left.scope, right.scope)
        && holders_equal(left.holder, right.holder)
        && left.generation.get() == right.generation.get()
        && left.claim_version.get() == right.claim_version.get()
        && left.issued_at.epoch().get() == right.issued_at.epoch().get()
        && left.issued_at.tick_millis() == right.issued_at.tick_millis()
        && left.expires_at.epoch().get() == right.expires_at.epoch().get()
        && left.expires_at.tick_millis() == right.expires_at.tick_millis()
}
pub(in crate::port) fn correlations_equal(
    left: ReconciliationCorrelation,
    right: ReconciliationCorrelation,
) -> bool {
    scopes_equal(left.scope, right.scope)
        && left.fenced_generation.get() == right.fenced_generation.get()
        && holders_equal(left.prior_holder, right.prior_holder)
}
const fn causes_equal(left: FenceCause, right: FenceCause) -> bool {
    matches!(
        (left, right),
        (FenceCause::ReleasedWithoutQuiescence, FenceCause::ReleasedWithoutQuiescence)
            | (FenceCause::Expired, FenceCause::Expired)
            | (FenceCause::HolderLost, FenceCause::HolderLost)
            | (FenceCause::ClockDiscontinuity, FenceCause::ClockDiscontinuity)
            | (FenceCause::Revoked, FenceCause::Revoked)
    )
}
pub(in crate::port) fn dispositions_equal(
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
fn reconciliation_states_equal(left: ReconciliationState, right: ReconciliationState) -> bool {
    correlations_equal(left.correlation, right.correlation) && causes_equal(left.cause, right.cause)
}
fn quarantined_states_equal(left: QuarantinedState, right: QuarantinedState) -> bool {
    correlations_equal(left.correlation, right.correlation)
        && causes_equal(left.cause, right.cause)
        && dispositions_equal(left.disposition, right.disposition)
}
const fn retirements_equal(left: RetirementReason, right: RetirementReason) -> bool {
    matches!(
        (left, right),
        (RetirementReason::GenerationExhausted, RetirementReason::GenerationExhausted)
            | (RetirementReason::VersionExhausted, RetirementReason::VersionExhausted)
    )
}
fn states_equal(left: LeaseState, right: LeaseState) -> bool {
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
pub(in crate::port) fn aggregates_equal(left: &LeaseAggregate, right: &LeaseAggregate) -> bool {
    scopes_equal(left.scope, right.scope)
        && left.generation.get() == right.generation.get()
        && left.version.get() == right.version.get()
        && left.authority_time.epoch().get() == right.authority_time.epoch().get()
        && left.authority_time.greatest_tick_millis() == right.authority_time.greatest_tick_millis()
        && states_equal(left.state, right.state)
}
