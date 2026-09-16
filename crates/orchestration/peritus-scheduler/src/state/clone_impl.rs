//! Verified semantic clone for the complete scheduler aggregate.

use super::SchedulerState;
#[cfg(verus_only)]
use crate::SchedulerBinding;
use crate::{SchedulerReservation, SchedulerTerminal, WorkRecord, WorkerRecord};
use vstd::prelude::*;

verus! {

impl SchedulerState {
    /// Every authoritative collection and cursor remains constrained, including empty states.
    pub(crate) proof fn clone_preserves_authoritative_fields(left: &Self, right: &Self)
        requires Self::clone_equivalent(left, right),
        ensures
            SchedulerBinding::clone_equivalent(left.spec_binding(), right.spec_binding()),
            left.spec_phase() == right.spec_phase(),
            left.spec_sequence() == right.spec_sequence(),
            left.spec_last_event_id() == right.spec_last_event_id(),
            left.spec_state_digest() == right.spec_state_digest(),
            left.spec_workers().len() == right.spec_workers().len(),
            left.spec_work().len() == right.spec_work().len(),
            left.spec_reservations().len() == right.spec_reservations().len(),
            left.spec_used_dispatches() == right.spec_used_dispatches(),
            left.spec_enqueue_ordinal() == right.spec_enqueue_ordinal(),
            left.spec_dispatch_ordinal() == right.spec_dispatch_ordinal(),
            left.spec_used_commands() == right.spec_used_commands(),
            terminal_clone_equivalent(&left.spec_terminal(), &right.spec_terminal()),
    {
        reveal(SchedulerState::clone_equivalent);
    }

    /// Semantic state cloning preserves the complete identity sequence of every collection.
    pub(crate) proof fn clone_preserves_collection_order(left: &Self, right: &Self)
        requires
            Self::reservation_clone_equivalent(left, right),
            left.spec_collections_ordered(),
        ensures right.spec_collections_ordered(),
    {
        reveal(SchedulerState::reservation_clone_equivalent);
        assert forall |index: int| 0 <= index < left.spec_workers().len() implies
            left.spec_workers()[index].spec_descriptor().spec_id()
                == right.spec_workers()[index].spec_descriptor().spec_id() by {
            WorkerRecord::clone_reservation_fields(
                &left.spec_workers()[index], &right.spec_workers()[index],
            );
        }
        assert forall |index: int| 0 <= index < left.spec_work().len() implies
            left.spec_work()[index].spec_definition().spec_id()
                == right.spec_work()[index].spec_definition().spec_id() by {
            WorkRecord::clone_reservation_fields(
                &left.spec_work()[index], &right.spec_work()[index],
            );
        }
        assert forall |index: int| 0 <= index < left.spec_reservations().len() implies
            left.spec_reservations()[index].spec_dispatch_id()
                == right.spec_reservations()[index].spec_dispatch_id() by {
            SchedulerReservation::clone_fields(
                &left.spec_reservations()[index], &right.spec_reservations()[index],
            );
        }
    }

    /// Relates a state clone to every authoritative source field.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        SchedulerBinding::clone_equivalent(&left.binding, &right.binding)
            && left.phase == right.phase
            && left.sequence == right.sequence
            && left.last_event_id == right.last_event_id
            && left.state_digest == right.state_digest
            && left.workers@.len() == right.workers@.len()
            && (forall |index: int| #![auto]
                0 <= index < left.workers@.len()
                    ==> WorkerRecord::clone_equivalent(
                        &left.workers@[index],
                        &right.workers@[index],
                    ))
            && left.work@.len() == right.work@.len()
            && (forall |index: int| #![auto]
                0 <= index < left.work@.len()
                    ==> WorkRecord::clone_equivalent(
                        &left.work@[index],
                        &right.work@[index],
                    ))
            && left.reservations@.len() == right.reservations@.len()
            && (forall |index: int| #![auto]
                0 <= index < left.reservations@.len()
                    ==> SchedulerReservation::clone_equivalent(
                        &left.reservations@[index],
                        &right.reservations@[index],
                    ))
            && left.used_dispatches@ == right.used_dispatches@
            && left.enqueue_ordinal == right.enqueue_ordinal
            && left.dispatch_ordinal == right.dispatch_ordinal
            && left.used_commands@ == right.used_commands@
            && terminal_clone_equivalent(&left.terminal, &right.terminal)
    }

    /// Relates a state clone to every reservation-relevant source field.
    pub closed spec fn reservation_clone_equivalent(left: &Self, right: &Self) -> bool {
        left.binding.spec_semantics() == right.binding.spec_semantics()
            && left.binding.spec_limits() == right.binding.spec_limits()
            && left.binding.spec_capacity().spec_entries()
                == right.binding.spec_capacity().spec_entries()
            && left.workers@.len() == right.workers@.len()
            && (forall |index: int| #![auto]
                0 <= index < left.workers@.len()
                    ==> WorkerRecord::clone_equivalent(
                        &left.workers@[index],
                        &right.workers@[index],
                    ))
            && left.work@.len() == right.work@.len()
            && (forall |index: int| #![auto]
                0 <= index < left.work@.len()
                    ==> WorkRecord::clone_equivalent(
                        &left.work@[index],
                        &right.work@[index],
                    ))
            && left.reservations@.len() == right.reservations@.len()
            && (forall |index: int| #![auto]
                0 <= index < left.reservations@.len()
                    ==> SchedulerReservation::clone_equivalent(
                        &left.reservations@[index],
                        &right.reservations@[index],
                    ))
            && left.used_dispatches@ == right.used_dispatches@
    }
}

