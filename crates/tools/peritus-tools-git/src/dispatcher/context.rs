//! Complete worker inputs, retaining workspace and artifact-store ownership until join.

use super::{
    Arc, ArtifactStore, CandidateSnapshot, GitCancellation, GitDispatchKind, GitMutationOutcome,
    MutationOutcome, Mutex, OwnedWorkspaceAuthorization, ReadOnlyWorkspace, WorkspaceGateway,
};
use crate::{RenderedOutput, dispatch_support::protocol_failure, operation::GitOperation};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure};

pub(super) enum OwnedContext {
    Read {
        workspace: Arc<Mutex<ReadOnlyWorkspace>>,
        retained: Option<Box<CandidateSnapshot>>,
    },
    Candidate {
        gateway: Arc<Mutex<WorkspaceGateway>>,
        authorization: Box<OwnedWorkspaceAuthorization>,
        mutation: Arc<MutationOutcome>,
        artifacts: Arc<Mutex<ArtifactStore>>,
    },
    Rollback {
        gateway: Arc<Mutex<WorkspaceGateway>>,
        authorization: Box<OwnedWorkspaceAuthorization>,
        target: Box<CandidateSnapshot>,
        artifacts: Arc<Mutex<ArtifactStore>>,
    },
}

impl OwnedContext {
    pub(super) fn run(
        self,
        kind: GitDispatchKind,
        invocation: AuthorizedInvocation,
        cancellation: GitCancellation,
        outcome: &Mutex<Option<GitMutationOutcome>>,
    ) -> Result<RenderedOutput, DispatchFailure> {
        match self {
            Self::Read { workspace, retained } => {
                let mut workspace = lock_owned(&workspace, &cancellation)?;
                workspace.set_git_cancellation(cancellation);
                let result = GitOperation::read(kind, &workspace, retained.as_deref())
                    .map_err(|error| crate::dispatch_support::tool_failure(&error))
                    .and_then(|mut operation| operation.execute(invocation));
                workspace.set_git_cancellation(GitCancellation::new());
                result
            }
            Self::Candidate { gateway, authorization, mutation, artifacts } => {
                let mut gateway = lock_owned(&gateway, &cancellation)?;
                let artifacts = lock_owned(&artifacts, &cancellation)?;
                gateway.set_git_cancellation(cancellation);
                let request = authorization.as_request();
                let (result, retained) =
                    match GitOperation::candidate(&mut gateway, &request, &mutation, &artifacts) {
                        Ok(mut operation) => {
                            let result = operation.execute(invocation);
                            (result, operation.take_mutation_outcome())
                        }
                        Err(error) => (Err(crate::dispatch_support::tool_failure(&error)), None),
                    };
                drop(artifacts);
                gateway.set_git_cancellation(GitCancellation::new());
                drop(gateway);
                *outcome
                    .lock()
                    .map_err(|_| protocol_failure("Git outcome ownership was poisoned"))? =
                    retained;
                result
            }
            Self::Rollback { gateway, authorization, target, artifacts } => {
                let mut gateway = lock_owned(&gateway, &cancellation)?;
                let artifacts = lock_owned(&artifacts, &cancellation)?;
                gateway.set_git_cancellation(cancellation);
                let request = authorization.as_request();
                let (result, retained) =
                    match GitOperation::rollback(&mut gateway, &request, &target, &artifacts) {
                        Ok(mut operation) => {
                            let result = operation.execute(invocation);
                            (result, operation.take_mutation_outcome())
                        }
                        Err(error) => (Err(crate::dispatch_support::tool_failure(&error)), None),
                    };
                drop(artifacts);
                gateway.set_git_cancellation(GitCancellation::new());
                drop(gateway);
                *outcome
                    .lock()
                    .map_err(|_| protocol_failure("Git outcome ownership was poisoned"))? =
                    retained;
                result
            }
        }
    }
}

fn lock_owned<'a, T>(
    owner: &'a Mutex<T>,
    cancellation: &GitCancellation,
) -> Result<std::sync::MutexGuard<'a, T>, DispatchFailure> {
    loop {
        if cancellation.is_cancelled() {
            return Err(crate::dispatch_support::cancelled_before_effect());
        }
        match owner.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(protocol_failure("Git target ownership was poisoned"));
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}
