//! Router-authorized filesystem dispatcher adapters.

mod execution;
mod mutation_execution;
mod operations;
mod read_execution;

use execution::spawn_read;
use mutation_execution::{spawn_mutation, spawn_recovered_mutation};
use operations::{
    caller_binding, compile_mutation, context_matches, minimum_result_capacity, protocol_failure,
    retained_action_failure, tool_failure, workspace_error,
};

use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use peritus_tool_protocol::{ArtifactReference, ImplementationIdentity, SchemaDigest};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure, ToolDispatcher, ToolStart};
use peritus_workspace::{
    AuthorizedPatch, MutationOutcome, MutationRecoveryOutcome, ReadOnlyWorkspace,
    WorkspaceAuthorizationRequest, WorkspaceCallerBinding, WorkspaceGateway,
};

use crate::{ArtifactInputFailure, FsToolError, FsToolOperation, descriptor_catalog};

/// Existing host-owned artifact authority used to resolve a mutation's immutable input.
///
/// Implementations must bind the reference to `caller`'s authorized workspace scope and must
/// reject digest-only access to artifacts outside that scope. The dispatcher independently
/// checks the returned byte length and SHA-256 before compiling or applying any patch.
pub trait ArtifactInputResolver: Send + Sync {
    /// Resolves one complete exact artifact for an already-authorized caller.
    ///
    /// # Errors
    /// Returns an authority, availability, integrity, or indeterminate resolution failure.
    fn resolve_complete(
        &self,
        caller: &WorkspaceCallerBinding,
        reference: &ArtifactReference,
    ) -> Result<Vec<u8>, ArtifactInputFailure>;
}

/// Exact filesystem operation served by one descriptor-specific dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsDispatchKind {
    /// `fs.create`.
    Create,
    /// `fs.discover`.
    Discover,
    /// `fs.metadata`.
    Metadata,
    /// `fs.patch`.
    Patch,
    /// `fs.read`.
    Read,
    /// `fs.remove`.
    Remove,
    /// `fs.replace`.
    Replace,
    /// `fs.search`.
    Search,
    /// `fs.write`.
    Write,
}

impl FsDispatchKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Create => "fs.create",
            Self::Discover => "fs.discover",
            Self::Metadata => "fs.metadata",
            Self::Patch => "fs.patch",
            Self::Read => "fs.read",
            Self::Remove => "fs.remove",
            Self::Replace => "fs.replace",
            Self::Search => "fs.search",
            Self::Write => "fs.write",
        }
    }

    const fn is_mutation(self) -> bool {
        matches!(self, Self::Create | Self::Patch | Self::Remove | Self::Replace | Self::Write)
    }
}

enum DispatchContext<'a> {
    Read(Arc<ReadOnlyWorkspace>),
    Mutation {
        gateway: Arc<Mutex<WorkspaceGateway>>,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        artifact_resolver: Option<&'a dyn ArtifactInputResolver>,
    },
}

enum MutationPreparation {
    New(AuthorizedPatch),
    Recovered(MutationRecoveryOutcome),
}

/// Descriptor-specific dispatcher whose only effect entry consumes router authority.
pub struct FsDispatcher<'a> {
    kind: FsDispatchKind,
    identity: ImplementationIdentity,
    descriptor_digest: SchemaDigest,
    context: DispatchContext<'a>,
    mutation_outcome: Arc<Mutex<Option<MutationOutcome>>>,
}

impl<'a> FsDispatcher<'a> {
    /// Creates a descriptor-specific immutable-read dispatcher.
    ///
    /// # Errors
    /// Rejects mutation kinds or an invalid frozen descriptor catalog.
    pub fn read(
        kind: FsDispatchKind,
        workspace: Arc<ReadOnlyWorkspace>,
    ) -> Result<Self, FsToolError> {
        if kind.is_mutation() {
            return Err(FsToolError::invalid(
                FsToolOperation::Catalog,
                "mutation kind cannot use an immutable dispatcher",
            ));
        }
        Self::build(kind, DispatchContext::Read(workspace))
    }

