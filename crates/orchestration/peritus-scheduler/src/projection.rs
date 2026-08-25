//! Rebuildable read-only scheduler projections.

use peritus_types::{ActorId, RevisionTuple, RunId, Sha256Digest};

use crate::{
    AttemptNumber, DispatchId, SchedulerPhase, SchedulerState, SchedulerTerminalKind, WorkId,
    WorkPhase, WorkerId, WorkerPhase,
};

/// Projected worker row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedWorker {
    id: WorkerId,
    owner: ActorId,
    phase: WorkerPhase,
    active: u16,
}
impl ProjectedWorker {
    /// Returns identity.
    #[must_use]
    pub const fn id(&self) -> WorkerId {
        self.id
    }
    /// Returns owner.
    #[must_use]
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    /// Returns phase.
    #[must_use]
    pub const fn phase(&self) -> WorkerPhase {
        self.phase
    }
    /// Returns live reservation count.
    #[must_use]
    pub const fn active(&self) -> u16 {
        self.active
    }
}

/// Projected work row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedWork {
    id: WorkId,
    phase: WorkPhase,
    attempts: u16,
    bypasses: u16,
    parent: Option<WorkId>,
}
impl ProjectedWork {
    /// Returns identity.
    #[must_use]
    pub const fn id(&self) -> WorkId {
        self.id
    }
    /// Returns phase.
    #[must_use]
    pub const fn phase(&self) -> WorkPhase {
        self.phase
    }
    /// Returns attempts started.
    #[must_use]
    pub const fn attempts(&self) -> u16 {
        self.attempts
    }
    /// Returns current bypass count.
    #[must_use]
    pub const fn bypasses(&self) -> u16 {
        self.bypasses
    }
    /// Returns parent work.
    #[must_use]
    pub const fn parent(&self) -> Option<WorkId> {
        self.parent
    }
}

/// Projected live reservation row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedReservation {
    dispatch_id: DispatchId,
    work_id: WorkId,
    worker_id: WorkerId,
    owner: ActorId,
    attempt: AttemptNumber,
    started: bool,
}
impl ProjectedReservation {
    /// Returns dispatch.
    #[must_use]
    pub const fn dispatch_id(&self) -> DispatchId {
        self.dispatch_id
    }
    /// Returns work.
    #[must_use]
    pub const fn work_id(&self) -> WorkId {
        self.work_id
    }
    /// Returns worker.
    #[must_use]
    pub const fn worker_id(&self) -> WorkerId {
        self.worker_id
    }
    /// Returns owner.
    #[must_use]
    pub const fn owner(&self) -> ActorId {
        self.owner
    }
    /// Returns attempt.
    #[must_use]
    pub const fn attempt(&self) -> AttemptNumber {
        self.attempt
    }
    /// Returns start acknowledgement state.
    #[must_use]
    pub const fn started(&self) -> bool {
        self.started
    }
}

/// Projected run summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedScheduler {
    run_id: RunId,
    revision: RevisionTuple,
    phase: SchedulerPhase,
    terminal: Option<SchedulerTerminalKind>,
    sequence: u64,
    state_digest: Sha256Digest,
}
impl ProjectedScheduler {
    /// Returns run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns phase.
    #[must_use]
    pub const fn phase(&self) -> SchedulerPhase {
        self.phase
    }
    /// Returns terminal classification.
    #[must_use]
    pub const fn terminal(&self) -> Option<SchedulerTerminalKind> {
        self.terminal
    }
    /// Returns sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns complete state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
}

/// Complete canonical projection without execution authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerProjection {
    scheduler: ProjectedScheduler,
    workers: Vec<ProjectedWorker>,
    work: Vec<ProjectedWork>,
    reservations: Vec<ProjectedReservation>,
}
impl SchedulerProjection {
    /// Projects one checked state deterministically.
    #[must_use]
    pub fn from_state(state: &SchedulerState) -> Self {
        let scheduler = ProjectedScheduler {
            run_id: state.run_id(),
            revision: state.binding().revision(),
            phase: state.phase(),
            terminal: state.terminal().map(crate::SchedulerTerminal::kind),
            sequence: state.sequence().get(),
            state_digest: state.state_digest(),
        };
        let workers = state
            .workers()
            .iter()
            .map(|record| ProjectedWorker {
                id: record.descriptor().id(),
                owner: record.descriptor().owner(),
                phase: record.phase(),
                active: u16::try_from(
                    state
                        .reservations()
                        .iter()
                        .filter(|reservation| reservation.worker_id() == record.descriptor().id())
                        .count(),
                )
                .unwrap_or(u16::MAX),
            })
            .collect();
        let work = state
            .work()
            .iter()
            .map(|record| ProjectedWork {
                id: record.spec().id(),
                phase: record.phase(),
                attempts: record.attempts_started(),
                bypasses: record.bypasses(),
                parent: record.spec().parent(),
            })
            .collect();
        let reservations = state
            .reservations()
            .iter()
            .map(|value| ProjectedReservation {
                dispatch_id: value.dispatch_id(),
                work_id: value.work_id(),
                worker_id: value.worker_id(),
                owner: value.owner(),
                attempt: value.attempt(),
                started: value.started(),
            })
            .collect();
        Self { scheduler, workers, work, reservations }
    }
    /// Borrows run summary.
    #[must_use]
    pub const fn scheduler(&self) -> &ProjectedScheduler {
        &self.scheduler
    }
    /// Borrows worker rows.
    #[must_use]
    pub fn workers(&self) -> &[ProjectedWorker] {
        &self.workers
    }
    /// Borrows work rows.
    #[must_use]
    pub fn work(&self) -> &[ProjectedWork] {
        &self.work
    }
    /// Borrows reservation rows.
    #[must_use]
    pub fn reservations(&self) -> &[ProjectedReservation] {
        &self.reservations
    }
}