fn clone_worker_records(values: &[WorkerRecord]) -> (result: Vec<WorkerRecord>)
    ensures
        result@.len() == values@.len(),
        forall |index: int| #![auto]
            0 <= index < values@.len()
                ==> WorkerRecord::clone_equivalent(&values@[index], &result@[index]),
{
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values.len(),
            result@.len() == index,
            forall |prior: int| #![auto]
                0 <= prior < index
                    ==> WorkerRecord::clone_equivalent(&values@[prior], &result@[prior]),
        decreases values.len() - index,
    {
        result.push(values[index].clone());
        index += 1;
    }
    result
}

fn clone_work_records(values: &[WorkRecord]) -> (result: Vec<WorkRecord>)
    ensures
        result@.len() == values@.len(),
        forall |index: int| #![auto]
            0 <= index < values@.len()
                ==> WorkRecord::clone_equivalent(&values@[index], &result@[index]),
{
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values.len(),
            result@.len() == index,
            forall |prior: int| #![auto]
                0 <= prior < index
                    ==> WorkRecord::clone_equivalent(&values@[prior], &result@[prior]),
        decreases values.len() - index,
    {
        result.push(values[index].clone());
        index += 1;
    }
    result
}

fn clone_reservations(
    values: &[SchedulerReservation],
) -> (result: Vec<SchedulerReservation>)
    ensures
        result@.len() == values@.len(),
        forall |index: int| #![auto]
            0 <= index < values@.len()
                ==> SchedulerReservation::clone_equivalent(&values@[index], &result@[index]),
{
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values.len(),
            result@.len() == index,
            forall |prior: int| #![auto]
                0 <= prior < index
                    ==> SchedulerReservation::clone_equivalent(
                        &values@[prior],
                        &result@[prior],
                    ),
        decreases values.len() - index,
    {
        result.push(values[index].clone());
        index += 1;
    }
    result
}

fn clone_dispatch_ids(values: &[crate::DispatchId]) -> (result: Vec<crate::DispatchId>)
    ensures result@ == values@,
{
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values@.len(),
            result@ == values@.take(index as int),
        decreases values@.len() - index,
    {
        result.push(values[index]);
        index += 1;
    }
    result
}

fn clone_command_ids(
    values: &[peritus_types::CommandId],
) -> (result: Vec<peritus_types::CommandId>)
    ensures result@ == values@,
{
    let mut result = Vec::with_capacity(values.len());
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values@.len(),
            result@ == values@.take(index as int),
        decreases values@.len() - index,
    {
        result.push(values[index]);
        index += 1;
    }
    result
}

