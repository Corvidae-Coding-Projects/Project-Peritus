//! Router-authorized Git dispatcher adapters.

use super::dispatcher::{GitDispatchKind, GitMutationOutcome};
use peritus_artifact_store::ArtifactStore;
use peritus_git::CandidateSnapshot;
use peritus_tool_protocol::SchemaDigest;
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure};
use peritus_workspace::{
    MutationOutcome, ReadOnlyWorkspace, RollbackRequest, WorkspaceAuthorizationRequest,
    WorkspaceCallerBinding, WorkspaceGateway,
};

use crate::{
    GitReadService, GitToolError, GitToolErrorKind, GitToolOperation, RecoveryClass,
    RenderedOutput, SnapshotInput, StatusInput, decoder, descriptor_catalog,
    dispatch_support::{
        caller_binding, minimum_result_capacity, protocol_failure, tool_failure, workspace_failure,
    },
};

enum DispatchContext<'a> {
    Read {
        workspace: &'a ReadOnlyWorkspace,
        retained: Option<&'a CandidateSnapshot>,
    },
    Candidate {
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        mutation: &'a MutationOutcome,
        artifacts: &'a ArtifactStore,
    },
    Rollback {
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        target: &'a CandidateSnapshot,
        artifacts: &'a ArtifactStore,
    },
}

/// Descriptor-specific Git dispatcher whose only effect entry consumes router authority.
pub struct GitOperation<'a> {
    kind: GitDispatchKind,
    descriptor_digest: SchemaDigest,
    context: DispatchContext<'a>,
    mutation_outcome: Option<GitMutationOutcome>,
}

impl<'a> GitOperation<'a> {
    /// Creates a status, diff, history, or snapshot dispatcher on one immutable C1 handle.
    ///
    /// # Errors
    /// Rejects an effectful kind or invalid frozen descriptor catalog.
    pub fn read(
        kind: GitDispatchKind,
        workspace: &'a ReadOnlyWorkspace,
        retained: Option<&'a CandidateSnapshot>,
    ) -> Result<Self, GitToolError> {
        if matches!(kind, GitDispatchKind::Candidate | GitDispatchKind::Rollback) {
            return Err(GitToolError::invalid(
                GitToolOperation::Catalog,
                "effectful Git kind cannot use an immutable dispatcher",
            ));
        }
        Self::build(kind, DispatchContext::Read { workspace, retained })
    }

    /// Creates the authorized candidate-plus-snapshot dispatcher.
    ///
    /// # Errors
    /// Returns a typed frozen-catalog construction failure.
    pub fn candidate(
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        mutation: &'a MutationOutcome,
        artifacts: &'a ArtifactStore,
    ) -> Result<Self, GitToolError> {
        Self::build(
            GitDispatchKind::Candidate,
            DispatchContext::Candidate { gateway, authorization, mutation, artifacts },
        )
    }

    /// Creates the authorized history-preserving rollback dispatcher.
    ///
    /// # Errors
    /// Returns a typed frozen-catalog construction failure.
    pub fn rollback(
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        target: &'a CandidateSnapshot,
        artifacts: &'a ArtifactStore,
    ) -> Result<Self, GitToolError> {
        Self::build(
            GitDispatchKind::Rollback,
            DispatchContext::Rollback { gateway, authorization, target, artifacts },
        )
    }

    fn build(kind: GitDispatchKind, context: DispatchContext<'a>) -> Result<Self, GitToolError> {
        let descriptor = descriptor_catalog()?
            .into_iter()
            .find(|descriptor| descriptor.name().as_str() == kind.name())
            .ok_or_else(|| {
                GitToolError::invalid(GitToolOperation::Catalog, "dispatcher descriptor is absent")
            })?;
        Ok(Self {
            kind,
            descriptor_digest: descriptor.descriptor_digest(),
            context,
            mutation_outcome: None,
        })
    }

    /// Takes a successful C1 Git mutation outcome after router dispatch.
    #[must_use]
    pub const fn take_mutation_outcome(&mut self) -> Option<GitMutationOutcome> {
        self.mutation_outcome.take()
    }
}

