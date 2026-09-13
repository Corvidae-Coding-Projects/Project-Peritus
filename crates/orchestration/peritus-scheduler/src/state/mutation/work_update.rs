//! Verified reservation-stable work lifecycle mutations.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::WorkRecord;
use crate::{SchedulerState, WorkId, WorkPhase, WorkTerminal};
use peritus_types::Sha256Digest;

mod apply_at;
mod attempt;
mod release;
mod wrappers;

use apply_at::update_work_at;
pub use attempt::begin_work_attempt_at;
#[cfg(verus_only)]
pub(crate) use release::{phase_release_matches, release_target_exists, terminal_release_matches};
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

    pub(super) open spec fn record_matches(
        &self,
        before: WorkRecord,
        after: WorkRecord,
        id: WorkId,
    ) -> bool {
        match self {
            Self::Phase(phase) => {
                &&& work_record_update_matches(before, after, id, *phase)
                &&& after.spec_retry_cause() == before.spec_retry_cause()
                &&& after.spec_terminal() == before.spec_terminal()
            },
            Self::QueueRetry => {
                &&& work_record_update_matches(before, after, id, WorkPhase::Queued)
                &&& after.spec_retry_cause().is_none()
                &&& after.spec_terminal() == before.spec_terminal()
            },
            Self::RetryPending(cause) => {
                &&& work_record_update_matches(before, after, id, WorkPhase::RetryPending)
                &&& after.spec_retry_cause() == Some(*cause)
                &&& after.spec_terminal() == before.spec_terminal()
            },
            Self::Terminal(terminal) => {
                &&& work_record_update_matches(before, after, id, WorkPhase::Terminal)
                &&& after.spec_retry_cause().is_none()
                &&& after.spec_terminal() == Some(*terminal)
            },
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
    &&& WorkRecord::lifecycle_update_stable(&before, &after)
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

/// Relates the complete work sequence and exact field effect of one concrete update.
pub(super) open spec fn exact_work_update_matches(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    update: WorkUpdate,
    found: bool,
) -> bool {
    &&& before.len() == after.len()
    &&& if found {
        exists |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& update.record_matches(before[index], after[index], id)
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

/// Relates an exact phase-only update, including every preserved target field.
pub open spec fn work_phase_update_matches(
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
            &&& after[index].spec_retry_cause() == before[index].spec_retry_cause()
            &&& after[index].spec_terminal() == before[index].spec_terminal()
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

/// Relates exact terminalization, including the supplied terminal payload.
pub open spec fn work_terminal_update_matches(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    id: WorkId,
    terminal: WorkTerminal,
    found: bool,
) -> bool {
    &&& before.len() == after.len()
    &&& if found {
        exists |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& work_record_update_matches(
                before[index], after[index], id, WorkPhase::Terminal,
            )
            &&& after[index].spec_retry_cause().is_none()
            &&& after[index].spec_terminal() == Some(terminal)
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

/// Relates every scheduler field outside the retained work sequence.
pub open spec fn work_update_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& after.spec_phase() == before.spec_phase()
    &&& after.spec_sequence() == before.spec_sequence()
    &&& after.spec_last_event_id() == before.spec_last_event_id()
    &&& after.spec_state_digest() == before.spec_state_digest()
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_workers() == before.spec_workers()
    &&& after.spec_reservations() == before.spec_reservations()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_enqueue_ordinal() == before.spec_enqueue_ordinal()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_used_commands() == before.spec_used_commands()
    &&& after.spec_terminal() == before.spec_terminal()
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
        work_update_preserves_other_state(old(state), final(state)),
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
        exact_work_update_matches(
            old(state).spec_work(),
            final(state).spec_work(),
            id,
            update,
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
            work_update_preserves_other_state(old(state), state),
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
            proof { reveal(work_update_preserves_other_state); }
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
        reveal(exact_work_update_matches);
        assert(exact_work_update_matches(
            old(state).spec_work(),
            state.spec_work(),
            id,
            update,
            false,
        ));
    }
    false
}

} // verus!
