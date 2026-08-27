//! Non-cloneable owner of every scheduler-dispatched Tokio worker task.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    time::Duration,
};

use peritus_scheduler::{DispatchId, SchedulerReservation};
use tokio::time::Instant;

use super::{
    WorkerAssignment, WorkerCancelDisposition, WorkerCancellation, WorkerCancellationReason,
    WorkerCounts, WorkerDrainDisposition, WorkerReapReport, WorkerRemainingWork,
    WorkerShutdownDisposition, WorkerShutdownReport, WorkerSupervisorError,
    WorkerSupervisorErrorKind, WorkerSupervisorLimits, WorkerSupervisorPhase,
    WorkerSupervisorSnapshot, WorkerTaskOutcome, WorkerTaskSnapshot, WorkerTaskState,
    WorkerTerminalObservation, task::OwnedWorkerTask,
};

const SHUTDOWN_POLL: Duration = Duration::from_millis(5);

/// Structured bounded owner for scheduler-dispatched asynchronous work.
#[must_use = "the worker supervisor must be shut down to observe every owned task"]
pub struct WorkerSupervisor {
    phase: WorkerSupervisorPhase,
    limits: WorkerSupervisorLimits,
    tasks: BTreeMap<DispatchId, OwnedWorkerTask>,
    owned_dispatches: BTreeSet<DispatchId>,
    results: VecDeque<WorkerTerminalObservation>,
}

impl WorkerSupervisor {
    /// Creates an empty accepting supervisor.
    #[must_use]
    pub(crate) const fn new(limits: WorkerSupervisorLimits) -> Self {
        Self {
            phase: WorkerSupervisorPhase::Accepting,
            limits,
            tasks: BTreeMap::new(),
            owned_dispatches: BTreeSet::new(),
            results: VecDeque::new(),
        }
    }

    /// Spawns one task under exact scheduler identities and supervisor ownership.
    ///
    /// # Errors
    ///
    /// Rejects non-accepting lifecycle, task capacity, duplicate dispatch ownership, or absence of
    /// a running Tokio runtime.
    pub(crate) fn spawn<F, Fut>(
        &mut self,
        assignment: WorkerAssignment,
        task: F,
    ) -> Result<(), WorkerSupervisorError>
    where
        F: FnOnce(WorkerCancellation) -> Fut + Send + 'static,
        Fut: Future<Output = WorkerTaskOutcome> + Send + 'static,
    {
        if self.phase != WorkerSupervisorPhase::Accepting {
            return Err(rejected(
                WorkerSupervisorErrorKind::NotAccepting,
                "worker supervisor is draining or stopped",
            ));
        }
        if self.tasks.len() >= self.limits.maximum_active_tasks() {
            return Err(rejected(
                WorkerSupervisorErrorKind::Capacity,
                "worker supervisor active-task bound is full",
            ));
        }
        let dispatch_id = assignment.dispatch_id();
        if self.owned_dispatches.contains(&dispatch_id) {
            return Err(rejected(
                WorkerSupervisorErrorKind::DuplicateDispatch,
                "dispatch identity is already owned by this supervisor",
            ));
        }
        let owned = OwnedWorkerTask::start(assignment, task)?;
        self.tasks.insert(dispatch_id, owned);
        self.owned_dispatches.insert(dispatch_id);
        Ok(())
    }

    /// Spawns work from an exact durable scheduler reservation after start acknowledgement.
    ///
    /// This is the production admission seam. The lower-level [`Self::spawn`] remains available
    /// for recovery adapters that already hold an equivalent reconstructed assignment.
    ///
    /// # Errors
    ///
    /// Rejects a reservation whose start acknowledgement is not yet durable, then applies the
    /// normal supervisor lifecycle, capacity, uniqueness, and runtime checks.
    pub(crate) fn spawn_reserved<F, Fut>(
        &mut self,
        reservation: &SchedulerReservation,
        task: F,
    ) -> Result<(), WorkerSupervisorError>
    where
        F: FnOnce(WorkerCancellation) -> Fut + Send + 'static,
        Fut: Future<Output = WorkerTaskOutcome> + Send + 'static,
    {
        if !reservation.started() {
            return Err(rejected(
                WorkerSupervisorErrorKind::ReservationNotStarted,
                "worker effect cannot start before its durable scheduler acknowledgement",
            ));
        }
        self.spawn(WorkerAssignment::from_reservation(reservation), task)
    }

