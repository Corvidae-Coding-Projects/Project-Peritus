//! Verified reservation-stable work lifecycle mutations.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::WorkRecord;
use crate::{SchedulerState, WorkId, WorkPhase, WorkTerminal};
use peritus_types::Sha256Digest;

mod attempt;
mod release;
mod wrappers;

pub use attempt::begin_work_attempt_at;
#[cfg(verus_only)]
pub(crate) use release::release_target_exists;
pub use release::{release_to_phase, release_to_retry_pending, release_to_terminal};
pub use wrappers::{queue_work_retry, set_work_bypasses, set_work_phase, terminalize_work};

verus! {

pub(super) enum WorkUpdate {
    Phase(WorkPhase),
    QueueRetry,
    RetryPending(Sha256Digest),
    Terminal(WorkTerminal),
}

impl WorkUpdate {
    pub(super) open spec fn spec_phase(&self) -> WorkPhase {
        match self {
            Self::Phase(phase) => *phase,
            Self::QueueRetry => WorkPhase::Queued,
            Self::RetryPending(_) => WorkPhase::RetryPending,
            Self::Terminal(_) => WorkPhase::Terminal,
        }
    }

}

/// Relates the uniquely targeted record before and after one phase mutation.
pub open spec fn work_record_update_matches(
    before: WorkRecord,
    after: WorkRecord,
    id: WorkId,
    phase: WorkPhase,
) -> bool {
    &&& before.spec_definition().spec_id() == id
    &&& WorkRecord::reservation_binding_equivalent(&before, &after)
    &&& after.spec_phase() == phase
}

/// Relates the complete work sequence before and after one lookup-based mutation.
pub open spec fn work_update_matches(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    phase: WorkPhase,
    found: bool,
) -> bool {
    &&& before.len() == after.len()
    &&& if found {
        exists |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& work_record_update_matches(before[index], after[index], id, phase)
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != index ==>
                    after[other] == before[other]
        }
    } else {
        &&& after == before
        &&& forall |index: int| #![trigger before[index]]
            0 <= index < before.len() ==>
                before[index].spec_definition().spec_id() != id
    }
}

fn update_work_at(
    state: &mut SchedulerState,
    index: usize,
    _id: WorkId,
    update: WorkUpdate,
)
    requires
        index < state.spec_work().len(),
        state.spec_work()[index as int].spec_definition().spec_id() == _id,
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        work_update_matches(
            old(state).spec_work(),
            final(state).spec_work(),
            _id,
            update.spec_phase(),
            true,
        ),
        old(state).spec_reservation_reducer_ready()
                && crate::verified::work_phase_update_admissible(
                    old(state).spec_work(),
                    old(state).spec_reservations(),
                    _id,
                    update.spec_phase(),
                )
            ==> final(state).spec_reservation_reducer_ready(),
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost before = state.spec_work();
    let ghost reservations = state.spec_reservations();
    let ghost used_dispatches = state.spec_used_dispatches();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost target_phase = update.spec_phase();
    let ghost was_admissible = crate::verified::work_phase_update_admissible(
        before,
        reservations,
        _id,
        target_phase,
    );
    match update {
        WorkUpdate::Phase(phase) => state.work[index].set_phase(phase),
        WorkUpdate::QueueRetry => state.work[index].queue_retry(),
        WorkUpdate::RetryPending(cause) => state.work[index].set_retry_pending(cause),
        WorkUpdate::Terminal(terminal) => state.work[index].terminalize(terminal),
    }
    proof {
        assert(before.len() == state.spec_work().len());
        assert(WorkRecord::reservation_binding_equivalent(
            &before[index as int],
            &state.spec_work()[index as int],
        ));
        assert(state.spec_work()[index as int].spec_phase() == target_phase);
        assert forall |other: int| #![auto]
            0 <= other < before.len() && other != index
                implies before[other] == state.spec_work()[other] by {
        }
        if had_invariant {
            crate::verified::actual_reservation_work_update_preserves(
                state.spec_binding(),
                state.spec_workers(),
                before,
                state.spec_work(),
                state.spec_reservations(),
                index as int,
            );
        }
        reveal(work_record_update_matches);
        reveal(work_update_matches);
        assert(exists |found_at: int| #![trigger before[found_at]] {
            &&& 0 <= found_at < before.len()
            &&& work_record_update_matches(
                before[found_at],
                state.spec_work()[found_at],
                _id,
                target_phase,
            )
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != found_at ==>
                    state.spec_work()[other] == before[other]
        }) by {
            assert(0 <= (index as int) && (index as int) < before.len());
        }
        if was_ready && was_admissible {
            reveal(SchedulerState::spec_reservation_reducer_ready);
            reveal(crate::verified::reservation_invariant_parts);
            crate::verified::admissible_work_update_preserves_relations(
                before,
                state.spec_work(),
                reservations,
                used_dispatches,
                _id,
                target_phase,
                index as int,
            );
        }
    };
}

