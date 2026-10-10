//! Router owner for the joined Git worker and its cancellation signal.

use super::{GitDispatchKind, GitMutationOutcome, OwnedContext};
use crate::{
    RenderedOutput,
    dispatch_support::{finish, indeterminate_failure, protocol_failure},
};
use peritus_git::GitCancellation;
use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{CancellationReason, PreparedToolCall, ResultStatus, ToolControl};
use peritus_tool_router::{
    AuthorizedInvocation, DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolExecution,
};
use std::{
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

pub(super) struct GitExecution {
    prepared: PreparedToolCall,
    started_at: AuthorityInstant,
    cancellation: GitCancellation,
    worker: Option<JoinHandle<Result<RenderedOutput, DispatchFailure>>>,
    mutation: bool,
}

pub(super) fn start(
    kind: GitDispatchKind,
    context: OwnedContext,
    invocation: AuthorizedInvocation,
    cancellation: GitCancellation,
    outcome: Arc<Mutex<Option<GitMutationOutcome>>>,
) -> Result<GitExecution, DispatchFailure> {
    let prepared = invocation.prepared().clone();
    let started_at = invocation.observed_at();
    let worker_cancellation = cancellation.clone();
    let worker = thread::Builder::new()
        .name("peritus-git-operation".to_owned())
        .spawn(move || context.run(kind, invocation, worker_cancellation, &outcome))
        .map_err(|_| protocol_failure("cannot start owned Git worker"))?;
    Ok(GitExecution {
        prepared,
        started_at,
        cancellation,
        worker: Some(worker),
        mutation: matches!(kind, GitDispatchKind::Candidate | GitDispatchKind::Rollback),
    })
}

impl ToolExecution for GitExecution {
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| protocol_failure("Git worker was already joined"))?;
        if !worker.is_finished() {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), None)
                .map_err(|_| protocol_failure("cannot encode pending Git execution"));
        }
        let worker =
            self.worker.take().ok_or_else(|| protocol_failure("Git worker was already joined"))?;
        let result = worker
            .join()
            .map_err(|_| indeterminate_failure("owned Git worker panicked; reconcile effects"))?;
        let rendered = result.map_err(|failure| {
            if self.cancellation.is_cancelled() {
                DispatchFailure::new(
                    if self.mutation && failure.status() != ResultStatus::Cancelled {
                        ResultStatus::Indeterminate
                    } else {
                        ResultStatus::Cancelled
                    },
                    failure.failure().clone(),
                )
                .expect("cancellation and indeterminate are non-success statuses")
            } else {
                failure
            }
        })?;
        let terminal = finish(&self.prepared, &rendered, self.started_at, observed_at).map_err(|failure| {
            if self.mutation {
                indeterminate_failure("Git mutation completed but terminal publication failed; recover its durable receipt")
            } else { failure }
        })?;
        ExecutionUpdate::new(&self.prepared, Vec::new(), Some(terminal))
            .map_err(|_| protocol_failure("cannot encode terminal Git execution"))
    }

    fn control(
        &mut self,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        match control {
            ToolControl::Poll => self.poll(observed_at),
            ToolControl::Cancel(reason) => self.cancel(reason, observed_at),
            _ => Err(protocol_failure("Git execution only supports polling and cancellation")),
        }
    }

    fn cancel(
        &mut self,
        _reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.cancellation.cancel();
        self.poll(observed_at)
    }

    fn recover(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure> {
        let update = self.poll(observed_at)?;
        Ok(if update.terminal().is_some() {
            RecoveryObservation::Completed(update)
        } else {
            RecoveryObservation::Active(update)
        })
    }
}

impl Drop for GitExecution {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.cancellation.cancel();
            // The router cannot release the workspace while its worker or Git descendants live.
            let _ = worker.join();
        }
    }
}
