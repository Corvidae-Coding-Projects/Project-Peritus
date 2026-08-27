//! Typed lifecycle, count, remaining-work, reap, and shutdown snapshots.

use super::{WorkerAssignment, WorkerCancellationReason, WorkerTerminalObservation};

/// Worker-supervisor admission and shutdown lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WorkerSupervisorPhase {
    /// New scheduler dispatches may be accepted.
    Accepting,
    /// New dispatches are rejected while existing work may finish.
    Draining,
    /// Cancellation and bounded joining are in progress.
    ShuttingDown,
    /// Aborted tasks remain unjoined and still belong to the supervisor.
    ShutdownIncomplete,
    /// Every owned task has joined and no new dispatch may start.
    Stopped,
}

/// Result of idempotently closing new-work admission.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WorkerDrainDisposition {
    /// This call moved the supervisor from accepting to draining.
    Began,
    /// The supervisor was already non-accepting.
    AlreadyNonAccepting(WorkerSupervisorPhase),
}

/// Result of one targeted cancellation request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WorkerCancelDisposition {
    /// The first cancellation reason was delivered.
    Requested,
    /// The same dispatch already retained a first cancellation reason.
    AlreadyRequested(WorkerCancellationReason),
    /// The task completed before cancellation could be delivered.
    AlreadyFinished,
}

/// Observable state of one still-owned task.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WorkerTaskState {
    /// The task is active without a cancellation request.
    Running,
    /// Cooperative cancellation was requested.
    CancellationRequested(WorkerCancellationReason),
    /// Tokio reports completion, but the join result has not been reaped.
    CompletedAwaitingReap,
    /// Bounded shutdown issued an abort, but the join is still outstanding.
    AbortRequested,
}

/// Exact snapshot of one active or completed-unreaped task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkerTaskSnapshot {
    assignment: WorkerAssignment,
    state: WorkerTaskState,
}

impl WorkerTaskSnapshot {
    pub(super) const fn new(assignment: WorkerAssignment, state: WorkerTaskState) -> Self {
        Self { assignment, state }
    }

    /// Returns the exact scheduler assignment.
    #[must_use]
    pub(crate) const fn assignment(self) -> WorkerAssignment {
        self.assignment
    }
    /// Returns the observed process-local task state.
    #[must_use]
    pub(crate) const fn state(self) -> WorkerTaskState {
        self.state
    }
}

/// One bounded item that prevents the supervisor snapshot from being quiescent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkerRemainingWork {
    /// A Tokio task is still owned or awaiting join.
    Task(WorkerTaskSnapshot),
    /// A joined terminal observation still awaits authoritative settlement.
    Observation(WorkerTerminalObservation),
}

/// Exact bounded worker counts derived from owned registries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkerCounts {
    active_tasks: usize,
    running_tasks: usize,
    cancellation_requested: usize,
    completed_awaiting_reap: usize,
    abort_requested: usize,
    pending_observations: usize,
}

impl WorkerCounts {
    pub(super) const fn new(
        active_tasks: usize,
        running_tasks: usize,
        cancellation_requested: usize,
        completed_awaiting_reap: usize,
        abort_requested: usize,
        pending_observations: usize,
    ) -> Self {
        Self {
            active_tasks,
            running_tasks,
            cancellation_requested,
            completed_awaiting_reap,
            abort_requested,
            pending_observations,
        }
    }

    /// Returns all Tokio tasks whose join handles remain owned.
    #[must_use]
    pub(crate) const fn active_tasks(self) -> usize {
        self.active_tasks
    }
    /// Returns active tasks without cancellation, completion, or abort observations.
    #[must_use]
    pub(crate) const fn running_tasks(self) -> usize {
        self.running_tasks
    }
    /// Returns active tasks with cooperative cancellation requested.
    #[must_use]
    pub(crate) const fn cancellation_requested(self) -> usize {
        self.cancellation_requested
    }
    /// Returns completed tasks whose join values still require reaping.
    #[must_use]
    pub(crate) const fn completed_awaiting_reap(self) -> usize {
        self.completed_awaiting_reap
    }
    /// Returns tasks aborted during shutdown whose joins remain outstanding.
    #[must_use]
    pub(crate) const fn abort_requested(self) -> usize {
        self.abort_requested
    }
    /// Returns terminal observations waiting in the bounded result queue.
    #[must_use]
    pub(crate) const fn pending_observations(self) -> usize {
        self.pending_observations
    }
}