proof fn establish_work_not_found(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<crate::SchedulerReservation>,
    id: WorkId,
    phase: WorkPhase,
    visited: int,
)
    requires
        after == before,
        visited == before.len(),
        forall |prior: int| #![auto]
            0 <= prior < visited ==> before[prior].spec_definition().spec_id() != id,
    ensures
        work_update_matches(before, after, id, phase, false),
        !crate::verified::work_phase_update_admissible(before, reservations, id, phase),
{
    assert forall |at: int| #![trigger before[at]]
        0 <= at < before.len() implies before[at].spec_definition().spec_id() != id by {
        assert(at < visited);
    }
    reveal(work_update_matches);
    reveal(crate::verified::work_phase_update_admissible);
}

pub(super) fn update_work(
    state: &mut SchedulerState,
    id: WorkId,
    update: WorkUpdate,
) -> (found: bool)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        work_update_matches(
            old(state).spec_work(),
            final(state).spec_work(),
            id,
            update.spec_phase(),
            found,
        ),
        old(state).spec_reservation_reducer_ready()
                && crate::verified::work_phase_update_admissible(
                    old(state).spec_work(),
                    old(state).spec_reservations(),
                    id,
                    update.spec_phase(),
                )
            ==> found && final(state).spec_reservation_reducer_ready(),
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost phase = state.spec_phase();
    let ghost binding = state.spec_binding();
    let ghost workers = state.spec_workers();
    let ghost before = state.spec_work();
    let ghost reservations = state.spec_reservations();
    let ghost used_dispatches = state.spec_used_dispatches();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost target_phase = update.spec_phase();
    let ghost was_admissible = crate::verified::work_phase_update_admissible(
        before,
        reservations,
        id,
        target_phase,
    );
    let mut index = 0;
    while index < state.work.len()
        invariant
            index <= state.work@.len(),
            phase == old(state).spec_phase(),
            binding == old(state).spec_binding(),
            workers == old(state).spec_workers(),
            state.spec_work() == before,
            before == old(state).spec_work(),
            state.spec_phase() == phase,
            state.spec_binding() == binding,
            state.spec_workers() == workers,
            target_phase == update.spec_phase(),
            forall |prior: int| #![auto]
                0 <= prior < index ==>
                    before[prior].spec_definition().spec_id() != id,
            had_invariant == old(state).spec_reservation_invariant(),
            reservations == old(state).spec_reservations(),
            used_dispatches == old(state).spec_used_dispatches(),
            was_ready == old(state).spec_reservation_reducer_ready(),
            was_admissible == crate::verified::work_phase_update_admissible(
                before,
                reservations,
                id,
                target_phase,
            ),
            state.spec_reservations() == reservations,
            state.spec_used_dispatches() == used_dispatches,
            had_invariant ==> state.spec_reservation_invariant(),
        decreases state.work@.len() - index,
    {
        if state.work[index].spec().id().same(&id) {
            proof {
                assert(before[index as int].spec_definition().spec_id() == id);
            }
            update_work_at(state, index, id, update);
            return true;
        }
        proof {
            assert(before[index as int].spec_definition().spec_id() != id);
        }
        index += 1;
    }
    proof {
        assert(state.spec_work() == before);
        establish_work_not_found(
            before,
            state.spec_work(),
            reservations,
            id,
            target_phase,
            index as int,
        );
        assert(work_update_matches(
            old(state).spec_work(),
            state.spec_work(),
            id,
            update.spec_phase(),
            false,
        ));
    }
    false
}

} // verus!