    /// Creates a descriptor-specific target-owned mutation dispatcher.
    ///
    /// # Errors
    /// Rejects read kinds or an invalid frozen descriptor catalog.
    pub fn mutation(
        kind: FsDispatchKind,
        gateway: Arc<Mutex<WorkspaceGateway>>,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
    ) -> Result<Self, FsToolError> {
        if !kind.is_mutation() {
            return Err(FsToolError::invalid(
                FsToolOperation::Catalog,
                "read kind cannot use a mutation dispatcher",
            ));
        }
        Self::build(
            kind,
            DispatchContext::Mutation { gateway, authorization, artifact_resolver: None },
        )
    }

    /// Creates a target-owned mutation dispatcher with the host's existing scoped artifact
    /// authority for larger mutation inputs.
    ///
    /// Artifact bytes are resolved and verified before the patch enters the C1 gateway.
    ///
    /// # Errors
    /// Rejects read kinds or an invalid frozen descriptor catalog.
    pub fn mutation_with_artifact_resolver(
        kind: FsDispatchKind,
        gateway: Arc<Mutex<WorkspaceGateway>>,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        artifact_resolver: &'a dyn ArtifactInputResolver,
    ) -> Result<Self, FsToolError> {
        if !kind.is_mutation() {
            return Err(FsToolError::invalid(
                FsToolOperation::Catalog,
                "read kind cannot use a mutation dispatcher",
            ));
        }
        Self::build(
            kind,
            DispatchContext::Mutation {
                gateway,
                authorization,
                artifact_resolver: Some(artifact_resolver),
            },
        )
    }

    fn build(kind: FsDispatchKind, context: DispatchContext<'a>) -> Result<Self, FsToolError> {
        let descriptor = descriptor_catalog()?
            .into_iter()
            .find(|descriptor| descriptor.name().as_str() == kind.name())
            .ok_or_else(|| {
                FsToolError::invalid(FsToolOperation::Catalog, "dispatcher descriptor is absent")
            })?;
        Ok(Self {
            kind,
            identity: descriptor.implementation_identity().clone(),
            descriptor_digest: descriptor.descriptor_digest(),
            context,
            mutation_outcome: Arc::new(Mutex::new(None)),
        })
    }

    /// Takes a successful C1 mutation outcome for a separately authorized candidate operation.
    #[must_use]
    pub fn take_mutation_outcome(&mut self) -> Option<MutationOutcome> {
        self.mutation_outcome.lock().ok()?.take()
    }
}

impl ToolDispatcher for FsDispatcher<'_> {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }

    fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor_digest
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        let started_at = invocation.observed_at();
        let monotonic_started = Instant::now();
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
        let arguments = prepared.arguments();
        match &mut self.context {
            DispatchContext::Read(workspace) => spawn_read(
                self.kind,
                Arc::clone(workspace),
                prepared,
                started_at,
                monotonic_started,
            ),
            DispatchContext::Mutation { gateway, authorization, artifact_resolver } => {
                let patch =
                    compile_mutation(self.kind, gateway, *artifact_resolver, &caller, arguments)
                        .map_err(|error| tool_failure(&error))?;
                let preparation = {
                    let mut gateway = gateway
                        .lock()
                        .map_err(|_| protocol_failure("workspace gateway lock is poisoned"))?;
                    if gateway.state().action_consumed(authorization.action_id()) {
                        gateway
                            .recover_mutation(authorization, patch)
                            .map(MutationPreparation::Recovered)
                    } else {
                        gateway.prepare_patch(authorization, patch).map(MutationPreparation::New)
                    }
                }
                .map_err(|error| tool_failure(&workspace_error(self.kind, &error)))?;
                match preparation {
                    MutationPreparation::New(operation) => spawn_mutation(
                        self.kind,
                        gateway,
                        self.mutation_outcome.clone(),
                        operation,
                        prepared,
                        started_at,
                        monotonic_started,
                    ),
                    MutationPreparation::Recovered(MutationRecoveryOutcome::AlreadyApplied(
                        outcome,
                    )) => spawn_recovered_mutation(
                        self.mutation_outcome.clone(),
                        outcome,
                        prepared,
                        started_at,
                        monotonic_started,
                    ),
                    MutationPreparation::Recovered(
                        MutationRecoveryOutcome::NotAttempted | MutationRecoveryOutcome::RolledBack,
                    ) => Err(tool_failure(&retained_action_failure(self.kind, false))),
                    MutationPreparation::Recovered(
                        MutationRecoveryOutcome::Dirty | MutationRecoveryOutcome::Indeterminate,
                    ) => Err(tool_failure(&retained_action_failure(self.kind, true))),
                }
            }
        }
    }
}