impl GitOperation<'_> {
    #[allow(
        clippy::too_many_lines,
        reason = "dispatch validates the exact prepared identity before routing one bounded Git operation"
    )]
    pub(super) fn execute(
        &mut self,
        invocation: AuthorizedInvocation,
    ) -> Result<RenderedOutput, DispatchFailure> {
        let caller = caller_binding(&invocation);
        if !context_matches(&self.context, &caller) {
            return Err(protocol_failure("authorized caller differs from the opened C1 target"));
        }
        let prepared = invocation.into_prepared();
        if prepared.descriptor().name().as_str() != self.kind.name()
            || prepared.descriptor_digest() != self.descriptor_digest
            || !minimum_result_capacity(&prepared)
        {
            return Err(protocol_failure("dispatcher identity or result capacity differs"));
        }
        let rendered = match &mut self.context {
            DispatchContext::Read { workspace, retained } => execute_read(
                self.kind,
                workspace,
                *retained,
                prepared.arguments(),
                prepared.call().limits().output_bytes(),
            ),
            DispatchContext::Candidate { gateway, authorization, mutation, artifacts } => {
                let input = decoder::candidate(prepared.arguments())
                    .map_err(|error| tool_failure(&error))?;
                let recovered = gateway
                    .recover_git_mutation(
                        authorization
                            .consumption_binding()
                            .map_err(|error| workspace_failure(&error))?,
                        authorization.action_id(),
                        authorization.action_digest().map_err(|error| workspace_failure(&error))?,
                        authorization.action_payload_digest(),
                        artifacts,
                    )
                    .map_err(|error| workspace_failure(&error))?;
                match recovered {
                    peritus_workspace::GitMutationRecoveryOutcome::Candidate(outcome)
                        if outcome.snapshot().snapshot_id() == input.snapshot_id() =>
                    {
                        self.mutation_outcome = Some(GitMutationOutcome::Candidate(outcome));
                        match self.mutation_outcome.as_ref() {
                            Some(GitMutationOutcome::Candidate(outcome)) => {
                                RenderedOutput::candidate(outcome)
                            }
                            _ => Err(GitToolError::new(
                                GitToolErrorKind::Protocol,
                                GitToolOperation::Candidate,
                                RecoveryClass::Reconcile,
                                "candidate receipt was not retained",
                            )),
                        }
                    }
                    peritus_workspace::GitMutationRecoveryOutcome::Candidate(_) => {
                        return Err(protocol_failure(
                            "candidate receipt differs from prepared input",
                        ));
                    }
                    peritus_workspace::GitMutationRecoveryOutcome::Rollback(_) => {
                        return Err(protocol_failure(
                            "rollback receipt cannot satisfy candidate call",
                        ));
                    }
                    peritus_workspace::GitMutationRecoveryOutcome::Incomplete => {
                        return Err(tool_failure(&GitToolError::new(
                            GitToolErrorKind::Workspace,
                            GitToolOperation::Candidate,
                            RecoveryClass::Reconcile,
                            "candidate effect has no complete durable receipt",
                        )));
                    }
                    peritus_workspace::GitMutationRecoveryOutcome::NotFound => {
                        let outcome = gateway
                            .create_candidate(
                                authorization,
                                mutation,
                                input.snapshot_id(),
                                artifacts,
                            )
                            .map_err(|error| workspace_failure(&error))?;
                        self.mutation_outcome = Some(GitMutationOutcome::Candidate(outcome));
                        match self.mutation_outcome.as_ref() {
                            Some(GitMutationOutcome::Candidate(outcome)) => {
                                RenderedOutput::candidate(outcome)
                            }
                            _ => {
                                return Err(protocol_failure("candidate receipt was not retained"));
                            }
                        }
                    }
                }
            }
            DispatchContext::Rollback { gateway, authorization, target, artifacts } => {
                let input = decoder::rollback(prepared.arguments())
                    .map_err(|error| tool_failure(&error))?;
                if input.target_snapshot_id() != target.snapshot_id() {
                    return Err(protocol_failure("rollback target differs from prepared input"));
                }
                let recovered = gateway
                    .recover_git_mutation(
                        authorization
                            .consumption_binding()
                            .map_err(|error| workspace_failure(&error))?,
                        authorization.action_id(),
                        authorization.action_digest().map_err(|error| workspace_failure(&error))?,
                        authorization.action_payload_digest(),
                        artifacts,
                    )
                    .map_err(|error| workspace_failure(&error))?;
                match recovered {
                    peritus_workspace::GitMutationRecoveryOutcome::Rollback(outcome)
                        if outcome.snapshot().snapshot_id() == input.successor_snapshot_id()
                            && outcome.restored_from() == target.commit() =>
                    {
                        self.mutation_outcome = Some(GitMutationOutcome::Rollback(outcome));
                        match self.mutation_outcome.as_ref() {
                            Some(GitMutationOutcome::Rollback(outcome)) => {
                                RenderedOutput::rollback(outcome)
                            }
                            _ => Err(GitToolError::new(
                                GitToolErrorKind::Protocol,
                                GitToolOperation::Rollback,
                                RecoveryClass::Reconcile,
                                "rollback receipt was not retained",
                            )),
                        }
                    }
                    peritus_workspace::GitMutationRecoveryOutcome::Rollback(_) => {
                        return Err(protocol_failure(
                            "rollback receipt differs from prepared input",
                        ));
                    }
                    peritus_workspace::GitMutationRecoveryOutcome::Candidate(_) => {
                        return Err(protocol_failure(
                            "candidate receipt cannot satisfy rollback call",
                        ));
                    }
                    peritus_workspace::GitMutationRecoveryOutcome::Incomplete => {
                        return Err(tool_failure(&GitToolError::new(
                            GitToolErrorKind::Workspace,
                            GitToolOperation::Rollback,
                            RecoveryClass::Reconcile,
                            "rollback effect has no complete durable receipt",
                        )));
                    }
                    peritus_workspace::GitMutationRecoveryOutcome::NotFound => {
                        let outcome = gateway
                            .rollback(
                                authorization,
                                RollbackRequest::new(target, input.successor_snapshot_id()),
                                artifacts,
                            )
                            .map_err(|error| workspace_failure(&error))?;
                        self.mutation_outcome = Some(GitMutationOutcome::Rollback(outcome));
                        match self.mutation_outcome.as_ref() {
                            Some(GitMutationOutcome::Rollback(outcome)) => {
                                RenderedOutput::rollback(outcome)
                            }
                            _ => return Err(protocol_failure("rollback receipt was not retained")),
                        }
                    }
                }
            }
        }
        .map_err(|error| tool_failure(&error))?;
        Ok(rendered)
    }
}

