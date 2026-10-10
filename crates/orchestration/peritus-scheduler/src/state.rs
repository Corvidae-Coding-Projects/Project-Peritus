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

use crate::{
    DispatchId, ResourceVector, SchedulerBinding, SchedulerError, SchedulerReservation, WorkPhase,
    WorkRecord, WorkerRecord,
};

#[doc = " Closed scheduler lifecycle."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SchedulerPhase {
    #[doc = " Admission and dispatch are enabled."]
    Active,
    #[doc = " Dispatch is paused while active ownership is preserved."]
    Paused,
    #[doc = " New admission is closed while retained queued work may drain."]
    Draining,
    #[doc = " Admission is closed and retained dispatch is temporarily paused."]
    DrainingPaused,
    #[doc = " Truthful immutable terminal was committed."]
    Terminal,
}

#[doc = " Complete deterministic replayable scheduler aggregate."]
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
    #[doc = " Borrows immutable scheduler binding."]
    #[must_use]
    pub const fn binding(&self) -> &SchedulerBinding {
        &self.binding
    }
    #[doc = " Returns lifecycle."]
    #[must_use]
    pub const fn phase(&self) -> SchedulerPhase {
        self.phase
    }
    #[doc = " Borrows workers in canonical identity order."]
    #[must_use]
    pub fn workers(&self) -> &[WorkerRecord] {
        &self.workers
    }
    #[doc = " Borrows work in canonical identity order."]
    #[must_use]
    pub fn work(&self) -> &[WorkRecord] {
        &self.work
    }
    #[doc = " Borrows live reservations in dispatch-identity order."]
    #[must_use]
    pub fn reservations(&self) -> &[SchedulerReservation] {
        &self.reservations
    }
    #[doc = " Borrows every historical dispatch identity in canonical order."]
    #[must_use]
    pub fn used_dispatches(&self) -> &[DispatchId] {
        &self.used_dispatches
    }
    #[doc = " Returns number of durable reservations created."]
    #[must_use]
    pub const fn dispatch_ordinal(&self) -> u64 {
        self.dispatch_ordinal
    }
    pub(crate) fn genesis(
        binding: SchedulerBinding,
        event_id: EventId,
        command_id: CommandId,
    ) -> Self {
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
        result
    }
}

impl SchedulerState {
    #[doc = " Returns bound run."]
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.binding.run_id()
    }
    #[doc = " Returns current one-based event sequence."]
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    #[doc = " Returns latest event identity."]
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    #[doc = " Returns canonical complete-state digest."]
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    #[doc = " Returns last assigned enqueue ordinal."]
    #[must_use]
    pub const fn enqueue_ordinal(&self) -> u64 {
        self.enqueue_ordinal
    }
    #[doc = " Borrows used command identities in event order."]
    #[must_use]
    pub fn used_commands(&self) -> &[CommandId] {
        &self.used_commands
    }
    #[doc = " Borrows immutable terminal summary."]
    #[must_use]
    pub const fn terminal(&self) -> Option<&SchedulerTerminal> {
        self.terminal.as_ref()
    }
}

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
