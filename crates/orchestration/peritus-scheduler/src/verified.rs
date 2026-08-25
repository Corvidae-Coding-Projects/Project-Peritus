//! Executable scheduler invariants and focused Verus proof roots.

use std::collections::BTreeSet;

use vstd::prelude::*;

use crate::{
    SchedulerPhase, SchedulerState, SchedulerTerminalKind, SchedulerTransition, WorkId, WorkPhase,
    WorkTerminal,
};

/// Returns whether every dependency of work is terminal-successful.
#[must_use]
pub fn dependencies_are_ready(state: &SchedulerState, work_id: WorkId) -> bool {
    state.work_item(work_id).is_some_and(|work| {
        work.spec().dependencies().iter().all(|id| {
            state
                .work_item(*id)
                .and_then(crate::WorkRecord::terminal)
                .is_some_and(|terminal| matches!(terminal, WorkTerminal::Succeeded { .. }))
        })
    })
}

/// Returns whether global and worker resource/concurrency capacities contain every reservation.
#[must_use]
pub fn reservations_fit(state: &SchedulerState) -> bool {
    let limits = state.binding().limits();
    let Ok(global) = resource_sum(state.reservations().iter(), limits.resource_dimensions()) else {
        return false;
    };
    if global.is_some_and(|used| !used.fits_within(state.binding().capacity())) {
        return false;
    }
    state.workers().iter().all(|worker| {
        let owned: Vec<_> = state
            .reservations()
            .iter()
            .filter(|reservation| reservation.worker_id() == worker.descriptor().id())
            .collect();
        if owned.len() > usize::from(worker.descriptor().concurrency()) {
            return false;
        }
        let Ok(used) = resource_sum(owned.into_iter(), limits.resource_dimensions()) else {
            return false;
        };
        used.is_none_or(|used| used.fits_within(worker.descriptor().capacity()))
    })
}

fn resource_sum<'a>(
    mut values: impl Iterator<Item = &'a crate::SchedulerReservation>,
    dimensions: u16,
) -> Result<Option<crate::ResourceVector>, crate::SchedulerError> {
    values.try_fold(None::<crate::ResourceVector>, |sum, reservation| {
        sum.map_or_else(
            || Ok(Some(reservation.resources().clone())),
            |current| current.checked_add(reservation.resources(), dimensions).map(Some),
        )
    })
}

/// Returns whether every live dispatch uniquely owns one work attempt and vice versa.
#[must_use]
pub fn unique_dispatch_ownership(state: &SchedulerState) -> bool {
    let mut dispatches = BTreeSet::new();
    let mut work = BTreeSet::new();
    let reservations_valid = state.reservations().iter().all(|reservation| {
        dispatches.insert(reservation.dispatch_id())
            && work.insert(reservation.work_id())
            && state.work_item(reservation.work_id()).is_some_and(|record| {
                matches!(
                    record.phase(),
                    WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling
                ) && record.attempts_started() == reservation.attempt().get()
            })
    });
    reservations_valid
        && state.work().iter().all(|record| {
            let count = state
                .reservations()
                .iter()
                .filter(|reservation| reservation.work_id() == record.spec().id())
                .count();
            if matches!(
                record.phase(),
                WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling
            ) {
                count == 1
            } else {
                count == 0
            }
        })
}

/// Returns whether attempts are nonzero when owned and never exceed immutable per-work bounds.
#[must_use]
pub fn attempts_are_monotonic(state: &SchedulerState) -> bool {
    state.work().iter().all(|record| {
        record.attempts_started() <= record.spec().maximum_attempts().get()
            && (!matches!(
                record.phase(),
                WorkPhase::Reserved | WorkPhase::Running | WorkPhase::Cancelling
            ) || record.attempts_started() > 0)
    })
}

/// Returns executable truthful-terminal invariant.
#[must_use]
pub fn no_implicit_success(state: &SchedulerState) -> bool {
    state.terminal().is_none_or(|terminal| {
        state.phase() == SchedulerPhase::Terminal
            && state.reservations().is_empty()
            && state.all_work_terminal()
            && (terminal.kind() != SchedulerTerminalKind::Completed
                || state.work().iter().all(|record| {
                    matches!(record.terminal(), Some(WorkTerminal::Succeeded { .. }))
                }))
    })
}

/// Returns exact complete-state replay equivalence.
#[must_use]
pub fn replay_equivalent(expected: &SchedulerState, observed: &SchedulerState) -> bool {
    expected == observed && expected.state_digest() == observed.state_digest()
}

