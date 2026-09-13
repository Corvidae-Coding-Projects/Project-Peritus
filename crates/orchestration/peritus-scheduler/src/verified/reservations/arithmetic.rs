//! Sequence arithmetic used by reservation insertion and release proofs.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceEntry, ResourceKind, SchedulerReservation};

#[cfg(verus_only)]
use super::{reservation_quantity, vector_quantity, worker_count, worker_quantity};

verus! {

proof fn inserted_tail<A>(values: Seq<A>, at: int, value: A)
    requires 0 < at <= values.len()
    ensures values.insert(at, value).drop_first() =~= values.drop_first().insert(at - 1, value)
{
    values.insert_ensures(at, value);
    values.drop_first().insert_ensures(at - 1, value);
    assert forall |index: int|
        0 <= index < values.insert(at, value).drop_first().len()
        implies values.insert(at, value).drop_first()[index]
            == values.drop_first().insert(at - 1, value)[index] by {
        if index < at - 1 {
            assert(values.insert(at, value)[index + 1] == values[index + 1]);
            assert(values.drop_first().insert(at - 1, value)[index]
                == values.drop_first()[index]);
        } else if index == at - 1 {
            assert(values.insert(at, value)[index + 1] == value);
            assert(values.drop_first().insert(at - 1, value)[index] == value);
        } else {
            assert(values.insert(at, value)[index + 1] == values[index]);
            assert(values.drop_first().insert(at - 1, value)[index]
                == values.drop_first()[index - 1]);
        }
    }
}

proof fn removed_tail<A>(values: Seq<A>, at: int)
    requires 0 < at < values.len()
    ensures values.remove(at).drop_first() =~= values.drop_first().remove(at - 1)
{
    values.remove_ensures(at);
    values.drop_first().remove_ensures(at - 1);
    assert forall |index: int|
        0 <= index < values.remove(at).drop_first().len()
        implies values.remove(at).drop_first()[index]
            == values.drop_first().remove(at - 1)[index] by {
        if index < at - 1 {
            assert(values.remove(at)[index + 1] == values[index + 1]);
            assert(values.drop_first().remove(at - 1)[index] == values.drop_first()[index]);
        } else {
            assert(values.remove(at)[index + 1] == values[index + 2]);
            assert(values.drop_first().remove(at - 1)[index]
                == values.drop_first()[index + 1]);
        }
    }
}

proof fn inserted_zero_tail<A>(values: Seq<A>, value: A)
    ensures values.insert(0, value).drop_first() =~= values
{
    values.insert_ensures(0, value);
    assert forall |index: int| #![auto] 0 <= index < values.len()
        implies values.insert(0, value).drop_first()[index] == values[index] by {
        assert(values.insert(0, value)[index + 1] == values[index]);
    }
}

proof fn removed_zero_is_tail<A>(values: Seq<A>)
    requires 0 < values.len()
    ensures values.remove(0) =~= values.drop_first()
{
    values.remove_ensures(0);
    assert forall |index: int| #![auto] 0 <= index < values.remove(0).len()
        implies values.remove(0)[index] == values.drop_first()[index] by {
        assert(values.remove(0)[index] == values[index + 1]);
    }
}

pub(super) proof fn vector_quantity_nonnegative(entries: Seq<ResourceEntry>, kind: ResourceKind)
    ensures 0 <= vector_quantity(entries, kind)
    decreases entries.len(),
{
    if entries.len() > 0 {
        vector_quantity_nonnegative(entries.drop_first(), kind);
    }
}

pub(super) proof fn reservation_quantity_insert(
    reservations: Seq<SchedulerReservation>,
    reservation: SchedulerReservation,
    at: int,
    kind: ResourceKind,
)
    requires 0 <= at <= reservations.len()
    ensures
        reservation_quantity(reservations.insert(at, reservation), kind)
            == reservation_quantity(reservations, kind)
                + vector_quantity(reservation.spec_resources().spec_entries(), kind)
    decreases reservations.len(),
{
    if at == 0 {
        reservations.insert_ensures(at, reservation);
        inserted_zero_tail(reservations, reservation);
    } else {
        inserted_tail(reservations, at, reservation);
        reservation_quantity_insert(reservations.drop_first(), reservation, at - 1, kind);
    }
}

pub(super) proof fn worker_quantity_insert(
    reservations: Seq<SchedulerReservation>,
    reservation: SchedulerReservation,
    at: int,
    worker: crate::WorkerId,
    kind: ResourceKind,
)
    requires 0 <= at <= reservations.len()
    ensures
        worker_quantity(reservations.insert(at, reservation), worker, kind)
            == worker_quantity(reservations, worker, kind)
                + if reservation.spec_worker_id() == worker {
                    vector_quantity(reservation.spec_resources().spec_entries(), kind)
                } else {
                    0
                }
    decreases reservations.len(),
{
    if at == 0 {
        reservations.insert_ensures(at, reservation);
        inserted_zero_tail(reservations, reservation);
    } else {
        inserted_tail(reservations, at, reservation);
        worker_quantity_insert(reservations.drop_first(), reservation, at - 1, worker, kind);
    }
}

pub(super) proof fn worker_count_insert(
    reservations: Seq<SchedulerReservation>,
    reservation: SchedulerReservation,
    at: int,
    worker: crate::WorkerId,
)
    requires 0 <= at <= reservations.len()
    ensures
        worker_count(reservations.insert(at, reservation), worker)
            == worker_count(reservations, worker)
                + if reservation.spec_worker_id() == worker { 1int } else { 0int }
    decreases reservations.len(),
{
    if at == 0 {
        reservations.insert_ensures(at, reservation);
        inserted_zero_tail(reservations, reservation);
    } else {
        inserted_tail(reservations, at, reservation);
        worker_count_insert(reservations.drop_first(), reservation, at - 1, worker);
    }
}

pub(super) proof fn reservation_quantity_remove(
    reservations: Seq<SchedulerReservation>,
    at: int,
    kind: ResourceKind,
)
    requires 0 <= at < reservations.len()
    ensures
        reservation_quantity(reservations.remove(at), kind)
                + vector_quantity(reservations[at].spec_resources().spec_entries(), kind)
            == reservation_quantity(reservations, kind)
    decreases reservations.len(),
{
    if at == 0 {
        removed_zero_is_tail(reservations);
    } else {
        removed_tail(reservations, at);
        reservation_quantity_remove(reservations.drop_first(), at - 1, kind);
    }
}

pub(super) proof fn worker_quantity_remove(
    reservations: Seq<SchedulerReservation>,
    at: int,
    worker: crate::WorkerId,
    kind: ResourceKind,
)
    requires 0 <= at < reservations.len()
    ensures
        worker_quantity(reservations.remove(at), worker, kind)
                + if reservations[at].spec_worker_id() == worker {
                    vector_quantity(reservations[at].spec_resources().spec_entries(), kind)
                } else {
                    0
                }
            == worker_quantity(reservations, worker, kind)
    decreases reservations.len(),
{
    if at == 0 {
        removed_zero_is_tail(reservations);
    } else {
        removed_tail(reservations, at);
        worker_quantity_remove(reservations.drop_first(), at - 1, worker, kind);
    }
}

pub(super) proof fn worker_count_remove(
    reservations: Seq<SchedulerReservation>,
    at: int,
    worker: crate::WorkerId,
)
    requires 0 <= at < reservations.len()
    ensures
        worker_count(reservations.remove(at), worker)
                + if reservations[at].spec_worker_id() == worker { 1int } else { 0int }
            == worker_count(reservations, worker)
    decreases reservations.len(),
{
    if at == 0 {
        removed_zero_is_tail(reservations);
    } else {
        removed_tail(reservations, at);
        worker_count_remove(reservations.drop_first(), at - 1, worker);
    }
}

pub(super) proof fn reservation_quantity_nonnegative(
    reservations: Seq<SchedulerReservation>,
    kind: ResourceKind,
)
    ensures 0 <= reservation_quantity(reservations, kind)
    decreases reservations.len(),
{
    if reservations.len() > 0 {
        vector_quantity_nonnegative(reservations.first().spec_resources().spec_entries(), kind);
        reservation_quantity_nonnegative(reservations.drop_first(), kind);
    }
}

pub(super) proof fn worker_quantity_nonnegative(
    reservations: Seq<SchedulerReservation>,
    worker: crate::WorkerId,
    kind: ResourceKind,
)
    ensures 0 <= worker_quantity(reservations, worker, kind)
    decreases reservations.len(),
{
    if reservations.len() > 0 {
        vector_quantity_nonnegative(reservations.first().spec_resources().spec_entries(), kind);
        worker_quantity_nonnegative(reservations.drop_first(), worker, kind);
    }
}

pub(super) proof fn worker_count_nonnegative(
    reservations: Seq<SchedulerReservation>,
    worker: crate::WorkerId,
)
    ensures 0 <= worker_count(reservations, worker)
    decreases reservations.len(),
{
    if reservations.len() > 0 {
        worker_count_nonnegative(reservations.drop_first(), worker);
    }
}

} // verus!
