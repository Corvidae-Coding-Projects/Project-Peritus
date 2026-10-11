//! Router-owned, cancellable Git execution with joined worker lifetimes.

use crate::{
    GitToolError, GitToolOperation, descriptor_catalog, dispatch_support::protocol_failure,
};
use peritus_artifact_store::ArtifactStore;
use peritus_git::{CandidateSnapshot, GitCancellation};
use peritus_tool_protocol::{ImplementationIdentity, SchemaDigest};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure, ToolDispatcher, ToolStart};
use peritus_workspace::{
    CandidateOutcome, MutationOutcome, OwnedWorkspaceAuthorization, ReadOnlyWorkspace,
    RollbackOutcome, WorkspaceGateway,
};
use std::sync::{Arc, Mutex};

mod context;
mod execution;
use context::OwnedContext;

/// Exact Git operation served by one descriptor-specific dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitDispatchKind {
    /// `git.candidate`.
    Candidate,
    /// `git.diff`.
    Diff,
    /// `git.history`.
    History,
    /// `git.rollback`.
    Rollback,
    /// `git.snapshot`.
    Snapshot,
    /// `git.status`.
    Status,
}

impl GitDispatchKind {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Candidate => "git.candidate",
            Self::Diff => "git.diff",
            Self::History => "git.history",
            Self::Rollback => "git.rollback",
            Self::Snapshot => "git.snapshot",
            Self::Status => "git.status",
        }
    }
}

/// Exact committed workspace outcome retained separately from terminal rendering.
pub enum GitMutationOutcome {
    /// Candidate and retained snapshot were created.
    Candidate(CandidateOutcome),
    /// A retained snapshot was restored as a successor.
    Rollback(RollbackOutcome),
}

/// One-shot Git dispatcher transferring complete operation ownership to the router.
/// The worker releases its workspace and artifact ownership before terminal publication.
pub struct GitDispatcher {
    kind: GitDispatchKind,
    identity: ImplementationIdentity,
    descriptor_digest: SchemaDigest,
    context: Option<OwnedContext>,
    outcome: Arc<Mutex<Option<GitMutationOutcome>>>,
}

impl GitDispatcher {
    /// Owns an immutable status, diff, history, or snapshot operation.
    ///
    /// # Errors
    /// Rejects a mutation kind or invalid descriptor catalog.
    pub fn read(
        kind: GitDispatchKind,
        workspace: Arc<Mutex<ReadOnlyWorkspace>>,
        retained: Option<CandidateSnapshot>,
    ) -> Result<Self, GitToolError> {
        if matches!(kind, GitDispatchKind::Candidate | GitDispatchKind::Rollback) {
            return Err(GitToolError::invalid(
                GitToolOperation::Catalog,
                "mutation requires writable authority",
            ));
        }
        Self::build(kind, OwnedContext::Read { workspace, retained: retained.map(Box::new) })
    }

    /// Owns a candidate operation and its original move-only authority receipts.
    ///
    /// # Errors
    /// Returns an invalid descriptor-catalog failure.
    pub fn candidate(
        gateway: Arc<Mutex<WorkspaceGateway>>,
        authorization: OwnedWorkspaceAuthorization,
        mutation: Arc<MutationOutcome>,
        artifacts: Arc<Mutex<ArtifactStore>>,
    ) -> Result<Self, GitToolError> {
        Self::build(
            GitDispatchKind::Candidate,
            OwnedContext::Candidate {
                gateway,
                authorization: Box::new(authorization),
                mutation,
                artifacts,
            },
        )
    }

    /// Owns a history-preserving rollback and its original authority receipts.
    ///
    /// # Errors
    /// Returns an invalid descriptor-catalog failure.
    pub fn rollback(
        gateway: Arc<Mutex<WorkspaceGateway>>,
        authorization: OwnedWorkspaceAuthorization,
        target: CandidateSnapshot,
        artifacts: Arc<Mutex<ArtifactStore>>,
    ) -> Result<Self, GitToolError> {
        Self::build(
            GitDispatchKind::Rollback,
            OwnedContext::Rollback {
                gateway,
                authorization: Box::new(authorization),
                target: Box::new(target),
                artifacts,
            },
        )
    }

    fn build(kind: GitDispatchKind, context: OwnedContext) -> Result<Self, GitToolError> {
        let descriptor = descriptor_catalog()?
            .into_iter()
            .find(|value| value.name().as_str() == kind.name())
            .ok_or_else(|| {
                GitToolError::invalid(GitToolOperation::Catalog, "dispatcher descriptor is absent")
            })?;
        Ok(Self {
            kind,
            identity: descriptor.implementation_identity().clone(),
            descriptor_digest: descriptor.descriptor_digest(),
            context: Some(context),
            outcome: Arc::new(Mutex::new(None)),
        })
    }

    /// Takes the exact durable mutation outcome after execution has completed.
    ///
    /// # Errors
    /// Returns a typed failure if the worker's result slot was poisoned.
    pub fn take_mutation_outcome(&self) -> Result<Option<GitMutationOutcome>, DispatchFailure> {
        self.outcome
            .lock()
            .map(|mut value| value.take())
            .map_err(|_| protocol_failure("Git outcome ownership was poisoned"))
    }
}

impl ToolDispatcher for GitDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }
    fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor_digest
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        let context = self
            .context
            .take()
            .ok_or_else(|| protocol_failure("Git operation was already transferred"))?;
        let cancellation = GitCancellation::new();
        execution::start(self.kind, context, invocation, cancellation, Arc::clone(&self.outcome))
            .map(|execution| ToolStart::Active(Box::new(execution)))
    }
}