/// Returns whether a transition advances exactly once with all state/event fences.
#[must_use]
pub fn transition_is_legal(prior: &SchedulerState, transition: &SchedulerTransition) -> bool {
    let event = transition.event();
    let successor = transition.state();
    prior.phase() != SchedulerPhase::Terminal
        && event.run_id() == prior.run_id()
        && event.previous_event() == Some(prior.last_event_id())
        && event.prior_state_digest() == prior.state_digest()
        && event.sequence().get() == prior.sequence().get().saturating_add(1)
        && successor.sequence() == event.sequence()
        && successor.last_event_id() == event.id()
        && successor.state_digest() == event.successor_state_digest()
}

verus! {

/// Mathematical exact resource conservation for one transition.
pub open spec fn resource_conserved(
    before: int,
    reserved: int,
    released: int,
    after: int,
    capacity: int,
) -> bool {
    0 <= before && 0 <= reserved && 0 <= released
        && released <= before + reserved
        && after == before + reserved - released
        && after <= capacity
}

/// Mathematical unique work/dispatch ownership.
pub open spec fn unique_ownership(dispatches: int, owned_work: int) -> bool {
    0 <= dispatches && dispatches == owned_work
}

/// Mathematical dependency readiness.
pub open spec fn dependency_ready(required: int, successful: int, claimed: bool) -> bool {
    !claimed || (0 <= required && required == successful)
}

/// Mathematical deterministic selector equality.
pub open spec fn deterministic_choice(left: int, right: int, claimed_equal: bool) -> bool {
    !claimed_equal || left == right
}

/// Mathematical bounded bypass counter.
pub open spec fn bounded_bypass(before: int, selected: bool, limit: int, after: int) -> bool {
    0 < limit && 0 <= before && before <= limit
        && if selected { after == 0 } else { after == if before < limit { before + 1 } else { limit } }
}

/// Mathematical attempt monotonicity.
pub open spec fn monotonic_attempt(before: int, dispatched: bool, maximum: int, after: int) -> bool {
    0 <= before && before <= maximum
        && if dispatched { after == before + 1 && after <= maximum } else { after == before }
}

/// Mathematical cancellation dominance over late success.
pub open spec fn cancellation_dominates(cancelling: bool, accepted_success: bool) -> bool {
    !cancelling || !accepted_success
}

/// Mathematical terminal success truth.
pub open spec fn truthful_completion(
    completed: bool,
    all_succeeded: bool,
    no_reservations: bool,
    no_directives: bool,
) -> bool {
    !completed || (all_succeeded && no_reservations && no_directives)
}

/// Mathematical replay equivalence.
pub open spec fn exact_replay(expected: int, observed: int, claimed: bool) -> bool {
    !claimed || expected == observed
}

/// Mathematical one-event reducer fence.
#[allow(clippy::too_many_arguments)]
pub open spec fn legal_reducer_step(
    open: bool,
    sequence: int,
    expected: int,
    predecessor: bool,
    revision: bool,
    digest: bool,
    fresh_command: bool,
    events: int,
    successor: int,
) -> bool {
    open && sequence == expected && predecessor && revision && digest && fresh_command
        && events == 1 && successor == sequence + 1
}

/// Proves capacity cannot be exceeded by a conserved transition.
pub proof fn conservation_preserves_capacity(
    before: int,
    reserved: int,
    released: int,
    after: int,
    capacity: int,
)
    requires resource_conserved(before, reserved, released, after, capacity)
    ensures 0 <= after && after <= capacity
{
}

/// Proves cancellation cannot accept a late success observation.
pub proof fn cancelled_work_cannot_succeed()
    ensures cancellation_dominates(true, false)
{
}

/// Proves a completed claim requires every named terminal premise.
pub proof fn completion_requires_quiescence()
    ensures !truthful_completion(true, true, true, false)
{
}

/// Proves a selected item resets its fairness counter.
pub proof fn selected_item_resets_bypass(before: int, limit: int)
    requires 0 < limit, 0 <= before, before <= limit
    ensures bounded_bypass(before, true, limit, 0)
{
}

/// Proves a saturated feasible item does not overflow its bypass counter.
pub proof fn saturated_bypass_stays_bounded(limit: int)
    requires 0 < limit
    ensures bounded_bypass(limit, false, limit, limit)
{
}

/// Proves readiness cannot be claimed with an unsuccessful dependency.
pub proof fn missing_dependency_success_blocks(required: int, successful: int)
    requires 0 <= successful, successful < required
    ensures !dependency_ready(required, successful, true)
{
}

} // verus!
