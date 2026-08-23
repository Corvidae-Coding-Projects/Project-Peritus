//! Full transition-record and source-command equality.

use super::aggregate::scopes_equal;
#[cfg(verus_only)]
use super::aggregate::scope_fields_match;
use super::binding::bindings_equal;
#[cfg(verus_only)]
use super::binding::binding_fields_match;
use super::{bytes16_equal, bytes32_equal};
#[cfg(verus_only)]
use super::{bytes16_match, bytes32_match};
use crate::{
    LeasePhase, LeaseTransitionKind, LeaseTransitionRecord, RetirementReason,
};
use peritus_types::{Generation, RevisionNumber};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn revision_option_fields_match(
    left: Option<RevisionNumber>,
    right: Option<RevisionNumber>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => {
            left_value.spec_value() == right_value.spec_value()
        }
        _ => false,
    }
}

const fn revision_options_equal(
    left: Option<RevisionNumber>,
    right: Option<RevisionNumber>,
) -> (equal: bool)
    ensures equal == revision_option_fields_match(left, right),
{
    match (left, right) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => left_value.get() == right_value.get(),
        _ => false,
    }
}

pub(crate) open spec fn generation_option_fields_match(
    left: Option<Generation>,
    right: Option<Generation>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => {
            left_value.spec_value() == right_value.spec_value()
        }
        _ => false,
    }
}

const fn generation_options_equal(
    left: Option<Generation>,
    right: Option<Generation>,
) -> (equal: bool)
    ensures equal == generation_option_fields_match(left, right),
{
    match (left, right) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => left_value.get() == right_value.get(),
        _ => false,
    }
}

pub(crate) open spec fn phase_fields_match(left: LeasePhase, right: LeasePhase) -> bool {
    left == right
}

const fn phases_equal(left: LeasePhase, right: LeasePhase) -> (equal: bool)
    ensures equal == phase_fields_match(left, right),
{
    matches!(
        (left, right),
        (LeasePhase::Available, LeasePhase::Available)
            | (LeasePhase::Active, LeasePhase::Active)
            | (LeasePhase::Reconciling, LeasePhase::Reconciling)
            | (LeasePhase::Quarantined, LeasePhase::Quarantined)
            | (LeasePhase::Retired, LeasePhase::Retired)
    )
}

pub(crate) open spec fn phase_option_fields_match(
    left: Option<LeasePhase>,
    right: Option<LeasePhase>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => phase_fields_match(left_value, right_value),
        _ => false,
    }
}

const fn phase_options_equal(
    left: Option<LeasePhase>,
    right: Option<LeasePhase>,
) -> (equal: bool)
    ensures equal == phase_option_fields_match(left, right),
{
    match (left, right) {
        (None, None) => true,
        (Some(left_value), Some(right_value)) => phases_equal(left_value, right_value),
        _ => false,
    }
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

pub(crate) open spec fn kind_fields_match(
    left: LeaseTransitionKind,
    right: LeaseTransitionKind,
) -> bool {
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
            LeaseTransitionKind::Used {
                action_id: left_id,
                action_digest: left_digest,
            },
            LeaseTransitionKind::Used {
                action_id: right_id,
                action_digest: right_digest,
            },
        ) => {
            bytes16_match(left_id.spec_bytes(), right_id.spec_bytes())
                && bytes32_match(left_digest.spec_bytes(), right_digest.spec_bytes())
        }
        (LeaseTransitionKind::Retired(left_reason), LeaseTransitionKind::Retired(right_reason)) => {
            retirement_fields_match(left_reason, right_reason)
        }
        _ => false,
    }
}

fn kinds_equal(left: LeaseTransitionKind, right: LeaseTransitionKind) -> (equal: bool)
    ensures equal == kind_fields_match(left, right),
{
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
            LeaseTransitionKind::Used {
                action_id: left_id,
                action_digest: left_digest,
            },
            LeaseTransitionKind::Used {
                action_id: right_id,
                action_digest: right_digest,
            },
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

pub(crate) open spec fn record_fields_match(
    left: &LeaseTransitionRecord,
    right: &LeaseTransitionRecord,
) -> bool {
    bytes16_match(left.command_id.spec_bytes(), right.command_id.spec_bytes())
        && scope_fields_match(left.scope, right.scope)
        && revision_option_fields_match(left.before_version, right.before_version)
        && left.after_version.spec_value() == right.after_version.spec_value()
        && generation_option_fields_match(left.before_generation, right.before_generation)
        && left.after_generation.spec_value() == right.after_generation.spec_value()
        && phase_option_fields_match(left.before_phase, right.before_phase)
        && phase_fields_match(left.after_phase, right.after_phase)
        && kind_fields_match(left.kind, right.kind)
        && binding_fields_match(&left.binding, &right.binding)
}

pub(in crate::port) fn records_equal(
    left: &LeaseTransitionRecord,
    right: &LeaseTransitionRecord,
) -> (equal: bool)
    ensures equal == record_fields_match(left, right),
{
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

} // verus!
