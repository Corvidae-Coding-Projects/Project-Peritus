//! Exact work-attempt mutation used by reservation admission.

use vstd::prelude::*;

use crate::SchedulerState;
#[cfg(verus_only)]
use crate::{WorkPhase, WorkRecord};

verus! {

pub fn begin_work_attempt_at(
    state: &mut SchedulerState,
    index: usize,
) -> (result: Option<crate::AttemptNumber>)
    ensures
        result.is_some() <==>
            index < old(state).spec_work().len()
                && old(state).spec_work()[index as int].spec_attempts_started() < u16::MAX
                && old(state).spec_work()[index as int].spec_attempts_started()
                    < old(state).spec_work()[index as int]
                        .spec_definition().spec_maximum_attempts().spec_value(),
        final(state).spec_binding().spec_limits()
            == old(state).spec_binding().spec_limits(),
        final(state).spec_binding().spec_capacity().spec_entries()
            == old(state).spec_binding().spec_capacity().spec_entries(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        final(state).spec_dispatch_ordinal() == old(state).spec_dispatch_ordinal(),
        final(state).spec_work().len() == old(state).spec_work().len(),
        forall |other: int| #![auto]
            0 <= other < old(state).spec_work().len() && other != index ==>
                final(state).spec_work()[other] == old(state).spec_work()[other],
        old(state).spec_reservation_invariant()
                && index < old(state).spec_work().len()
                && (forall |reservation_index: int| #![auto]
                    0 <= reservation_index < old(state).spec_reservations().len() ==>
                        old(state).spec_reservations()[reservation_index].spec_work_id()
                            != old(state).spec_work()[index as int]
                                .spec_definition().spec_id())
            ==> final(state).spec_reservation_invariant(),
        match result {
            Some(attempt) =>
                index < final(state).spec_work().len()
                    && WorkRecord::reservation_subject_equivalent(
                        &old(state).spec_work()[index as int],
                        &final(state).spec_work()[index as int],
                    )
                    && final(state).spec_work()[index as int].spec_phase()
                        == WorkPhase::Reserved
                    && attempt.spec_value()
                        == final(state).spec_work()[index as int].spec_attempts_started(),
            None => true,
        },
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost before = state.spec_work();
    proof {
        assert(state.work@ == state.spec_work());
        assert(state.work.len() == state.spec_work().len());
        assert(had_invariant == old(state).spec_reservation_invariant());
        assert(before == old(state).spec_work());
    }
    if index >= state.work.len() {
        proof {
            assert(state.spec_reservation_invariant()
                == old(state).spec_reservation_invariant());
        }
        return None;
    }
    let ghost was_unreserved = forall |reservation_index: int| #![auto]
            0 <= reservation_index < state.spec_reservations().len() ==>
                state.spec_reservations()[reservation_index].spec_work_id()
                    != state.spec_work()[index as int].spec_definition().spec_id();
    proof {
        assert(was_unreserved == (forall |reservation_index: int| #![auto]
                    0 <= reservation_index < old(state).spec_reservations().len() ==>
                        old(state).spec_reservations()[reservation_index].spec_work_id()
                            != old(state).spec_work()[index as int]
                                .spec_definition().spec_id()
        ));
    }
    let result = state.work[index].try_begin_attempt();
    proof {
        if result.is_some() {
            assert(before.len() == state.spec_work().len());
            assert(WorkRecord::reservation_subject_equivalent(
                &before[index as int],
                &state.spec_work()[index as int],
            ));
            assert forall |other: int| #![auto]
                0 <= other < before.len() && other != index
                    implies before[other] == state.spec_work()[other] by {
            }
            if had_invariant && was_unreserved {
                assert forall |reservation_index: int| #![auto]
                    0 <= reservation_index < state.spec_reservations().len()
                    implies state.spec_reservations()[reservation_index].spec_work_id()
                        != before[index as int].spec_definition().spec_id() by {
                }
                crate::verified::actual_unreserved_work_update_preserves(
                    state.spec_binding(),
                    state.spec_workers(),
                    before,
                    state.spec_work(),
                    state.spec_reservations(),
                    index as int,
                );
            }
        } else {
            assert(before.len() == state.spec_work().len());
            WorkRecord::reservation_lifecycle_fields(
                &before[index as int],
                &state.spec_work()[index as int],
            );
            assert(WorkRecord::reservation_binding_equivalent(
                &before[index as int],
                &state.spec_work()[index as int],
            ));
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
        }
        if had_invariant && was_unreserved {
            assert(state.spec_reservation_invariant());
        }
        if old(state).spec_reservation_invariant()
            && index < old(state).spec_work().len()
            && forall |reservation_index: int| #![auto]
                0 <= reservation_index < old(state).spec_reservations().len() ==>
                    old(state).spec_reservations()[reservation_index].spec_work_id()
                        != old(state).spec_work()[index as int].spec_definition().spec_id()
        {
            assert(had_invariant);
            assert(was_unreserved);
            assert(state.spec_reservation_invariant());
        }
    }
    result
}

} // verus!
