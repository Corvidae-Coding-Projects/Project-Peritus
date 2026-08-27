//! Exact scheduler bindings and redaction-safe terminal task observations.

use peritus_scheduler::{DispatchId, SchedulerReservation, WorkId, WorkerId};
use peritus_types::Sha256Digest;

/// Immutable scheduler identities owned by one spawned task.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorkerAssignment {
    work_id: WorkId,
    dispatch_id: DispatchId,
    worker_id: WorkerId,
}

impl WorkerAssignment {
    /// Creates one exact scheduler assignment.
    #[must_use]
    pub(crate) const fn new(work_id: WorkId, dispatch_id: DispatchId, worker_id: WorkerId) -> Self {
        Self { work_id, dispatch_id, worker_id }
    }

    /// Projects exact identities from an already durable scheduler reservation.
    #[must_use]
    pub(crate) const fn from_reservation(reservation: &SchedulerReservation) -> Self {
        Self::new(reservation.work_id(), reservation.dispatch_id(), reservation.worker_id())
    }

    /// Returns the durable work identity.
    #[must_use]
    pub(crate) const fn work_id(self) -> WorkId {
        self.work_id
    }
    /// Returns the unique dispatch reservation identity.
    #[must_use]
    pub(crate) const fn dispatch_id(self) -> DispatchId {
        self.dispatch_id
    }
    /// Returns the scheduler worker identity.
    #[must_use]
    pub(crate) const fn worker_id(self) -> WorkerId {
        self.worker_id
    }
}

/// Closed reason delivered through the cooperative task cancellation token.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkerCancellationReason {
    /// The owning scheduler cancelled this dispatch.
    Scheduler,
    /// The authenticated user cancelled the parent operation.
    User,
    /// Daemon draining requires the effect to stop or durably pause.
    Shutdown,
    /// Recovery determined that the active attempt must stop.
    Recovery,
}

/// Closed redaction-safe worker failure classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkerFailureKind {
    /// An authoritative domain reducer rejected or failed the operation.
    Domain,
    /// A provider adapter or model stream failed.
    Provider,
    /// A checked tool execution failed.
    Tool,
    /// Workspace materialization failed.
    Materialization,
    /// Evaluation, debugging, or analysis failed.
    Analysis,
    /// A rollout or activation adapter failed.
    Rollout,
    /// The task returned an explicitly indeterminate effect result.
    Indeterminate,
    /// Tokio observed a task panic without exposing its payload.
    SupervisorPanicked,
    /// The supervisor aborted the task after bounded shutdown grace.
    SupervisorAborted,
}

/// Only terminal value a supervised worker future may return.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerTaskOutcome {
    /// Work completed with an exact already-computed result digest.
    Completed {
        /// Digest of the authoritative or publishable result observation.
        result_digest: Sha256Digest,
    },
    /// Work failed under a closed category and optional evidence digest.
    Failed {
        /// Redaction-safe failure class.
        kind: WorkerFailureKind,
        /// Optional exact evidence reference digest.
        evidence_digest: Option<Sha256Digest>,
    },
    /// The task cooperatively observed cancellation.
    Cancelled(WorkerCancellationReason),
}

/// One terminal task observation fenced to its exact scheduler assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerTerminalObservation {
    assignment: WorkerAssignment,
    outcome: WorkerTaskOutcome,
}

impl WorkerTerminalObservation {
    pub(super) const fn new(assignment: WorkerAssignment, outcome: WorkerTaskOutcome) -> Self {
        Self { assignment, outcome }
    }

    /// Returns the exact scheduler assignment.
    #[must_use]
    pub(crate) const fn assignment(self) -> WorkerAssignment {
        self.assignment
    }
    /// Returns the redaction-safe terminal outcome.
    #[must_use]
    pub(crate) const fn outcome(self) -> WorkerTaskOutcome {
        self.outcome
    }
}