fn execute_read(
    kind: GitDispatchKind,
    workspace: &ReadOnlyWorkspace,
    retained: Option<&CandidateSnapshot>,
    arguments: &peritus_tool_protocol::BoundedJson,
    output_bytes: u64,
) -> Result<RenderedOutput, GitToolError> {
    let service = GitReadService::new(workspace);
    match kind {
        GitDispatchKind::Status => RenderedOutput::status(&service.status(StatusInput)?),
        GitDispatchKind::Diff => RenderedOutput::diff_with_budget(
            &service.diff(&decoder::diff(arguments)?)?,
            output_bytes,
        ),
        GitDispatchKind::History => RenderedOutput::history_with_budget(
            &service.history(decoder::history(arguments)?)?,
            output_bytes,
        ),
        GitDispatchKind::Snapshot => match decoder::snapshot(arguments)? {
            SnapshotInput::Current => RenderedOutput::snapshot(&service.current_snapshot()),
            input @ SnapshotInput::Retained(_) => {
                let retained = retained.ok_or_else(|| {
                    GitToolError::invalid(
                        GitToolOperation::Snapshot,
                        "retained snapshot was not resolved by the C1 owner",
                    )
                })?;
                RenderedOutput::retained_snapshot(&service.retained_snapshot(input, retained)?)
            }
        },
        _ => Err(GitToolError::invalid(
            GitToolOperation::Catalog,
            "effectful kind reached immutable Git dispatcher",
        )),
    }
}

fn context_matches(context: &DispatchContext<'_>, caller: &WorkspaceCallerBinding) -> bool {
    match context {
        DispatchContext::Read { workspace, .. } => {
            workspace.target_binding().is_some_and(|target| {
                target.workspace_id() == caller.workspace_id()
                    && target.environment_id() == caller.environment_id()
                    && target.resource_id() == caller.resource_id()
            })
        }
        DispatchContext::Candidate { authorization, .. }
        | DispatchContext::Rollback { authorization, .. } => {
            authorization.caller_binding() == Some(caller)
        }
    }
}
