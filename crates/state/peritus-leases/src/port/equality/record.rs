//! Full transition-record and source-command equality.

use super::aggregate::scopes_equal;

use super::binding::bindings_equal;

use super::{bytes16_equal, bytes32_equal};

use crate::{LeasePhase, LeaseTransitionKind, LeaseTransitionRecord, RetirementReason};
use peritus_types::{Generation, RevisionNumber};

const fn revision_options_equal(
    left: Option<RevisionNumber>,
    right: Option<RevisionNumber>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => left_value.get() == right_value.get(),
        _ => false,
    }
}
const fn generation_options_equal(left: Option<Generation>, right: Option<Generation>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => left_value.get() == right_value.get(),
        _ => false,
    }
}
const fn phases_equal(left: LeasePhase, right: LeasePhase) -> bool {
    matches!(
        (left, right),
        (LeasePhase::Available, LeasePhase::Available)
            | (LeasePhase::Active, LeasePhase::Active)
            | (LeasePhase::Reconciling, LeasePhase::Reconciling)
            | (LeasePhase::Quarantined, LeasePhase::Quarantined)
            | (LeasePhase::Retired, LeasePhase::Retired)
    )
}
const fn phase_options_equal(left: Option<LeasePhase>, right: Option<LeasePhase>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => phases_equal(left_value, right_value),
        _ => false,
    }
}
const fn retirements_equal(left: RetirementReason, right: RetirementReason) -> bool {
    matches!(
        (left, right),
        (RetirementReason::GenerationExhausted, RetirementReason::GenerationExhausted)
            | (RetirementReason::VersionExhausted, RetirementReason::VersionExhausted)
    )
}
fn kinds_equal(left: LeaseTransitionKind, right: LeaseTransitionKind) -> bool {
    match (left, right) {
        (LeaseTransitionKind::Minted, LeaseTransitionKind::Minted)
        | (LeaseTransitionKind::Acquired, LeaseTransitionKind::Acquired)
        | (LeaseTransitionKind::Renewed, LeaseTransitionKind::Renewed)
        | (LeaseTransitionKind::ReleasedAvailable, LeaseTransitionKind::ReleasedAvailable)
        | (LeaseTransitionKind::ReleasedReconciling, LeaseTransitionKind::ReleasedReconciling)
        | (LeaseTransitionKind::Expired, LeaseTransitionKind::Expired)
        | (LeaseTransitionKind::HolderLost, LeaseTransitionKind::HolderLost)
        | (LeaseTransitionKind::ClockDiscontinuity, LeaseTransitionKind::ClockDiscontinuity)
        | (LeaseTransitionKind::Revoked, LeaseTransitionKind::Revoked)
        | (LeaseTransitionKind::ReconciledAvailable, LeaseTransitionKind::ReconciledAvailable)
        | (
            LeaseTransitionKind::ReconciledQuarantined,
            LeaseTransitionKind::ReconciledQuarantined,
        ) => true,
        (
            LeaseTransitionKind::Used { action_id: left_id, action_digest: left_digest },
            LeaseTransitionKind::Used { action_id: right_id, action_digest: right_digest },
        ) => {
            bytes16_equal(*left_id.as_bytes(), *right_id.as_bytes())
                && bytes32_equal(*left_digest.as_bytes(), *right_digest.as_bytes())
        }
        (LeaseTransitionKind::Retired(left_reason), LeaseTransitionKind::Retired(right_reason)) => {
            retirements_equal(left_reason, right_reason)
        }
        _ => false,
    }
}
pub(in crate::port) fn records_equal(
    left: &LeaseTransitionRecord,
    right: &LeaseTransitionRecord,
) -> bool {
    bytes16_equal(*left.command_id.as_bytes(), *right.command_id.as_bytes())
        && scopes_equal(left.scope, right.scope)
        && revision_options_equal(left.before_version, right.before_version)
        && left.after_version.get() == right.after_version.get()
        && generation_options_equal(left.before_generation, right.before_generation)
        && left.after_generation.get() == right.after_generation.get()
        && phase_options_equal(left.before_phase, right.before_phase)
        && phases_equal(left.after_phase, right.after_phase)
        && kinds_equal(left.kind, right.kind)
        && bindings_equal(&left.binding, &right.binding)
}