    /// Atomically closes new-work admission.
    #[must_use]
    pub(crate) fn begin_draining(&mut self) -> WorkerDrainDisposition {
        if self.phase == WorkerSupervisorPhase::Accepting {
            self.phase = WorkerSupervisorPhase::Draining;
            WorkerDrainDisposition::Began
        } else {
            WorkerDrainDisposition::AlreadyNonAccepting(self.phase)
        }
    }

    /// Requests cooperative first-wins cancellation for one active dispatch.
    ///
    /// # Errors
    ///
    /// Rejects a dispatch not currently owned by an active task.
    pub(crate) fn cancel(
        &mut self,
        dispatch_id: DispatchId,
        reason: WorkerCancellationReason,
    ) -> Result<WorkerCancelDisposition, WorkerSupervisorError> {
        if let Some(task) = self.tasks.get_mut(&dispatch_id) {
            return Ok(task.request_cancel(reason));
        }
        if self.owned_dispatches.contains(&dispatch_id) {
            return Ok(WorkerCancelDisposition::AlreadyFinished);
        }
        Err(rejected(
            WorkerSupervisorErrorKind::UnknownDispatch,
            "supervisor does not own the requested dispatch identity",
        ))
    }

    /// Joins a bounded number of completed tasks into the bounded result queue.
    ///
    /// # Errors
    ///
    /// Rejects a zero requested reap bound.
    pub(crate) async fn reap(
        &mut self,
        maximum: usize,
    ) -> Result<WorkerReapReport, WorkerSupervisorError> {
        if maximum == 0 {
            return Err(invalid_bound());
        }
        let capacity = self.limits.maximum_results().saturating_sub(self.results.len());
        let limit = maximum.min(self.limits.maximum_reap_per_pass()).min(capacity);
        let observations = self.collect_finished(limit).await;
        let reaped = observations.len();
        self.results.extend(observations);
        let blocked = self.results.len() == self.limits.maximum_results()
            && self.tasks.values().any(OwnedWorkerTask::is_finished);
        Ok(WorkerReapReport::new(reaped, self.tasks.len(), blocked))
    }

    /// Drains at most `maximum` terminal observations for authoritative settlement.
    ///
    /// # Errors
    ///
    /// Rejects a zero requested result bound.
    pub(crate) fn drain_results(
        &mut self,
        maximum: usize,
    ) -> Result<Vec<WorkerTerminalObservation>, WorkerSupervisorError> {
        if maximum == 0 {
            return Err(invalid_bound());
        }
        let count = maximum.min(self.results.len());
        let observations: Vec<_> = self.results.drain(..count).collect();
        for observation in &observations {
            self.owned_dispatches.remove(&observation.assignment().dispatch_id());
        }
        Ok(observations)
    }

    /// Returns an exact bounded lifecycle, count, and remaining-work snapshot.
    #[must_use]
    pub(crate) fn snapshot(&self) -> WorkerSupervisorSnapshot {
        let task_snapshots: Vec<_> = self.tasks.values().map(OwnedWorkerTask::snapshot).collect();
        let counts = counts(&task_snapshots, self.results.len());
        let mut remaining = Vec::with_capacity(task_snapshots.len() + self.results.len());
        remaining.extend(task_snapshots.into_iter().map(WorkerRemainingWork::Task));
        remaining.extend(self.results.iter().copied().map(WorkerRemainingWork::Observation));
        WorkerSupervisorSnapshot::new(self.phase, counts, remaining)
    }

