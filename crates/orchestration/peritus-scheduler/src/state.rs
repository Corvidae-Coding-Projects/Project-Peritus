//! Complete authoritative scheduler state.

mod clone_impl;
mod lookup;
pub mod mutation;
mod ordering;
pub mod queue;
mod terminal;
mod validation;

pub use terminal::{SchedulerTerminal, SchedulerTerminalKind};

use peritus_types::{CommandId, EventId, EventSequence, RunId, Sha256Digest};
use vstd::prelude::*;

use crate::{
    DispatchId, ResourceVector, SchedulerBinding, SchedulerError, SchedulerReservation, WorkPhase,
    WorkRecord, WorkerRecord,
};

verus! {

/// Closed scheduler lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SchedulerPhase {
    /// Admission and dispatch are enabled.
    Active,
    /// Dispatch is paused while active ownership is preserved.
    Paused,
    /// New admission is closed while retained queued work may drain.
    Draining,
    /// Admission is closed and retained dispatch is temporarily paused.
    DrainingPaused,
    /// Truthful immutable terminal was committed.
    Terminal,
}

} // verus!

verus! {

/// Exact mathematical projection of the reserved all-zero digest value.
pub(crate) open spec fn digest_is_zero(digest: Sha256Digest) -> bool {
    forall |index: int| 0 <= index < 32 ==> digest.spec_bytes()[index] == 0
}

/// Complete deterministic replayable scheduler aggregate.
#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerState {
    binding: SchedulerBinding,
    phase: SchedulerPhase,
    sequence: EventSequence,
    last_event_id: EventId,
    state_digest: Sha256Digest,
    workers: Vec<WorkerRecord>,
    work: Vec<WorkRecord>,
    reservations: Vec<SchedulerReservation>,
    used_dispatches: Vec<DispatchId>,
    enqueue_ordinal: u64,
    dispatch_ordinal: u64,
    used_commands: Vec<CommandId>,
    terminal: Option<SchedulerTerminal>,
}

impl SchedulerState {
    /// Returns the mathematical immutable scheduler binding.
    pub closed spec fn spec_binding(&self) -> &SchedulerBinding { &self.binding }
    /// Returns the mathematical scheduler lifecycle.
    pub closed spec fn spec_phase(&self) -> SchedulerPhase { self.phase }
    /// Returns the mathematical retained worker sequence.
    pub closed spec fn spec_workers(&self) -> Seq<WorkerRecord> { self.workers@ }
    /// Returns the mathematical retained work sequence.
    pub closed spec fn spec_work(&self) -> Seq<WorkRecord> { self.work@ }
    /// Returns the mathematical live-reservation sequence.
    pub closed spec fn spec_reservations(&self) -> Seq<SchedulerReservation> {
        self.reservations@
    }
    /// Returns the mathematical retained dispatch-identity history.
    pub closed spec fn spec_used_dispatches(&self) -> Seq<DispatchId> {
        self.used_dispatches@
    }
    /// Returns the mathematical count of durable dispatches created.
    pub closed spec fn spec_dispatch_ordinal(&self) -> u64 {
        self.dispatch_ordinal
    }
    /// Returns the exact event sequence fence.
    pub closed spec fn spec_sequence(&self) -> EventSequence { self.sequence }
    /// Returns the exact predecessor event fence.
    pub closed spec fn spec_last_event_id(&self) -> EventId { self.last_event_id }
    /// Returns the exact retained state digest.
    pub closed spec fn spec_state_digest(&self) -> Sha256Digest { self.state_digest }
    /// Returns the exact admission ordinal.
    pub closed spec fn spec_enqueue_ordinal(&self) -> u64 { self.enqueue_ordinal }
    /// Returns command identities in durable event order.
    pub closed spec fn spec_used_commands(&self) -> Seq<CommandId> { self.used_commands@ }
    /// Returns the retained terminal evidence.
    pub closed spec fn spec_terminal(&self) -> Option<SchedulerTerminal> { self.terminal }

    /// Borrows immutable scheduler binding.
    #[must_use]
    pub const fn binding(&self) -> (result: &SchedulerBinding)
        ensures result == self.spec_binding(),
    {
        &self.binding
    }

