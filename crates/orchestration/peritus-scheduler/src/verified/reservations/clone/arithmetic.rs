//! Exact capacity transport across pointwise-equivalent reservation sequences.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceKind, SchedulerReservation};

#[cfg(verus_only)]
use super::super::{reservation_quantity, worker_count, worker_quantity};

verus! {

proof fn tails_are_equivalent(
    before: Seq<SchedulerReservation>,
    after: Seq<SchedulerReservation>,
)
    requires
        0 < before.len(),
        before.len() == after.len(),
        forall |index: int| #![auto]
            0 <= index < before.len() ==>
                SchedulerReservation::invariant_equivalent(&before[index], &after[index]),
    ensures
        before.drop_first().len() == after.drop_first().len(),
        forall |index: int| #![auto]
            0 <= index < before.drop_first().len() ==>
                SchedulerReservation::invariant_equivalent(
                    &before.drop_first()[index],
                    &after.drop_first()[index],
                ),
{
    assert forall |index: int| #![auto]
        0 <= index < before.drop_first().len() implies
            SchedulerReservation::invariant_equivalent(
                &before.drop_first()[index],
                &after.drop_first()[index],
            ) by {
        assert(before.drop_first()[index] == before[index + 1]);
        assert(after.drop_first()[index] == after[index + 1]);
    }
}

pub(super) proof fn reservation_quantities_match(
    before: Seq<SchedulerReservation>,
    after: Seq<SchedulerReservation>,
    kind: ResourceKind,
)
    requires
        before.len() == after.len(),
        forall |index: int| #![auto]
            0 <= index < before.len() ==>
                SchedulerReservation::invariant_equivalent(&before[index], &after[index]),
    ensures reservation_quantity(before, kind) == reservation_quantity(after, kind),
    decreases before.len(),
{
    if before.len() > 0 {
        SchedulerReservation::invariant_fields(&before.first(), &after.first());
        tails_are_equivalent(before, after);
        reservation_quantities_match(before.drop_first(), after.drop_first(), kind);
    }
}

pub(super) proof fn worker_quantities_match(
    before: Seq<SchedulerReservation>,
    after: Seq<SchedulerReservation>,
    worker: crate::WorkerId,
    kind: ResourceKind,
)
    requires
        before.len() == after.len(),
        forall |index: int| #![auto]
            0 <= index < before.len() ==>
                SchedulerReservation::invariant_equivalent(&before[index], &after[index]),
    ensures worker_quantity(before, worker, kind) == worker_quantity(after, worker, kind),
    decreases before.len(),
{
    if before.len() > 0 {
        SchedulerReservation::invariant_fields(&before.first(), &after.first());
        tails_are_equivalent(before, after);
        worker_quantities_match(before.drop_first(), after.drop_first(), worker, kind);
    }
}

pub(super) proof fn worker_counts_match(
    before: Seq<SchedulerReservation>,
    after: Seq<SchedulerReservation>,
    worker: crate::WorkerId,
)
    requires
        before.len() == after.len(),
        forall |index: int| #![auto]
            0 <= index < before.len() ==>
                SchedulerReservation::invariant_equivalent(&before[index], &after[index]),
    ensures worker_count(before, worker) == worker_count(after, worker),
    decreases before.len(),
{
    if before.len() > 0 {
        SchedulerReservation::invariant_fields(&before.first(), &after.first());
        tails_are_equivalent(before, after);
        worker_counts_match(before.drop_first(), after.drop_first(), worker);
    }
}

} // verus!
