//! One owned Tokio task, cooperative cancellation sender, and join normalization.

use std::future::Future;

use tokio::{sync::watch, task::JoinHandle};

use super::{
    WorkerAssignment, WorkerCancelDisposition, WorkerCancellation, WorkerCancellationReason,
    WorkerFailureKind, WorkerSupervisorError, WorkerSupervisorErrorKind, WorkerTaskOutcome,
    WorkerTaskSnapshot, WorkerTaskState, WorkerTerminalObservation,
};

pub(super) struct OwnedWorkerTask {
    assignment: WorkerAssignment,
    cancellation: watch::Sender<Option<WorkerCancellationReason>>,
    cancellation_requested: Option<WorkerCancellationReason>,
    abort_requested: bool,
    join: JoinHandle<WorkerTaskOutcome>,
}

impl OwnedWorkerTask {
    pub(super) fn start<F, Fut>(
        assignment: WorkerAssignment,
        task: F,
    ) -> Result<Self, WorkerSupervisorError>
    where
        F: FnOnce(WorkerCancellation) -> Fut + Send + 'static,
        Fut: Future<Output = WorkerTaskOutcome> + Send + 'static,
    {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            WorkerSupervisorError::new(
                WorkerSupervisorErrorKind::RuntimeUnavailable,
                "worker task requires a running Tokio runtime",
            )
        })?;
        let (cancellation, receiver) = watch::channel(None);
        let token = WorkerCancellation::new(receiver.clone());
        let join = runtime.spawn(async move {
            let _receiver_guard = receiver;
            task(token).await
        });
        Ok(Self {
            assignment,
            cancellation,
            cancellation_requested: None,
            abort_requested: false,
            join,
        })
    }

    pub(super) fn is_finished(&self) -> bool {
        self.join.is_finished()
    }

    pub(super) fn request_cancel(
        &mut self,
        reason: WorkerCancellationReason,
    ) -> WorkerCancelDisposition {
        if self.join.is_finished() {
            return WorkerCancelDisposition::AlreadyFinished;
        }
        if let Some(retained) = self.cancellation_requested {
            return WorkerCancelDisposition::AlreadyRequested(retained);
        }
        if self.cancellation.send(Some(reason)).is_err() {
            return WorkerCancelDisposition::AlreadyFinished;
        }
        self.cancellation_requested = Some(reason);
        WorkerCancelDisposition::Requested
    }

    pub(super) fn abort(&mut self) -> bool {
        if self.join.is_finished() || self.abort_requested {
            return false;
        }
        self.abort_requested = true;
        self.join.abort();
        true
    }

    pub(super) fn snapshot(&self) -> WorkerTaskSnapshot {
        let state = if self.join.is_finished() {
            WorkerTaskState::CompletedAwaitingReap
        } else if self.abort_requested {
            WorkerTaskState::AbortRequested
        } else if let Some(reason) = self.cancellation_requested {
            WorkerTaskState::CancellationRequested(reason)
        } else {
            WorkerTaskState::Running
        };
        WorkerTaskSnapshot::new(self.assignment, state)
    }

    pub(super) async fn join(self) -> WorkerTerminalObservation {
        let outcome = match self.join.await {
            Ok(outcome) => outcome,
            Err(error) if error.is_cancelled() => WorkerTaskOutcome::Failed {
                kind: WorkerFailureKind::SupervisorAborted,
                evidence_digest: None,
            },
            Err(_) => WorkerTaskOutcome::Failed {
                kind: WorkerFailureKind::SupervisorPanicked,
                evidence_digest: None,
            },
        };
        WorkerTerminalObservation::new(self.assignment, outcome)
    }
}