/// Complete bounded point-in-time worker-supervisor snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkerSupervisorSnapshot {
    phase: WorkerSupervisorPhase,
    counts: WorkerCounts,
    remaining: Vec<WorkerRemainingWork>,
}

impl WorkerSupervisorSnapshot {
    pub(super) const fn new(
        phase: WorkerSupervisorPhase,
        counts: WorkerCounts,
        remaining: Vec<WorkerRemainingWork>,
    ) -> Self {
        Self { phase, counts, remaining }
    }

    /// Returns the admission/shutdown lifecycle.
    #[must_use]
    pub(crate) const fn phase(&self) -> WorkerSupervisorPhase {
        self.phase
    }
    /// Returns exact derived counts.
    #[must_use]
    pub(crate) const fn counts(&self) -> WorkerCounts {
        self.counts
    }
    /// Borrows the complete bounded remaining-work inventory.
    #[must_use]
    pub(crate) fn remaining(&self) -> &[WorkerRemainingWork] {
        &self.remaining
    }
}

/// Result of one bounded completed-task reap pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkerReapReport {
    reaped: usize,
    active_remaining: usize,
    result_capacity_blocked: bool,
}

impl WorkerReapReport {
    pub(super) const fn new(
        reaped: usize,
        active_remaining: usize,
        result_capacity_blocked: bool,
    ) -> Self {
        Self { reaped, active_remaining, result_capacity_blocked }
    }
    /// Returns joined task count.
    #[must_use]
    pub(crate) const fn reaped(self) -> usize {
        self.reaped
    }
    /// Returns join handles still owned.
    #[must_use]
    pub(crate) const fn active_remaining(self) -> usize {
        self.active_remaining
    }
    /// Returns whether completed work could not be joined because the result queue was full.
    #[must_use]
    pub(crate) const fn result_capacity_blocked(self) -> bool {
        self.result_capacity_blocked
    }
}

/// Truthful worker shutdown classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WorkerShutdownDisposition {
    /// No task, abort, or terminal observation remains for external settlement.
    Clean,
    /// Returned observations or still-owned tasks require further handling.
    Unclean,
}

/// Bounded shutdown outcome with all observations transferred to the coordinator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkerShutdownReport {
    disposition: WorkerShutdownDisposition,
    cancellation_requests: usize,
    abort_requests: usize,
    observations: Vec<WorkerTerminalObservation>,
    remaining: Vec<WorkerTaskSnapshot>,
}

impl WorkerShutdownReport {
    pub(super) const fn new(
        disposition: WorkerShutdownDisposition,
        cancellation_requests: usize,
        abort_requests: usize,
        observations: Vec<WorkerTerminalObservation>,
        remaining: Vec<WorkerTaskSnapshot>,
    ) -> Self {
        Self { disposition, cancellation_requests, abort_requests, observations, remaining }
    }
    /// Returns clean only when no returned or retained work exists.
    #[must_use]
    pub(crate) const fn disposition(&self) -> WorkerShutdownDisposition {
        self.disposition
    }
    /// Returns first cancellation requests delivered during shutdown.
    #[must_use]
    pub(crate) const fn cancellation_requests(&self) -> usize {
        self.cancellation_requests
    }
    /// Returns task aborts requested after graceful cancellation expired.
    #[must_use]
    pub(crate) const fn abort_requests(&self) -> usize {
        self.abort_requests
    }
    /// Borrows joined observations that still require external authoritative settlement.
    #[must_use]
    pub(crate) fn observations(&self) -> &[WorkerTerminalObservation] {
        &self.observations
    }
    /// Borrows abort-resistant tasks whose join handles remain supervisor-owned.
    #[must_use]
    pub(crate) fn remaining(&self) -> &[WorkerTaskSnapshot] {
        &self.remaining
    }
}