pub closed spec fn terminal_clone_equivalent(
    left: &Option<SchedulerTerminal>,
    right: &Option<SchedulerTerminal>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => SchedulerTerminal::clone_equivalent(left, right),
        (None, None) => true,
        _ => false,
    }
}

closed spec fn terminal_ref_clone_equivalent(
    left: Option<&SchedulerTerminal>,
    right: &Option<SchedulerTerminal>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => SchedulerTerminal::clone_equivalent(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn clone_terminal(value: Option<&SchedulerTerminal>) -> (result: Option<SchedulerTerminal>)
    ensures terminal_ref_clone_equivalent(value, &result),
{
    let terminal = value?;
    Some(terminal.clone())
}

impl Clone for SchedulerState {
    fn clone(&self) -> (result: Self)
        ensures
            Self::clone_equivalent(self, &result),
            Self::reservation_clone_equivalent(self, &result),
            self.spec_collections_ordered() ==> result.spec_collections_ordered(),
            self.spec_reservation_reducer_ready()
                ==> result.spec_reservation_reducer_ready(),
    {
        let result = Self {
            binding: self.binding.clone(),
            phase: self.phase,
            sequence: self.sequence,
            last_event_id: self.last_event_id,
            state_digest: self.state_digest,
            workers: clone_worker_records(&self.workers),
            work: clone_work_records(&self.work),
            reservations: clone_reservations(&self.reservations),
            used_dispatches: clone_dispatch_ids(&self.used_dispatches),
            enqueue_ordinal: self.enqueue_ordinal,
            dispatch_ordinal: self.dispatch_ordinal,
            used_commands: clone_command_ids(&self.used_commands),
            terminal: clone_terminal(self.terminal.as_ref()),
        };
        proof {
            reveal(terminal_ref_clone_equivalent);
            reveal(terminal_clone_equivalent);
            reveal(SchedulerState::clone_equivalent);
            reveal(SchedulerState::reservation_clone_equivalent);
            SchedulerBinding::clone_preserves_reservation_fields(
                &self.binding,
                &result.binding,
            );
            Self::clone_preserves_authoritative_fields(self, &result);
            if self.spec_collections_ordered() {
                Self::clone_preserves_collection_order(self, &result);
            }
            if self.spec_reservation_reducer_ready() {
                assert forall |index: int| #![auto]
                    0 <= index < self.spec_workers().len() implies
                        WorkerRecord::reservation_owner_equivalent(
                            &self.spec_workers()[index],
                            &result.spec_workers()[index],
                        ) by {
                    WorkerRecord::clone_reservation_fields(
                        &self.spec_workers()[index],
                        &result.spec_workers()[index],
                    );
                }
                assert forall |index: int| #![auto]
                    0 <= index < self.spec_work().len() implies
                        WorkRecord::reservation_lifecycle_equivalent(
                            &self.spec_work()[index],
                            &result.spec_work()[index],
                        ) by {
                    WorkRecord::clone_reservation_fields(
                        &self.spec_work()[index],
                        &result.spec_work()[index],
                    );
                }
                assert forall |index: int| #![auto]
                    0 <= index < self.spec_reservations().len() implies
                        SchedulerReservation::invariant_equivalent(
                            &self.spec_reservations()[index],
                            &result.spec_reservations()[index],
                        ) by {
                    SchedulerReservation::clone_fields(
                        &self.spec_reservations()[index],
                        &result.spec_reservations()[index],
                    );
                }
                crate::verified::equivalent_state_preserves(
                    self.spec_binding(),
                    result.spec_binding(),
                    self.spec_workers(),
                    result.spec_workers(),
                    self.spec_work(),
                    result.spec_work(),
                    self.spec_reservations(),
                    result.spec_reservations(),
                    self.spec_used_dispatches(),
                    result.spec_used_dispatches(),
                );
                crate::verified::equivalent_sequences_preserve_phase(
                    self.spec_work(),
                    result.spec_work(),
                    self.spec_reservations(),
                    result.spec_reservations(),
                );
                assert(result.spec_reservation_reducer_ready());
            }
        }
        result
    }
}

} // verus!