    /// Cancels, waits, aborts, and boundedly rejoins every owned task.
    ///
    /// Returned observations still require external authoritative settlement. Abort-resistant
    /// tasks remain owned by the supervisor and appear in the unclean report.
    pub(crate) async fn shutdown(&mut self) -> WorkerShutdownReport {
        self.phase = WorkerSupervisorPhase::ShuttingDown;
        let cancellation_requests = self.cancel_all(WorkerCancellationReason::Shutdown);
        let mut observations = self.take_all_results();

        self.wait_until(self.limits.shutdown_grace(), &mut observations).await;
        let abort_requests = self.abort_remaining();
        self.wait_until(self.limits.abort_join_grace(), &mut observations).await;

        let remaining: Vec<_> = self.tasks.values().map(OwnedWorkerTask::snapshot).collect();
        self.phase = if remaining.is_empty() {
            WorkerSupervisorPhase::Stopped
        } else {
            WorkerSupervisorPhase::ShutdownIncomplete
        };
        let disposition = if remaining.is_empty() && observations.is_empty() {
            WorkerShutdownDisposition::Clean
        } else {
            WorkerShutdownDisposition::Unclean
        };
        WorkerShutdownReport::new(
            disposition,
            cancellation_requests,
            abort_requests,
            observations,
            remaining,
        )
    }

    async fn wait_until(
        &mut self,
        duration: Duration,
        observations: &mut Vec<WorkerTerminalObservation>,
    ) {
        let deadline = Instant::now() + duration;
        loop {
            let joined = self.collect_finished(self.tasks.len()).await;
            for observation in &joined {
                self.owned_dispatches.remove(&observation.assignment().dispatch_id());
            }
            observations.extend(joined);
            if self.tasks.is_empty() || Instant::now() >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            tokio::time::sleep(SHUTDOWN_POLL.min(remaining)).await;
        }
    }

    async fn collect_finished(&mut self, maximum: usize) -> Vec<WorkerTerminalObservation> {
        let dispatches: Vec<_> = self
            .tasks
            .iter()
            .filter_map(|(dispatch_id, task)| task.is_finished().then_some(*dispatch_id))
            .take(maximum)
            .collect();
        let mut observations = Vec::with_capacity(dispatches.len());
        for dispatch_id in dispatches {
            if let Some(task) = self.tasks.remove(&dispatch_id) {
                observations.push(task.join().await);
            }
        }
        observations
    }

    fn cancel_all(&mut self, reason: WorkerCancellationReason) -> usize {
        let mut requested = 0;
        for task in self.tasks.values_mut() {
            if task.request_cancel(reason) == WorkerCancelDisposition::Requested {
                requested += 1;
            }
        }
        requested
    }

    fn abort_remaining(&mut self) -> usize {
        let mut requested = 0;
        for task in self.tasks.values_mut() {
            if task.abort() {
                requested += 1;
            }
        }
        requested
    }

    fn take_all_results(&mut self) -> Vec<WorkerTerminalObservation> {
        let observations: Vec<_> = self.results.drain(..).collect();
        for observation in &observations {
            self.owned_dispatches.remove(&observation.assignment().dispatch_id());
        }
        observations
    }
}

impl Drop for WorkerSupervisor {
    fn drop(&mut self) {
        for task in self.tasks.values_mut() {
            let _ = task.abort();
        }
    }
}

fn counts(tasks: &[WorkerTaskSnapshot], pending_observations: usize) -> WorkerCounts {
    let mut running = 0;
    let mut cancellation = 0;
    let mut completed = 0;
    let mut aborted = 0;
    for task in tasks {
        match task.state() {
            WorkerTaskState::Running => running += 1,
            WorkerTaskState::CancellationRequested(_) => cancellation += 1,
            WorkerTaskState::CompletedAwaitingReap => completed += 1,
            WorkerTaskState::AbortRequested => aborted += 1,
        }
    }
    WorkerCounts::new(tasks.len(), running, cancellation, completed, aborted, pending_observations)
}

const fn invalid_bound() -> WorkerSupervisorError {
    rejected(
        WorkerSupervisorErrorKind::InvalidLimit,
        "requested worker reap or result bound is zero",
    )
}

const fn rejected(kind: WorkerSupervisorErrorKind, detail: &'static str) -> WorkerSupervisorError {
    WorkerSupervisorError::new(kind, detail)
}
