//! Router-authorized Git dispatcher adapters.

use peritus_artifact_store::ArtifactStore;
use peritus_git::CandidateSnapshot;
use peritus_tool_protocol::{ImplementationIdentity, SchemaDigest};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure, ToolDispatcher, ToolStart};
use peritus_workspace::{
    CandidateOutcome, MutationOutcome, ReadOnlyWorkspace, RollbackOutcome, RollbackRequest,
    WorkspaceAuthorizationRequest, WorkspaceCallerBinding, WorkspaceGateway,
};

use crate::{
    GitReadService, GitToolError, GitToolOperation, RenderedOutput, SnapshotInput, StatusInput,
    decoder, descriptor_catalog,
    dispatch_support::{
        caller_binding, finish, minimum_result_capacity, protocol_failure, tool_failure,
        unsupported_failure, workspace_failure,
    },
};

/// Exact Git operation served by one descriptor-specific dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitDispatchKind {
    /// `git.candidate`.
    Candidate,
    /// `git.diff`.
    Diff,
    /// `git.history`.
    History,
    /// `git.merge`, currently typed unsupported.
    Merge,
    /// `git.rollback`.
    Rollback,
    /// `git.snapshot`.
    Snapshot,
    /// `git.status`.
    Status,
}

impl GitDispatchKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Candidate => "git.candidate",
            Self::Diff => "git.diff",
            Self::History => "git.history",
            Self::Merge => "git.merge",
            Self::Rollback => "git.rollback",
            Self::Snapshot => "git.snapshot",
            Self::Status => "git.status",
        }
    }
}

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
    MergeUnsupported,
}

/// Successful Git mutation retained after synchronous router dispatch.
pub enum GitMutationOutcome {
    /// Candidate and retained snapshot were created.
    Candidate(CandidateOutcome),
    /// A retained snapshot was restored as a successor.
    Rollback(RollbackOutcome),
}

/// Descriptor-specific Git dispatcher whose only effect entry consumes router authority.
pub struct GitDispatcher<'a> {
    kind: GitDispatchKind,
    identity: ImplementationIdentity,
    descriptor_digest: SchemaDigest,
    context: DispatchContext<'a>,
    mutation_outcome: Option<GitMutationOutcome>,
}

impl<'a> GitDispatcher<'a> {
    /// Creates a status, diff, history, or snapshot dispatcher on one immutable C1 handle.
    ///
    /// # Errors
    /// Rejects an effectful kind or invalid frozen descriptor catalog.
    pub fn read(
        kind: GitDispatchKind,
        workspace: &'a ReadOnlyWorkspace,
        retained: Option<&'a CandidateSnapshot>,
    ) -> Result<Self, GitToolError> {
        if matches!(
            kind,
            GitDispatchKind::Candidate | GitDispatchKind::Rollback | GitDispatchKind::Merge
        ) {
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

    /// Creates the authorized-but-unsupported merge dispatcher with no target mutation handle.
    ///
    /// # Errors
    /// Returns a typed frozen-catalog construction failure.
    pub fn merge_unsupported() -> Result<Self, GitToolError> {
        Self::build(GitDispatchKind::Merge, DispatchContext::MergeUnsupported)
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
            identity: descriptor.implementation_identity().clone(),
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

impl ToolDispatcher for GitDispatcher<'_> {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }

    fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor_digest
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        let completed_at = invocation.observed_at();
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
            DispatchContext::Read { workspace, retained } => {
                execute_read(self.kind, workspace, *retained, prepared.arguments())
            }
            DispatchContext::Candidate { gateway, authorization, mutation, artifacts } => {
                let input = decoder::candidate(prepared.arguments())
                    .map_err(|error| tool_failure(&error))?;
                let outcome = gateway
                    .create_candidate(authorization, mutation, input.snapshot_id(), artifacts)
                    .map_err(|_| workspace_failure("target-owned candidate creation failed"))?;
                let rendered = RenderedOutput::candidate(&outcome);
                self.mutation_outcome = Some(GitMutationOutcome::Candidate(outcome));
                rendered
            }
            DispatchContext::Rollback { gateway, authorization, target, artifacts } => {
                let input = decoder::rollback(prepared.arguments())
                    .map_err(|error| tool_failure(&error))?;
                if input.target_snapshot_id() != target.snapshot_id() {
                    return Err(protocol_failure("rollback target differs from prepared input"));
                }
                let outcome = gateway
                    .rollback(
                        authorization,
                        RollbackRequest::new(target, input.successor_snapshot_id()),
                        artifacts,
                    )
                    .map_err(|_| workspace_failure("target-owned rollback failed"))?;
                let rendered = RenderedOutput::rollback(&outcome);
                self.mutation_outcome = Some(GitMutationOutcome::Rollback(outcome));
                rendered
            }
            DispatchContext::MergeUnsupported => return Err(unsupported_failure()),
        }
        .map_err(|error| tool_failure(&error))?;
        finish(&prepared, &rendered, completed_at).map(ToolStart::Completed)
    }
}

fn execute_read(
    kind: GitDispatchKind,
    workspace: &ReadOnlyWorkspace,
    retained: Option<&CandidateSnapshot>,
    arguments: &peritus_tool_protocol::BoundedJson,
) -> Result<RenderedOutput, GitToolError> {
    let service = GitReadService::new(workspace);
    match kind {
        GitDispatchKind::Status => RenderedOutput::status(&service.status(StatusInput)?),
        GitDispatchKind::Diff => RenderedOutput::diff(&service.diff(&decoder::diff(arguments)?)?),
        GitDispatchKind::History => {
            RenderedOutput::history(&service.history(decoder::history(arguments)?)?)
        }
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
        DispatchContext::MergeUnsupported => true,
    }
}
