//! Exact proof bridges for the linear transition constructor.

#[cfg(verus_only)]
use super::core::{
    authority_time_advance_matches, transition_error, transition_result_matches,
    transition_result_projection, AuthorityTimeAdvance,
};
#[cfg(verus_only)]
use super::{LeaseTransition, LeaseTransitionKind, LeaseTransitionOutcome};
#[cfg(verus_only)]
use crate::state::LeaseState;
#[cfg(verus_only)]
use crate::{LeaseAggregate, LeaseError, LeaseTransitionFailure};
#[cfg(verus_only)]
use peritus_policy::{AuthorityInstant, AuthorityTimeFailure, PolicyErrorKind};
#[cfg(verus_only)]
use peritus_types::{CommandId, Generation, RevisionNumber};
use vstd::prelude::*;

verus! {

pub(super) proof fn establish_transition_inputs(
    before: &LeaseAggregate,
    version: RevisionNumber,
    version_value: u64,
    before_version_value: u64,
)
    requires
        version.spec_value() == before.version.spec_value() + 1,
        version_value as int == version.spec_value(),
        before_version_value as int == before.version.spec_value(),
    ensures
        before.spec_is_valid() == before.internal_is_valid(),
        before.version.spec_value() < u64::MAX as int,
        version.spec_value() <= u64::MAX as int,
{
    before.reveal_internal_validity();
}

pub(super) proof fn establish_corrupt_transition_rejection(
    before: &LeaseAggregate,
    version: RevisionNumber,
    time_advance: AuthorityTimeAdvance,
    error: LeaseError,
    failure: &LeaseTransitionFailure,
)
    requires
        error == LeaseError::CorruptState,
        !before.spec_is_valid(),
        !before.internal_is_valid(),
        failure.spec_error() == error,
        crate::model::concrete_snapshot_preserved(before, &failure.spec_aggregate()),
    ensures
        transition_error(before, version, time_advance) == Some(error),
        crate::model::concrete_rejection_preserves_input(before, failure),
        crate::model::concrete::rejections::exact_transition_rejection(
            before,
            failure,
            error,
        ),
{
    crate::model::concrete::establish_rejection_preservation(before, failure);
}

pub(super) proof fn establish_time_transition_rejection(
    before: &LeaseAggregate,
    version: RevisionNumber,
    time_advance: AuthorityTimeAdvance,
    observed_at: AuthorityInstant,
    time_failure: &AuthorityTimeFailure,
    error: LeaseError,
    failure: &LeaseTransitionFailure,
)
    requires
        time_advance == AuthorityTimeAdvance::Observe(observed_at),
        before.internal_is_valid(),
        version.spec_value() == before.version.spec_value() + 1,
        before.version.spec_value() < u64::MAX as int,
        time_failure.spec_epoch() == before.authority_time.spec_epoch(),
        time_failure.spec_greatest_tick_millis()
            == before.authority_time.spec_greatest_tick_millis(),
        if observed_at.spec_epoch() != before.authority_time.spec_epoch() {
            time_failure.spec_error_kind() == PolicyErrorKind::ClockEpochMismatch
                && error == LeaseError::ClockEpochMismatch
        } else {
            observed_at.spec_tick_millis()
                < before.authority_time.spec_greatest_tick_millis()
                && time_failure.spec_error_kind() == PolicyErrorKind::ClockRegression
                && error == LeaseError::ClockRegression
        },
        failure.spec_error() == error,
        crate::model::concrete_snapshot_preserved(before, &failure.spec_aggregate()),
    ensures
        transition_error(before, version, time_advance) == Some(error),
        crate::model::concrete_rejection_preserves_input(before, failure),
        crate::model::concrete::rejections::exact_transition_rejection(
            before,
            failure,
            error,
        ),
{
    assert(super::validation::observation_error(
        &before.authority_time,
        observed_at,
    ) == Some(error));
    crate::model::concrete::establish_rejection_preservation(before, failure);
}

pub(super) proof fn establish_accepted_transition(
    before: &LeaseAggregate,
    accepted: &LeaseTransition,
    command_id: CommandId,
    version: RevisionNumber,
    generation: Generation,
    time_advance: AuthorityTimeAdvance,
    state: LeaseState,
    kind: LeaseTransitionKind,
    binding: crate::LeaseCommandBinding,
)
    requires
        transition_error(before, version, time_advance).is_none(),
        crate::model::concrete_transition_matches(
            before,
            &accepted.next,
            accepted.record,
            command_id,
            generation,
            state,
            kind,
            binding,
        ),
        authority_time_advance_matches(before, &accepted.next, time_advance),
    ensures
        transition_result_matches(
            before,
            LeaseTransitionOutcome::Accepted(*accepted),
            command_id,
            version,
            generation,
            time_advance,
            state,
            kind,
            binding,
        ),
        transition_result_projection(
            before,
            LeaseTransitionOutcome::Accepted(*accepted),
            command_id,
            generation,
            time_advance,
            state,
            kind,
            binding,
        ),
{
}

} // verus!