    /// Returns lifecycle.
    #[must_use]
    pub const fn phase(&self) -> (result: SchedulerPhase)
        ensures result == self.spec_phase(),
    {
        self.phase
    }

    /// Borrows workers in canonical identity order.
    #[must_use]
    pub fn workers(&self) -> (result: &[WorkerRecord])
        ensures result@ == self.spec_workers(),
    {
        &self.workers
    }

    /// Borrows work in canonical identity order.
    #[must_use]
    pub fn work(&self) -> (result: &[WorkRecord])
        ensures result@ == self.spec_work(),
    {
        &self.work
    }

    /// Borrows live reservations in dispatch-identity order.
    #[must_use]
    pub fn reservations(&self) -> (result: &[SchedulerReservation])
        ensures result@ == self.spec_reservations(),
    {
        &self.reservations
    }

    /// Borrows every historical dispatch identity in canonical order.
    #[must_use]
    pub fn used_dispatches(&self) -> (result: &[DispatchId])
        ensures result@ == self.spec_used_dispatches(),
    {
        &self.used_dispatches
    }

    /// Returns number of durable reservations created.
    #[must_use]
    pub const fn dispatch_ordinal(&self) -> (result: u64)
        ensures result == self.spec_dispatch_ordinal(),
    {
        self.dispatch_ordinal
    }

    pub(crate) fn genesis(
        binding: SchedulerBinding,
        event_id: EventId,
        command_id: CommandId,
    ) -> (result: Self)
        ensures
            result.spec_reservation_reducer_ready(),
            result.spec_collections_ordered(),
            *result.spec_binding() == binding,
            result.spec_phase() == SchedulerPhase::Active,
            result.spec_workers() == Seq::<WorkerRecord>::empty(),
            result.spec_work() == Seq::<WorkRecord>::empty(),
            result.spec_reservations() == Seq::<SchedulerReservation>::empty(),
            result.spec_used_dispatches() == Seq::<DispatchId>::empty(),
            result.spec_sequence().spec_value() == 1,
            result.spec_last_event_id() == event_id,
            digest_is_zero(result.spec_state_digest()),
            result.spec_enqueue_ordinal() == 0,
            result.spec_dispatch_ordinal() == 0,
            result.spec_used_commands() == Seq::<CommandId>::empty().push(command_id),
            result.spec_terminal().is_none(),
    {
        let used_commands = vec![command_id];
        let result = Self {
            binding,
            phase: SchedulerPhase::Active,
            sequence: EventSequence::first(),
            last_event_id: event_id,
            state_digest: Sha256Digest::new([0; 32]),
            workers: Vec::new(),
            work: Vec::new(),
            reservations: Vec::new(),
            used_dispatches: Vec::new(),
            enqueue_ordinal: 0,
            dispatch_ordinal: 0,
            used_commands,
            terminal: None,
        };
        proof {
            reveal(SchedulerState::spec_reservation_reducer_ready);
            reveal(crate::verified::reservation_invariant_parts);
            reveal(crate::verified::reservations_bind_active_work);
            reveal(crate::verified::reservations_are_retained_dispatches);
            reveal(crate::verified::reservations_have_work);
            reveal(crate::verified::reservations_have_workers);
            reveal(crate::verified::reservation_identities_unique);
            reveal(crate::verified::worker_identities_unique);
            reveal(crate::verified::work_identities_unique);
            assert forall |kind: crate::ResourceKind|
                #![trigger crate::verified::reservation_quantity(
                    result.spec_reservations(), kind,
                )]
                0 <= crate::verified::reservation_quantity(result.spec_reservations(), kind)
                    <= crate::verified::vector_quantity(
                        result.spec_binding().spec_capacity().spec_entries(),
                        kind,
                    ) by {
                crate::verified::vector_quantity_nonnegative(
                    result.spec_binding().spec_capacity().spec_entries(),
                    kind,
                );
            }
            assert(result.spec_reservation_reducer_ready());
            assert forall |index: int| 0 <= index < 32 implies
                result.spec_state_digest().spec_bytes()[index] == 0 by {
            }
        }
        result
    }
}

} // verus!

