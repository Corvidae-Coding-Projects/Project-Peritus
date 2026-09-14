//! Inert effect directives reconstructed from committed state.

#[cfg(verus_only)]
use crate::WorkRecord;
use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkId, WorkPhase, WorkerId};
use vstd::prelude::*;

/// Effect-shell work derived only from already durable scheduler state.
// Direct verification preserves the documented enum fields without relying on synthesized
// projections from the pinned Verus macro.
#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Debug, Eq, PartialEq)]
pub enum SchedulerDirective {
    /// Idempotently deliver an unacknowledged committed dispatch.
    Dispatch(SchedulerReservation),
    /// Ask an owner to terminate one cancelling dispatch.
    Cancel {
        /// Cancelling dispatch identity.
        dispatch_id: DispatchId,
        /// Work whose execution must stop.
        work_id: WorkId,
        /// Worker receiving the cancellation request.
        worker_id: WorkerId,
    },
}

verus! {

impl SchedulerDirective {
    /// Relates an exact semantic clone of an inert runtime directive.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        match (left, right) {
            (Self::Dispatch(left), Self::Dispatch(right)) => {
                SchedulerReservation::clone_equivalent(left, right)
            },
            (
                Self::Cancel {
                    dispatch_id: left_dispatch,
                    work_id: left_work,
                    worker_id: left_worker,
                },
                Self::Cancel {
                    dispatch_id: right_dispatch,
                    work_id: right_work,
                    worker_id: right_worker,
                },
            ) => {
                left_dispatch == right_dispatch
                    && left_work == right_work
                    && left_worker == right_worker
            },
            _ => false,
        }
    }
}

impl Clone for SchedulerDirective {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        match self {
            Self::Dispatch(reservation) => Self::Dispatch(reservation.clone()),
            Self::Cancel { dispatch_id, work_id, worker_id } => Self::Cancel {
                dispatch_id: *dispatch_id,
                work_id: *work_id,
                worker_id: *worker_id,
            },
        }
    }
}

/// Returns whether a retained reservation contributes one pending runtime directive.
pub open spec fn reservation_is_pending(
    work: Seq<WorkRecord>,
    reservation: SchedulerReservation,
) -> bool {
    exists |work_index: int| #![trigger work[work_index]] {
        &&& 0 <= work_index < work.len()
        &&& work[work_index].spec_definition().spec_id() == reservation.spec_work_id()
        &&& (work[work_index].spec_phase() == WorkPhase::Reserved
                && !reservation.spec_started()
            || work[work_index].spec_phase() == WorkPhase::Cancelling)
    }
}

/// Relates one emitted directive to the exact reservation and retained lifecycle behind it.
pub open spec fn directive_matches_reservation(
    work: Seq<WorkRecord>,
    directive: SchedulerDirective,
    reservation: SchedulerReservation,
) -> bool {
    match directive {
        SchedulerDirective::Dispatch(observed) => {
            &&& SchedulerReservation::clone_equivalent(&reservation, &observed)
            &&& !reservation.spec_started()
            &&& exists |work_index: int| #![trigger work[work_index]] {
                &&& 0 <= work_index < work.len()
                &&& work[work_index].spec_definition().spec_id()
                    == reservation.spec_work_id()
                &&& work[work_index].spec_phase() == WorkPhase::Reserved
            }
        },
        SchedulerDirective::Cancel { dispatch_id, work_id, worker_id } => {
            &&& dispatch_id == reservation.spec_dispatch_id()
            &&& work_id == reservation.spec_work_id()
            &&& worker_id == reservation.spec_worker_id()
            &&& exists |work_index: int| #![trigger work[work_index]] {
                &&& 0 <= work_index < work.len()
                &&& work[work_index].spec_definition().spec_id()
                    == reservation.spec_work_id()
                &&& work[work_index].spec_phase() == WorkPhase::Cancelling
            }
        },
    }
}

/// Relates a directive sequence to its reservation-order filter and map sources.
pub open spec fn directives_match_sources(
    work: Seq<WorkRecord>,
    directives: Seq<SchedulerDirective>,
    sources: Seq<SchedulerReservation>,
) -> bool {
    &&& directives.len() == sources.len()
    &&& forall |index: int| #![trigger directives[index], sources[index]]
        0 <= index < directives.len() ==>
            directive_matches_reservation(work, directives[index], sources[index])
}

