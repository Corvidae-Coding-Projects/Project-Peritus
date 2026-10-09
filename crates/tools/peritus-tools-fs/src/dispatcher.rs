//! Router-authorized filesystem dispatcher adapters.

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    BoundedText, FailureCategory, ImplementationIdentity, RecoveryRoute, ResponsibleSubsystem,
    ResultStatus, Retryability, SchemaDigest, ToolFailure, ToolResult, ToolTiming, Truncation,
    TruncationMetadata,
};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure, ToolDispatcher, ToolStart};
use peritus_workspace::{
    MutationOutcome, ReadOnlyWorkspace, WorkspaceAuthorizationRequest, WorkspaceCallerBinding,
    WorkspaceGateway,
};

use crate::{
    CompiledMutation, FsReadService, FsToolError, FsToolErrorKind, FsToolOperation, RecoveryClass,
    RenderedOutput, WorkspaceVersion, decoder, descriptor_catalog,
};

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
    Read(&'a ReadOnlyWorkspace),
    Mutation {
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
    },
}

/// Descriptor-specific dispatcher whose only effect entry consumes router authority.
pub struct FsDispatcher<'a> {
    kind: FsDispatchKind,
    identity: ImplementationIdentity,
    descriptor_digest: SchemaDigest,
    context: DispatchContext<'a>,
    mutation_outcome: Option<MutationOutcome>,
}

impl<'a> FsDispatcher<'a> {
    /// Creates a descriptor-specific immutable-read dispatcher.
    ///
    /// # Errors
    /// Rejects mutation kinds or an invalid frozen descriptor catalog.
    pub fn read(
        kind: FsDispatchKind,
        workspace: &'a ReadOnlyWorkspace,
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
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
    ) -> Result<Self, FsToolError> {
        if !kind.is_mutation() {
            return Err(FsToolError::invalid(
                FsToolOperation::Catalog,
                "read kind cannot use a mutation dispatcher",
            ));
        }
        Self::build(kind, DispatchContext::Mutation { gateway, authorization })
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
            mutation_outcome: None,
        })
    }

    /// Takes a successful C1 mutation outcome for a separately authorized candidate operation.
    #[must_use]
    pub const fn take_mutation_outcome(&mut self) -> Option<MutationOutcome> {
        self.mutation_outcome.take()
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
        let arguments = prepared.arguments();
        let rendered = match &mut self.context {
            DispatchContext::Read(workspace) => execute_read(self.kind, workspace, arguments),
            DispatchContext::Mutation { gateway, authorization } => {
                let outcome = execute_mutation(self.kind, gateway, authorization, arguments)
                    .map_err(|error| tool_failure(&error))?;
                let rendered = RenderedOutput::mutation(&outcome);
                self.mutation_outcome = Some(outcome);
                rendered
            }
        }
        .map_err(|error| tool_failure(&error))?;
        finish(&prepared, &rendered, completed_at).map(ToolStart::Completed)
    }
}

fn execute_read(
    kind: FsDispatchKind,
    workspace: &ReadOnlyWorkspace,
    arguments: &peritus_tool_protocol::BoundedJson,
) -> Result<RenderedOutput, FsToolError> {
    let service = FsReadService::new(workspace);
    match kind {
        FsDispatchKind::Discover => {
            RenderedOutput::discover(&service.discover(&decoder::discover(arguments)?)?)
        }
        FsDispatchKind::Metadata => {
            RenderedOutput::metadata(&service.metadata(&decoder::metadata(arguments)?)?)
        }
        FsDispatchKind::Read => RenderedOutput::file(&service.read(&decoder::read(arguments)?)?),
        FsDispatchKind::Search => {
            RenderedOutput::search(&service.search(&decoder::search(arguments)?)?)
        }
        _ => Err(FsToolError::invalid(
            FsToolOperation::Catalog,
            "mutation kind reached immutable dispatcher",
        )),
    }
}

fn execute_mutation(
    kind: FsDispatchKind,
    gateway: &mut WorkspaceGateway,
    authorization: &WorkspaceAuthorizationRequest<'_>,
    arguments: &peritus_tool_protocol::BoundedJson,
) -> Result<MutationOutcome, FsToolError> {
    let state = gateway.state();
    let version =
        WorkspaceVersion::new(state.binding().workspace_id(), state.generation(), state.revision());
    let compiled = match kind {
        FsDispatchKind::Create => CompiledMutation::create(version, decoder::create(arguments)?),
        FsDispatchKind::Patch => CompiledMutation::patch(version, decoder::patch(arguments)?),
        FsDispatchKind::Remove => CompiledMutation::remove(version, decoder::remove(arguments)?),
        FsDispatchKind::Replace => CompiledMutation::replace(version, decoder::replace(arguments)?),
        FsDispatchKind::Write => CompiledMutation::write(version, decoder::write(arguments)?),
        _ => Err(FsToolError::invalid(
            FsToolOperation::Catalog,
            "read kind reached mutation dispatcher",
        )),
    }?;
    gateway.apply_patch(authorization, compiled.into_patch()).map_err(|_| {
        FsToolError::new(
            FsToolErrorKind::Workspace,
            compiled_operation(kind),
            RecoveryClass::Reconcile,
            "target-owned C1 workspace mutation failed",
        )
    })
}

