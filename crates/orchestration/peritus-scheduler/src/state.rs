//! Complete authoritative scheduler state.

pub mod mutation;
mod terminal;
mod validation;

pub use terminal::{SchedulerTerminal, SchedulerTerminalKind};

use peritus_types::{CommandId, EventId, EventSequence, RunId, Sha256Digest};

use crate::{
    DispatchId, ResourceVector, SchedulerBinding, SchedulerError, SchedulerReservation, WorkId,
    WorkPhase, WorkRecord, WorkerId, WorkerRecord,
};

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

/// Complete deterministic replayable scheduler aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    pub(crate) fn genesis(
        binding: SchedulerBinding,
        event_id: EventId,
        command_id: CommandId,
    ) -> Self {
        Self {
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
            used_commands: vec![command_id],
            terminal: None,
        }
    }
    /// Returns bound run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.binding.run_id()
    }
    /// Borrows immutable scheduler binding.
    #[must_use]
    pub const fn binding(&self) -> &SchedulerBinding {
        &self.binding
    }
    /// Returns lifecycle.
    #[must_use]
    pub const fn phase(&self) -> SchedulerPhase {
        self.phase
    }
    /// Returns current one-based event sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    /// Returns latest event identity.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    /// Returns canonical complete-state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Borrows workers in canonical identity order.
    #[must_use]
    pub fn workers(&self) -> &[WorkerRecord] {
        &self.workers
    }
    /// Borrows work in canonical identity order.
    #[must_use]
    pub fn work(&self) -> &[WorkRecord] {
        &self.work
    }
    /// Borrows live reservations in dispatch-identity order.
    #[must_use]
    pub fn reservations(&self) -> &[SchedulerReservation] {
        &self.reservations
    }
    /// Borrows every historical dispatch identity in canonical order.
    #[must_use]
    pub fn used_dispatches(&self) -> &[DispatchId] {
        &self.used_dispatches
    }
    /// Returns last assigned enqueue ordinal.
    #[must_use]
    pub const fn enqueue_ordinal(&self) -> u64 {
        self.enqueue_ordinal
    }
    /// Returns number of durable reservations created.
    #[must_use]
    pub const fn dispatch_ordinal(&self) -> u64 {
        self.dispatch_ordinal
    }
    /// Borrows used command identities in event order.
    #[must_use]
    pub fn used_commands(&self) -> &[CommandId] {
        &self.used_commands
    }
    /// Borrows immutable terminal summary.
    #[must_use]
    pub const fn terminal(&self) -> Option<&SchedulerTerminal> {
        self.terminal.as_ref()
    }

    /// Looks up a worker.
    #[must_use]
    pub fn worker(&self, id: WorkerId) -> Option<&WorkerRecord> {
        self.workers
            .binary_search_by_key(&id, |record| record.descriptor().id())
            .ok()
            .map(|index| &self.workers[index])
    }
    /// Looks up work.
    #[must_use]
    pub fn work_item(&self, id: WorkId) -> Option<&WorkRecord> {
        self.work
            .binary_search_by_key(&id, |record| record.spec().id())
            .ok()
            .map(|index| &self.work[index])
    }
    /// Looks up an active dispatch.
    #[must_use]
    pub fn reservation(&self, id: DispatchId) -> Option<&SchedulerReservation> {
        self.reservations
            .binary_search_by_key(&id, SchedulerReservation::dispatch_id)
            .ok()
            .map(|index| &self.reservations[index])
    }
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
