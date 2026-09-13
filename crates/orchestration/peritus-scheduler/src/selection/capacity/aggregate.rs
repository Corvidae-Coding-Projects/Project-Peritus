//! Canonical aggregate resource sums with exact fallback semantics.

use vstd::prelude::*;

use crate::{ResourceKind, ResourceVector, SchedulerReservation, WorkerId};

verus! {

pub(super) enum Aggregate {
    Empty,
    Value(ResourceVector),
    Failed,
}

impl Aggregate {
    pub(super) open spec fn spec_quantity(&self, kind: ResourceKind) -> int {
        match self {
            Self::Value(vector) => vector.spec_quantity(kind),
            Self::Empty | Self::Failed => 0,
        }
    }

    pub(super) open spec fn spec_usable(&self) -> bool {
        !matches!(self, Self::Failed)
    }

    pub(super) fn quantity(&self, kind: ResourceKind) -> (result: u64)
        ensures result as int == self.spec_quantity(kind),
    {
        match self {
            Self::Value(vector) => vector.quantity(kind),
            Self::Empty | Self::Failed => 0,
        }
    }

    pub(super) proof fn global_quantity_matches(
        &self,
        reservations: Seq<SchedulerReservation>,
        kind: ResourceKind,
    )
        requires
            self.spec_usable(),
            global_sound(self, reservations),
        ensures
            self.spec_quantity(kind)
                == crate::verified::reservation_quantity(reservations, kind),
    {
        match self {
            Self::Empty => {
                assert(crate::verified::reservation_quantity(reservations, kind) == 0);
            }
            Self::Value(vector) => {
                assert(vector.spec_quantity(kind)
                    == crate::verified::reservation_quantity(reservations, kind));
            }
            Self::Failed => assert(false),
        }
    }

    pub(super) proof fn worker_quantity_matches(
        &self,
        reservations: Seq<SchedulerReservation>,
        worker: WorkerId,
        kind: ResourceKind,
    )
        requires
            self.spec_usable(),
            worker_sound(self, reservations, worker),
        ensures
            self.spec_quantity(kind)
                == crate::verified::worker_quantity(reservations, worker, kind),
    {
        match self {
            Self::Empty => {
                assert(crate::verified::worker_quantity(reservations, worker, kind) == 0);
            }
            Self::Value(vector) => {
                assert(vector.spec_quantity(kind)
                    == crate::verified::worker_quantity(reservations, worker, kind));
            }
            Self::Failed => assert(false),
        }
    }
}

pub(super) open spec fn global_sound(
    aggregate: &Aggregate,
    _reservations: Seq<SchedulerReservation>,
) -> bool {
    match aggregate {
        Aggregate::Empty => forall |kind: ResourceKind| #![auto]
            crate::verified::reservation_quantity(_reservations, kind) == 0,
        Aggregate::Value(vector) => forall |kind: ResourceKind| #![auto]
            vector.spec_quantity(kind)
                == crate::verified::reservation_quantity(_reservations, kind),
        Aggregate::Failed => true,
    }
}

pub(super) open spec fn worker_sound(
    aggregate: &Aggregate,
    reservations: Seq<SchedulerReservation>,
    worker: WorkerId,
) -> bool {
    match aggregate {
        Aggregate::Empty => forall |kind: ResourceKind| #![auto]
            crate::verified::worker_quantity(reservations, worker, kind) == 0,
        Aggregate::Value(vector) => forall |kind: ResourceKind| #![auto]
            vector.spec_quantity(kind)
                == crate::verified::worker_quantity(reservations, worker, kind),
        Aggregate::Failed => true,
    }
}

fn prepend_global(
    aggregate: Aggregate,
    reservations: &[SchedulerReservation],
    index: usize,
) -> (result: Aggregate)
    requires
        index < reservations@.len(),
        global_sound(
            &aggregate,
            reservations@.subrange(index as int + 1, reservations@.len() as int),
        ),
    ensures global_sound(
        &result,
        reservations@.subrange(index as int, reservations@.len() as int),
    ),
{
    let reservation = &reservations[index];
    let ghost tail = reservations@.subrange(index as int + 1, reservations@.len() as int);
    let ghost full = reservations@.subrange(index as int, reservations@.len() as int);
    assert(full.len() > 0);
    assert(full.first() == *reservation);
    assert(full.drop_first() =~= tail);
    assert(full.drop_first() == tail);
    match aggregate {
        Aggregate::Empty => {
            let vector = reservation.resources().clone();
            proof {
                assert forall |kind: ResourceKind| #![auto]
                    vector.spec_quantity(kind)
                        == crate::verified::reservation_quantity(
                            full, kind) by {
                    assert(crate::verified::reservation_quantity(tail, kind) == 0);
                    vector.quantity_matches_entries(kind);
                    reservation.spec_resources().quantity_matches_entries(kind);
                    assert(vector.spec_entries()
                        == reservation.spec_resources().spec_entries());
                    assert(vector.spec_quantity(kind)
                        == reservation.spec_resources().spec_quantity(kind));
                    reveal(crate::verified::vector_quantity);
                    reveal(crate::verified::reservation_quantity);
                }
            }
            Aggregate::Value(vector)
        }
        Aggregate::Value(current) => {
            let Some(vector) = current.aggregate_add(reservation.resources()) else {
                return Aggregate::Failed;
            };
            proof {
                assert forall |kind: ResourceKind| #![auto]
                    vector.spec_quantity(kind)
                        == crate::verified::reservation_quantity(
                            full, kind) by {
                    assert(current.spec_quantity(kind)
                        == crate::verified::reservation_quantity(tail, kind));
                    assert(vector.spec_quantity(kind)
                        == current.spec_quantity(kind)
                            + reservation.spec_resources().spec_quantity(kind));
                    reservation.spec_resources().quantity_matches_entries(kind);
                    reveal(crate::verified::vector_quantity);
                    reveal(crate::verified::reservation_quantity);
                }
            }
            Aggregate::Value(vector)
        }
        Aggregate::Failed => Aggregate::Failed,
    }
}

fn prepend_worker(
    aggregate: Aggregate,
    reservations: &[SchedulerReservation],
    index: usize,
    worker: WorkerId,
) -> (result: Aggregate)
    requires
        index < reservations@.len(),
        worker_sound(
            &aggregate,
            reservations@.subrange(index as int + 1, reservations@.len() as int),
            worker,
        ),
    ensures worker_sound(
        &result,
        reservations@.subrange(index as int, reservations@.len() as int),
        worker,
    ),
{
    let reservation = &reservations[index];
    let ghost tail = reservations@.subrange(index as int + 1, reservations@.len() as int);
    let ghost full = reservations@.subrange(index as int, reservations@.len() as int);
    assert(full.len() > 0);
    assert(full.first() == *reservation);
    assert(full.drop_first() =~= tail);
    assert(full.drop_first() == tail);
    if !reservation.worker_id().same(&worker) {
        proof {
            assert(reservation.spec_worker_id() != worker);
            assert forall |kind: ResourceKind| #![auto]
                crate::verified::worker_quantity(
                    full, worker, kind)
                    == crate::verified::worker_quantity(tail, worker, kind) by {
                reveal(crate::verified::worker_quantity);
            }
            assert(worker_sound(&aggregate, full, worker));
        }
        return aggregate;
    }
    assert(reservation.spec_worker_id() == worker);
    match aggregate {
        Aggregate::Empty => {
            let vector = reservation.resources().clone();
            proof {
                assert forall |kind: ResourceKind| #![auto]
                    vector.spec_quantity(kind)
                        == crate::verified::worker_quantity(
                            full, worker, kind) by {
                    assert(crate::verified::worker_quantity(tail, worker, kind) == 0);
                    vector.quantity_matches_entries(kind);
                    reservation.spec_resources().quantity_matches_entries(kind);
                    assert(vector.spec_entries()
                        == reservation.spec_resources().spec_entries());
                    assert(vector.spec_quantity(kind)
                        == reservation.spec_resources().spec_quantity(kind));
                    reveal(crate::verified::vector_quantity);
                    reveal(crate::verified::worker_quantity);
                }
            }
            Aggregate::Value(vector)
        }
        Aggregate::Value(current) => {
            let Some(vector) = current.aggregate_add(reservation.resources()) else {
                return Aggregate::Failed;
            };
            proof {
                assert forall |kind: ResourceKind| #![auto]
                    vector.spec_quantity(kind)
                        == crate::verified::worker_quantity(
                            full, worker, kind) by {
                    assert(current.spec_quantity(kind)
                        == crate::verified::worker_quantity(tail, worker, kind));
                    assert(vector.spec_quantity(kind)
                        == current.spec_quantity(kind)
                            + reservation.spec_resources().spec_quantity(kind));
                    reservation.spec_resources().quantity_matches_entries(kind);
                    reveal(crate::verified::vector_quantity);
                    reveal(crate::verified::worker_quantity);
                }
            }
            Aggregate::Value(vector)
        }
        Aggregate::Failed => Aggregate::Failed,
    }
}

pub(super) fn global(
    reservations: &[SchedulerReservation],
) -> (result: Aggregate)
    ensures global_sound(&result, reservations@),
{
    let mut result = Aggregate::Empty;
    let mut index = reservations.len();
    while index > 0
        invariant
            index <= reservations@.len(),
            global_sound(
                &result,
                reservations@.subrange(index as int, reservations@.len() as int),
            ),
        decreases index,
    {
        index -= 1;
        result = prepend_global(result, reservations, index);
    }
    assert(reservations@.subrange(0, reservations@.len() as int) =~= reservations@);
    result
}

pub(super) fn worker(
    reservations: &[SchedulerReservation],
    worker_id: WorkerId,
) -> (result: Aggregate)
    ensures worker_sound(&result, reservations@, worker_id),
{
    let mut result = Aggregate::Empty;
    let mut index = reservations.len();
    while index > 0
        invariant
            index <= reservations@.len(),
            worker_sound(
                &result,
                reservations@.subrange(index as int, reservations@.len() as int),
                worker_id,
            ),
        decreases index,
    {
        index -= 1;
        result = prepend_worker(result, reservations, index, worker_id);
    }
    assert(reservations@.subrange(0, reservations@.len() as int) =~= reservations@);
    result
}

} // verus!