fn caller_binding(invocation: &AuthorizedInvocation) -> WorkspaceCallerBinding {
    let binding = invocation.binding();
    WorkspaceCallerBinding::new(
        invocation.action_id(),
        binding.actor_id(),
        binding.role(),
        binding.revision().workspace_id(),
        binding.environment_id(),
        binding.resource_id(),
        invocation.prepared().descriptor().name().clone(),
        invocation.prepared().descriptor_digest().get(),
        invocation.prepared_digest(),
    )
}

fn context_matches(context: &DispatchContext<'_>, caller: &WorkspaceCallerBinding) -> bool {
    match context {
        DispatchContext::Read(workspace) => workspace.target_binding().is_some_and(|target| {
            target.workspace_id() == caller.workspace_id()
                && target.environment_id() == caller.environment_id()
                && target.resource_id() == caller.resource_id()
        }),
        DispatchContext::Mutation { authorization, .. } => {
            authorization.caller_binding() == Some(caller)
        }
    }
}

const fn compiled_operation(kind: FsDispatchKind) -> FsToolOperation {
    match kind {
        FsDispatchKind::Create => FsToolOperation::Create,
        FsDispatchKind::Patch => FsToolOperation::Patch,
        FsDispatchKind::Remove => FsToolOperation::Remove,
        FsDispatchKind::Replace => FsToolOperation::Replace,
        FsDispatchKind::Write => FsToolOperation::Write,
        FsDispatchKind::Discover => FsToolOperation::Discover,
        FsDispatchKind::Metadata => FsToolOperation::Metadata,
        FsDispatchKind::Read => FsToolOperation::Read,
        FsDispatchKind::Search => FsToolOperation::Search,
    }
}

fn finish(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    rendered: &RenderedOutput,
    completed_at: AuthorityInstant,
) -> Result<ToolResult, DispatchFailure> {
    if rendered.structured().canonical_bytes().len() as u64
        > prepared.call().limits().output_bytes()
    {
        return Err(protocol_failure("structured result exceeds the selected call output bound"));
    }
    let timing = ToolTiming::new(completed_at, completed_at)
        .map_err(|_| protocol_failure("dispatcher completion time is invalid"))?;
    ToolResult::success(
        prepared,
        rendered.structured().clone(),
        rendered.human().clone(),
        rendered.model().clone(),
        Vec::new(),
        timing,
        TruncationMetadata {
            output: if rendered.truncated() {
                Truncation::TailDropped
            } else {
                Truncation::Complete
            },
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        0,
    )
    .map_err(|_| protocol_failure("terminal filesystem result is invalid"))
}

const fn minimum_result_capacity(prepared: &peritus_tool_protocol::PreparedToolCall) -> bool {
    let limits = prepared.call().limits();
    limits.output_bytes() >= 512 && limits.model_bytes() >= 128 && limits.human_bytes() >= 128
}

fn tool_failure(error: &FsToolError) -> DispatchFailure {
    let category = match error.kind() {
        FsToolErrorKind::Inspection | FsToolErrorKind::Patch | FsToolErrorKind::Workspace => {
            FailureCategory::Workspace
        }
        FsToolErrorKind::Unsupported => FailureCategory::Infrastructure,
        FsToolErrorKind::InvalidInput | FsToolErrorKind::Protocol => FailureCategory::Protocol,
    };
    failure(category, error.kind().code(), error.detail(), error.recovery())
}

fn protocol_failure(detail: &'static str) -> DispatchFailure {
    failure(
        FailureCategory::Protocol,
        FsToolErrorKind::Protocol.code(),
        detail,
        RecoveryClass::CorrectInput,
    )
}

fn failure(
    category: FailureCategory,
    code: &'static str,
    detail: &'static str,
    recovery: RecoveryClass,
) -> DispatchFailure {
    let (retryability, route) = match recovery {
        RecoveryClass::CorrectInput | RecoveryClass::SelectSupportedOperation => {
            (Retryability::NewAction, RecoveryRoute::None)
        }
        RecoveryClass::Reobserve | RecoveryClass::Reauthorize => {
            (Retryability::NewAction, RecoveryRoute::Reauthorize)
        }
        RecoveryClass::Reconcile => {
            (Retryability::AfterRecovery, RecoveryRoute::ReconcileWorkspace)
        }
    };
    let failure = ToolFailure::new(
        category,
        bounded(code),
        ResponsibleSubsystem::Workspace,
        retryability,
        route,
        bounded(detail),
    );
    DispatchFailure::new(ResultStatus::Failed, failure)
        .expect("non-success static dispatch failure is valid")
}

fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value.to_owned()).expect("static filesystem failure text is bounded")
}