/// Exact canonical reservation-order filter/map, truncated to the configured batch size.
pub open spec fn pending_directives_exact(
    state: &SchedulerState,
    directives: Seq<SchedulerDirective>,
    batch_size: nat,
) -> bool {
    exists |cutoff: int| {
        &&& 0 <= cutoff <= state.spec_reservations().len()
        &&& directives_match_sources(
            state.spec_work(),
            directives,
            state.spec_reservations().take(cutoff).filter(
                |reservation: SchedulerReservation|
                    reservation_is_pending(state.spec_work(), reservation),
            ),
        )
        &&& directives.len() <= batch_size
        &&& (cutoff == state.spec_reservations().len() || directives.len() == batch_size)
    }
}

proof fn matching_work_fixes_phase(
    state: &SchedulerState,
    reservation: SchedulerReservation,
    observed: WorkRecord,
)
    requires
        state.spec_reservation_reducer_ready(),
        exists |observed_index: int| #![trigger state.spec_work()[observed_index]] {
            &&& 0 <= observed_index < state.spec_work().len()
            &&& state.spec_work()[observed_index].spec_definition().spec_id()
                == reservation.spec_work_id()
            &&& state.spec_work()[observed_index] == observed
        },
    ensures forall |index: int| #![trigger state.spec_work()[index]]
        0 <= index < state.spec_work().len()
            && state.spec_work()[index].spec_definition().spec_id()
                == reservation.spec_work_id() ==>
                    state.spec_work()[index].spec_phase() == observed.spec_phase(),
{
    reveal(crate::verified::reservation_invariant_parts);
    reveal(crate::verified::work_identities_unique);
    let observed_index = choose |observed_index: int|
        #![trigger state.spec_work()[observed_index]] {
        &&& 0 <= observed_index < state.spec_work().len()
        &&& state.spec_work()[observed_index].spec_definition().spec_id()
            == reservation.spec_work_id()
        &&& state.spec_work()[observed_index] == observed
    };
    assert forall |index: int| #![trigger state.spec_work()[index]]
        0 <= index < state.spec_work().len()
            && state.spec_work()[index].spec_definition().spec_id()
                == reservation.spec_work_id()
        implies state.spec_work()[index].spec_phase() == observed.spec_phase() by {
        if index != observed_index {
            assert(state.spec_work()[index].spec_definition().spec_id()
                != state.spec_work()[observed_index].spec_definition().spec_id());
        } else {
            assert(state.spec_work()[index] == observed);
        }
    }
}

proof fn matching_sequences_push(
    work: Seq<WorkRecord>,
    directives: Seq<SchedulerDirective>,
    sources: Seq<SchedulerReservation>,
    directive: SchedulerDirective,
    source: SchedulerReservation,
)
    requires
        directives_match_sources(work, directives, sources),
        directive_matches_reservation(work, directive, source),
    ensures directives_match_sources(
        work,
        directives.push(directive),
        sources.push(source),
    ),
{
    reveal(directives_match_sources);
    assert forall |index: int|
        #![trigger directives.push(directive)[index], sources.push(source)[index]]
        0 <= index < directives.push(directive).len()
        implies directive_matches_reservation(
            work,
            directives.push(directive)[index],
            sources.push(source)[index],
        ) by {
        if index < directives.len() {
            assert(directives.push(directive)[index] == directives[index]);
            assert(sources.push(source)[index] == sources[index]);
        } else {
            assert(index == directives.len());
        }
    }
}

fn directive_for(
    state: &SchedulerState,
    reservation: &SchedulerReservation,
) -> (result: Option<SchedulerDirective>)
    ensures state.spec_reservation_reducer_ready() && state.spec_work_ordered() ==> match result {
        Some(directive) => {
            &&& reservation_is_pending(state.spec_work(), *reservation)
            &&& directive_matches_reservation(state.spec_work(), directive, *reservation)
        },
        None => !reservation_is_pending(state.spec_work(), *reservation),
    },
{
    let found = state.work_item(reservation.work_id());
    let Some(record) = found else {
        proof {
            if state.spec_work_ordered() {
                assert(!reservation_is_pending(state.spec_work(), *reservation));
            }
        }
        return None;
    };
    proof {
        if state.spec_reservation_reducer_ready() {
            matching_work_fixes_phase(state, *reservation, *record);
        }
    }
    let phase = record.phase();
    if phase.same(WorkPhase::Reserved) && !reservation.started() {
        let directive = SchedulerDirective::Dispatch(reservation.clone());
        proof {
            if state.spec_reservation_reducer_ready() {
                assert(reservation_is_pending(state.spec_work(), *reservation));
                assert(directive_matches_reservation(
                    state.spec_work(), directive, *reservation,
                ));
            }
        }
        Some(directive)
    } else if phase.same(WorkPhase::Cancelling) {
        let directive = SchedulerDirective::Cancel {
            dispatch_id: reservation.dispatch_id(),
            work_id: reservation.work_id(),
            worker_id: reservation.worker_id(),
        };
        proof {
            if state.spec_reservation_reducer_ready() {
                assert(reservation_is_pending(state.spec_work(), *reservation));
                assert(directive_matches_reservation(
                    state.spec_work(), directive, *reservation,
                ));
            }
        }
        Some(directive)
    } else {
        proof {
            if state.spec_reservation_reducer_ready() {
                assert(!reservation_is_pending(state.spec_work(), *reservation));
            }
        }
        None
    }
}

