//! Composition of exact cancellation steps into an ordered successful prefix.

use super::frame::{
    cancellation_step_preserves_ids, cancellation_step_preserves_other_record,
    cancellation_step_target_exists, target_existence_preserved,
};
use super::{
    all_targets_exist, cancellation_final_matches, cancellation_result_matches,
    cancellation_step_matches, cancellation_updates_match, target_exists,
    unselected_work_unchanged, work_id_layout_matches,
};
use crate::{SchedulerReservation, WorkId, WorkRecord};
use vstd::prelude::*;

verus! {

pub(super) proof fn empty_updates(
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
)
    ensures cancellation_updates_match(
        work,
        work,
        reservations,
        Seq::<WorkId>::empty(),
    ),
{
    let states = Seq::<Seq<WorkRecord>>::empty().push(work);
    assert(states.len() == 1);
    assert(states[0] == work);
    assert(exists |candidate: Seq<Seq<WorkRecord>>| {
        &&& candidate.len() == 1
        &&& candidate[0] == work
        &&& candidate[0] == work
        &&& forall |step: int| 0 <= step < 0 ==>
            cancellation_step_matches(
                candidate[step],
                candidate[step + 1],
                reservations,
                Seq::<WorkId>::empty()[step],
            )
    }) by {
        assert forall |step: int| 0 <= step < 0 implies
            cancellation_step_matches(
                states[step],
                states[step + 1],
                reservations,
                Seq::<WorkId>::empty()[step],
            ) by {
        }
    }
    reveal(cancellation_updates_match);
}

pub(super) proof fn append_update(
    before: Seq<WorkRecord>,
    middle: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    affected: Seq<WorkId>,
    id: WorkId,
)
    requires
        cancellation_updates_match(before, middle, reservations, affected),
        cancellation_step_matches(middle, after, reservations, id),
    ensures cancellation_updates_match(before, after, reservations, affected.push(id)),
{
    reveal(cancellation_updates_match);
    let states = choose |states: Seq<Seq<WorkRecord>>| {
        &&& states.len() == affected.len() + 1
        &&& states[0] == before
        &&& states[affected.len() as int] == middle
        &&& forall |step: int| #![trigger states[step]] 0 <= step < affected.len() ==>
            cancellation_step_matches(
                states[step],
                states[step + 1],
                reservations,
                affected[step],
            )
    };
    let extended = states.push(after);
    assert(extended.len() == affected.push(id).len() + 1);
    assert(extended[0] == before);
    assert(extended[affected.push(id).len() as int] == after);
    assert forall |step: int| #![trigger extended[step]]
        0 <= step < affected.push(id).len() implies
        cancellation_step_matches(
            extended[step],
            extended[step + 1],
            reservations,
            affected.push(id)[step],
        ) by {
        if step < affected.len() {
            assert(extended[step] == states[step]);
            assert(extended[step + 1] == states[step + 1]);
            assert(affected.push(id)[step] == affected[step]);
        } else {
            assert(step == affected.len());
            assert(extended[step] == middle);
            assert(extended[step + 1] == after);
            assert(affected.push(id)[step] == id);
        }
    }
    assert(exists |candidate: Seq<Seq<WorkRecord>>| {
        &&& candidate.len() == affected.push(id).len() + 1
        &&& candidate[0] == before
        &&& candidate[affected.push(id).len() as int] == after
        &&& forall |step: int| #![trigger candidate[step]]
            0 <= step < affected.push(id).len() ==>
            cancellation_step_matches(
                candidate[step],
                candidate[step + 1],
                reservations,
                affected.push(id)[step],
            )
    }) by {
    }
}

pub(super) proof fn advance_prefix(
    before: Seq<WorkRecord>,
    prior_work: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    affected: Seq<WorkId>,
    index: int,
)
    requires
        0 <= index < affected.len(),
        cancellation_updates_match(
            before,
            prior_work,
            reservations,
            affected.take(index),
        ),
        work_id_layout_matches(before, prior_work),
        unselected_work_unchanged(before, prior_work, affected.take(index)),
        cancellation_step_matches(prior_work, after, reservations, affected[index]),
    ensures
        cancellation_updates_match(
            before,
            after,
            reservations,
            affected.take(index + 1),
        ),
        work_id_layout_matches(before, after),
        target_exists(before, affected[index]),
        unselected_work_unchanged(before, after, affected.take(index + 1)),
{
    // Compose the checked step/trace lemmas without re-expanding their existential witnesses.
    hide(cancellation_updates_match);
    hide(cancellation_step_matches);
    let id = affected[index];
    let prefix = affected.take(index);
    cancellation_step_preserves_ids(prior_work, after, reservations, id);
    cancellation_step_target_exists(prior_work, after, reservations, id);
    target_existence_preserved(before, prior_work, id);
    reveal(work_id_layout_matches);
    assert forall |at: int| #![trigger before[at]] 0 <= at < before.len() implies
        after[at].spec_definition().spec_id()
            == before[at].spec_definition().spec_id() by {
        assert(prior_work[at].spec_definition().spec_id()
            == before[at].spec_definition().spec_id());
    }
    append_update(before, prior_work, after, reservations, prefix, id);
    assert(prefix.push(id) =~= affected.take(index + 1));
    assert(affected.take(index + 1)[index] == id);
    assert(affected.take(index + 1).contains(id));
    assert(unselected_work_unchanged(
        before,
        after,
        affected.take(index + 1),
    )) by {
        reveal(unselected_work_unchanged);
        assert forall |at: int| #![trigger before[at]] 0 <= at < before.len()
                && !affected.take(index + 1).contains(
                    before[at].spec_definition().spec_id(),
                )
            implies after[at] == before[at] by {
            assert(!prefix.contains(before[at].spec_definition().spec_id()));
            assert(prior_work[at] == before[at]);
            assert(before[at].spec_definition().spec_id() != id);
            cancellation_step_preserves_other_record(
                prior_work,
                after,
                reservations,
                id,
                at,
            );
        }
    }
}

pub(super) proof fn establish_missing_result(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    affected: Seq<WorkId>,
    index: int,
)
    requires
        0 <= index < affected.len(),
        cancellation_updates_match(
            before,
            after,
            reservations,
            affected.take(index),
        ),
        !target_exists(before, affected[index]),
        forall |prior: int| #![trigger affected[prior]] 0 <= prior < index ==>
            target_exists(before, affected[prior]),
    ensures
        cancellation_result_matches(before, after, reservations, affected, false),
        !all_targets_exist(before, affected),
{
    let processed = index;
    reveal(cancellation_result_matches);
    assert(exists |witness: int| {
        &&& 0 <= witness <= affected.len()
        &&& cancellation_updates_match(
            before,
            after,
            reservations,
            affected.take(witness),
        )
        &&& witness < affected.len()
        &&& !target_exists(before, affected[witness])
        &&& forall |prior: int| #![trigger affected[prior]] 0 <= prior < witness ==>
            target_exists(before, affected[prior])
    }) by {
        assert(0 <= processed < affected.len());
    }
    reveal(all_targets_exist);
}

pub(super) proof fn establish_complete_result(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    affected: Seq<WorkId>,
)
    requires
        cancellation_updates_match(before, after, reservations, affected),
        unselected_work_unchanged(before, after, affected),
        all_targets_exist(before, affected),
    ensures
        cancellation_result_matches(before, after, reservations, affected, true),
        cancellation_final_matches(before, after, reservations, affected),
{
    hide(cancellation_updates_match);
    reveal(cancellation_result_matches);
    reveal(cancellation_final_matches);
}

proof fn cancelling_record_survives_suffix(
    states: Seq<Seq<WorkRecord>>,
    reservations: Seq<SchedulerReservation>,
    affected: Seq<WorkId>,
    id: WorkId,
    selected: int,
    changed: int,
    step: int,
)
    requires
        affected.no_duplicates(),
        0 <= selected < affected.len(),
        affected[selected] == id,
        selected + 1 <= step <= affected.len(),
        states.len() == affected.len() + 1,
        forall |at: int| #![trigger states[at]] 0 <= at < affected.len() ==>
            cancellation_step_matches(
                states[at], states[at + 1], reservations, affected[at],
            ),
        0 <= changed < states[step].len(),
        states[step][changed].spec_definition().spec_id() == id,
        states[step][changed].spec_phase() == crate::WorkPhase::Cancelling,
    ensures
        0 <= changed < states[affected.len() as int].len(),
        states[affected.len() as int][changed].spec_definition().spec_id() == id,
        states[affected.len() as int][changed].spec_phase()
            == crate::WorkPhase::Cancelling,
    decreases affected.len() - step,
{
    if step < affected.len() {
        assert(affected[step] != id) by {
            if affected[step] == id {
                assert(affected[step] == affected[selected]);
                assert(false);
            }
        }
        cancellation_step_preserves_other_record(
            states[step], states[step + 1], reservations, affected[step], changed,
        );
        cancelling_record_survives_suffix(
            states, reservations, affected, id, selected, changed, step + 1,
        );
    }
}

/// A uniquely selected identity retaining a reservation ends the exact cancellation trace in
/// `Cancelling`, even when later selected identities are updated in the same batch.
pub(crate) proof fn active_target_finishes_cancelling(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    affected: Seq<WorkId>,
    id: WorkId,
)
    requires
        cancellation_updates_match(before, after, reservations, affected),
        affected.no_duplicates(),
        affected.contains(id),
        super::has_active_reservation(reservations, id),
    ensures exists |index: int| #![trigger after[index]]
        0 <= index < after.len()
            && after[index].spec_definition().spec_id() == id
            && after[index].spec_phase() == crate::WorkPhase::Cancelling,
{
    reveal(cancellation_updates_match);
    let states = choose |states: Seq<Seq<WorkRecord>>| {
        &&& states.len() == affected.len() + 1
        &&& states[0] == before
        &&& states[affected.len() as int] == after
        &&& forall |step: int| #![trigger states[step]] 0 <= step < affected.len() ==>
            cancellation_step_matches(
                states[step], states[step + 1], reservations, affected[step],
            )
    };
    let selected = choose |step: int| 0 <= step < affected.len() && affected[step] == id;
    reveal(cancellation_step_matches);
    reveal(crate::state::mutation::work_phase_update_matches);
    let changed = choose |index: int| #![trigger states[selected][index]] {
        &&& 0 <= index < states[selected].len()
        &&& crate::state::mutation::work_record_update_matches(
            states[selected][index], states[selected + 1][index], id,
            crate::WorkPhase::Cancelling,
        )
        &&& forall |other: int| #![auto]
            0 <= other < states[selected].len() && other != index ==>
                states[selected + 1][other] == states[selected][other]
    };
    reveal(crate::state::mutation::work_record_update_matches);
    WorkRecord::lifecycle_update_fields(
        &states[selected][changed],
        &states[selected + 1][changed],
    );
    assert(states[selected + 1][changed].spec_definition().spec_id() == id);
    assert(states[selected + 1][changed].spec_phase() == crate::WorkPhase::Cancelling);
    cancelling_record_survives_suffix(
        states, reservations, affected, id, selected, changed, selected + 1,
    );
    assert(states[affected.len() as int] == after);
}

} // verus!