verus! {

impl SchedulerState {
    /// Returns bound run.
    #[must_use]
    pub const fn run_id(&self) -> (result: RunId)
        ensures result == self.spec_binding().spec_run_id(),
    {
        self.binding.run_id()
    }
    /// Returns current one-based event sequence.
    #[must_use]
    pub const fn sequence(&self) -> (result: EventSequence)
        ensures result == self.spec_sequence(),
    {
        self.sequence
    }
    /// Returns latest event identity.
    #[must_use]
    pub const fn last_event_id(&self) -> (result: EventId)
        ensures result == self.spec_last_event_id(),
    {
        self.last_event_id
    }
    /// Returns canonical complete-state digest.
    #[must_use]
    pub const fn state_digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_state_digest(),
    {
        self.state_digest
    }
    /// Returns last assigned enqueue ordinal.
    #[must_use]
    pub const fn enqueue_ordinal(&self) -> (result: u64)
        ensures result == self.spec_enqueue_ordinal(),
    {
        self.enqueue_ordinal
    }
    /// Borrows used command identities in event order.
    #[must_use]
    pub fn used_commands(&self) -> (result: &[CommandId])
        ensures result@ == self.spec_used_commands(),
    {
        &self.used_commands
    }
    /// Borrows immutable terminal summary.
    #[must_use]
    pub const fn terminal(&self) -> (result: Option<&SchedulerTerminal>)
        ensures match result {
            Some(value) => self.spec_terminal() == Some(*value),
            None => self.spec_terminal().is_none(),
        },
    {
        self.terminal.as_ref()
    }
}

} // verus!

impl SchedulerState {
    /// Returns used global resources, with `None` representing exact zero.
    ///
    /// # Errors
    /// Rejects incompatible resource dimensions or quantity overflow.
    pub fn used_resources(&self) -> Result<Option<ResourceVector>, SchedulerError> {
        let maximum = self.binding.limits().resource_dimensions();
        self.reservations.iter().try_fold(None::<ResourceVector>, |sum, reservation| {
            sum.map_or_else(
                || Ok(Some(reservation.resources().clone())),
                |current| current.checked_add(reservation.resources(), maximum).map(Some),
            )
        })
    }
    /// Returns whether every admitted work item is terminal.
    #[must_use]
    pub fn all_work_terminal(&self) -> bool {
        self.work.iter().all(|record| record.phase() == WorkPhase::Terminal)
    }
    /// Returns conservative upper bound used before canonical storage admission.
    #[must_use]
    pub fn estimated_encoded_bytes(&self) -> u64 {
        1_024_u64
            .saturating_add((self.workers.len() as u64).saturating_mul(512))
            .saturating_add(self.work.iter().fold(0_u64, |total, record| {
                total
                    .saturating_add(512)
                    .saturating_add((record.spec().dependencies().len() as u64).saturating_mul(16))
                    .saturating_add(
                        (record.spec().request().entries().len() as u64).saturating_mul(16),
                    )
            }))
            .saturating_add((self.reservations.len() as u64).saturating_mul(384))
            .saturating_add((self.used_dispatches.len() as u64).saturating_mul(16))
            .saturating_add((self.used_commands.len() as u64).saturating_mul(16))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_wire(
        binding: SchedulerBinding,
        phase: SchedulerPhase,
        sequence: EventSequence,
        last_event_id: EventId,
        state_digest: Sha256Digest,
        workers: Vec<WorkerRecord>,
        work: Vec<WorkRecord>,
        reservations: Vec<SchedulerReservation>,
        used_dispatches: Vec<DispatchId>,
        enqueue_ordinal: u64,
        dispatch_ordinal: u64,
        used_commands: Vec<CommandId>,
        terminal: Option<SchedulerTerminal>,
    ) -> Self {
        Self {
            binding,
            phase,
            sequence,
            last_event_id,
            state_digest,
            workers,
            work,
            reservations,
            used_dispatches,
            enqueue_ordinal,
            dispatch_ordinal,
            used_commands,
            terminal,
        }
    }
}