/// Returns bounded pending directives in canonical dispatch order.
#[must_use]
pub fn pending_directives(state: &SchedulerState) -> (result: Vec<SchedulerDirective>)
    ensures
        state.spec_reservation_reducer_ready() && state.spec_work_ordered() ==>
            pending_directives_exact(
                state,
                result@,
                state.spec_binding().spec_limits().spec_dispatch_batch_size() as nat,
            ),
{
    let reservations = state.reservations();
    let batch_size = state.binding().limits().dispatch_batch_size() as usize;
    let mut result = Vec::new();
    let mut index = 0;
    proof {
        reveal(Seq::filter);
        assert(reservations@.take(0) =~= Seq::<SchedulerReservation>::empty());
        assert(directives_match_sources(
            state.spec_work(),
            result@,
            Seq::<SchedulerReservation>::empty(),
        ));
    }
    while index < reservations.len() && result.len() < batch_size
        invariant
            index <= reservations@.len(),
            batch_size == state.spec_binding().spec_limits().spec_dispatch_batch_size(),
            result@.len() <= batch_size,
            state.spec_reservation_reducer_ready() && state.spec_work_ordered() ==>
                directives_match_sources(
                    state.spec_work(),
                    result@,
                    reservations@.take(index as int).filter(
                        |reservation: SchedulerReservation|
                            reservation_is_pending(state.spec_work(), reservation),
                    ),
                ),
        decreases reservations@.len() - index,
    {
        let reservation = &reservations[index];
        let ghost previous = result@;
        let ghost prior_sources = reservations@.take(index as int).filter(
            |source: SchedulerReservation|
                reservation_is_pending(state.spec_work(), source),
        );
        let directive = directive_for(state, reservation);
        if let Some(value) = directive {
            proof {
                if state.spec_reservation_reducer_ready() && state.spec_work_ordered() {
                    matching_sequences_push(
                        state.spec_work(), previous, prior_sources, value, *reservation,
                    );
                }
            }
            result.push(value);
        }
        proof {
            if state.spec_reservation_reducer_ready() && state.spec_work_ordered() {
                let prefix = reservations@.take(index as int);
                let source = *reservation;
                prefix.lemma_filter_push(source, |candidate: SchedulerReservation|
                    reservation_is_pending(state.spec_work(), candidate));
                assert(prefix.push(source) =~= reservations@.take(index as int + 1));
                let next_sources = prefix.push(source).filter(
                    |candidate: SchedulerReservation|
                        reservation_is_pending(state.spec_work(), candidate),
                );
                if reservation_is_pending(state.spec_work(), source) {
                    assert(result@.len() == previous.len() + 1);
                    assert(next_sources =~= prior_sources.push(source));
                    assert(result@ =~= previous.push(result@[result@.len() - 1]));
                } else {
                    assert(result@ =~= previous);
                    assert(next_sources =~= prior_sources);
                }
            }
        }
        index += 1;
    }
    proof {
        if state.spec_reservation_reducer_ready() && state.spec_work_ordered() {
            assert(index == reservations@.len() || result@.len() == batch_size);
            assert(pending_directives_exact(
                state,
                result@,
                state.spec_binding().spec_limits().spec_dispatch_batch_size() as nat,
            )) by {
                let cutoff = index as int;
                assert(0 <= cutoff <= state.spec_reservations().len());
                assert(reservations@ == state.spec_reservations());
            }
        }
    }
    result
}

} // verus!
