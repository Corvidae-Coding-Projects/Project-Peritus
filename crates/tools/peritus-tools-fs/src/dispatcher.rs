//! Router-authorized filesystem dispatcher adapters.

use std::sync::{Arc, Mutex};

use peritus_policy::AuthorityInstant;
use peritus_artifact_store::ArtifactStore;
use peritus_tool_protocol::{
    BoundedText, CancellationReason, FailureCategory, ImplementationIdentity, ProgressKind,
    RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability, SchemaDigest, ToolControl,
    ToolFailure, ToolProgress, ToolResult, ToolTiming, Truncation, TruncationMetadata,
};
use peritus_tool_router::{
    AuthorizedInvocation, ControlRetryability, DispatchFailure, ExecutionUpdate,
    RecoveryObservation, ToolDispatcher, ToolExecution, ToolStart,
};
use peritus_workspace::{
    MutationOperationReference, MutationOutcome, MutationOutcomeReference,
    PreparedWorkspaceMutation, ReadOnlyWorkspace, WorkspaceAuthorizationRequest,
    WorkspaceCallerBinding, WorkspaceGateway,
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
    ReadOwned(Arc<ReadOnlyWorkspace>),
    Mutation {
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        artifacts: Option<&'a ArtifactStore>,
    },
    MutationOwned {
        gateway: Arc<Mutex<WorkspaceGateway>>,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        artifacts: Option<&'a ArtifactStore>,
    },
}

/// Descriptor-specific dispatcher whose only effect entry consumes router authority.
pub struct FsDispatcher<'a> {
    kind: FsDispatchKind,
    identity: ImplementationIdentity,
    descriptor_digest: SchemaDigest,
    context: DispatchContext<'a>,
    mutation_outcome: Option<MutationOutcome>,
    mutation_operation: Option<MutationOperationReference>,
    mutation_outcome_reference: Arc<Mutex<Option<MutationOutcomeReference>>>,
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

    /// Creates an immutable-read dispatcher which transfers work to a cancellable owner.
    ///
    /// # Errors
    /// Rejects mutation kinds or an invalid frozen descriptor catalog.
    pub fn read_owned(
        kind: FsDispatchKind,
        workspace: Arc<ReadOnlyWorkspace>,
    ) -> Result<Self, FsToolError> {
        if kind.is_mutation() {
            return Err(FsToolError::invalid(
                FsToolOperation::Catalog,
                "mutation kind cannot use an immutable dispatcher",
            ));
        }
        Self::build(kind, DispatchContext::ReadOwned(workspace))
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
        Self::build(kind, DispatchContext::Mutation { gateway, authorization, artifacts: None })
    }

    /// Creates a mutation dispatcher which durably prepares the operation and transfers its
    /// effect boundary to a cancellable owner.
    ///
    /// # Errors
    /// Rejects read kinds or an invalid frozen descriptor catalog.
    pub fn mutation_owned(
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
            DispatchContext::MutationOwned { gateway, authorization, artifacts: None },
        )
    }

    /// Creates a mutation dispatcher that can resolve exact artifact-backed final content.
    ///
    /// # Errors
    /// Rejects read kinds or an invalid frozen descriptor catalog.
    pub fn mutation_with_artifacts(
        kind: FsDispatchKind,
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        artifacts: &'a ArtifactStore,
    ) -> Result<Self, FsToolError> {
        if !kind.is_mutation() {
            return Err(FsToolError::invalid(
                FsToolOperation::Catalog,
                "read kind cannot use a mutation dispatcher",
            ));
        }
        Self::build(
            kind,
            DispatchContext::Mutation { gateway, authorization, artifacts: Some(artifacts) },
        )
    }

    /// Creates an owned mutation dispatcher that resolves artifact-backed content before handing
    /// the prepared effect to its cancellable owner.
    ///
    /// # Errors
    /// Rejects read kinds or an invalid frozen descriptor catalog.
    pub fn mutation_owned_with_artifacts(
        kind: FsDispatchKind,
        gateway: Arc<Mutex<WorkspaceGateway>>,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        artifacts: &'a ArtifactStore,
    ) -> Result<Self, FsToolError> {
        if !kind.is_mutation() {
            return Err(FsToolError::invalid(
                FsToolOperation::Catalog,
                "read kind cannot use a mutation dispatcher",
            ));
        }
        Self::build(
            kind,
            DispatchContext::MutationOwned {
                gateway,
                authorization,
                artifacts: Some(artifacts),
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
            mutation_outcome: None,
            mutation_operation: None,
            mutation_outcome_reference: Arc::new(Mutex::new(None)),
        })
    }

    /// Takes a successful C1 mutation outcome for a separately authorized candidate operation.
    #[must_use]
    pub const fn take_mutation_outcome(&mut self) -> Option<MutationOutcome> {
        self.mutation_outcome.take()
    }

    /// Returns the durable operation identity once authority has been consumed and retained.
    #[must_use]
    pub const fn mutation_operation_reference(&self) -> Option<MutationOperationReference> {
        self.mutation_operation
    }

    /// Returns the durable successful outcome identity published by either dispatch path.
    ///
    /// # Errors
    /// Returns a typed ownership failure if a prior panic poisoned the handoff lock.
    pub fn mutation_outcome_reference(
        &self,
    ) -> Result<Option<MutationOutcomeReference>, FsToolError> {
        self.mutation_outcome_reference
            .lock()
            .map(|reference| *reference)
            .map_err(|_| ownership_error("mutation outcome handoff lock is poisoned"))
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
        let caller = caller_binding(&invocation);
        if !context_matches(&self.context, &caller) {
            return Err(protocol_failure("authorized caller differs from the opened C1 target"));
        }
        let prepared = invocation.into_prepared();
        if prepared.descriptor().name().as_str() != self.kind.name()
            || prepared.descriptor_digest() != self.descriptor_digest
            || !minimum_result_capacity(&prepared, self.kind)
        {
            return Err(protocol_failure("dispatcher identity or result capacity differs"));
        }
        let rendered = match &mut self.context {
            DispatchContext::Read(workspace) => execute_read(
                self.kind,
                workspace,
                prepared.arguments(),
                selected_output_bytes(&prepared),
            ),
            DispatchContext::ReadOwned(workspace) => {
                let execution = FsExecution::read(
                    prepared,
                    started_at,
                    self.kind,
                    Arc::clone(workspace),
                );
                return Ok(ToolStart::Active(Box::new(execution)));
            }
            DispatchContext::Mutation { gateway, authorization, artifacts } => {
                let version = workspace_version(gateway);
                let compiled = compile_mutation(
                    self.kind,
                    version,
                    *artifacts,
                    prepared.arguments(),
                )
                .map_err(|error| tool_failure(&error))?;
                let operation = compiled.operation();
                let owned = gateway
                    .prepare_patch(authorization, compiled.into_patch())
                    .map_err(|error| {
                        tool_failure(&FsToolError::from_workspace(
                            FsToolErrorKind::Workspace,
                            operation,
                            &error,
                            "target-owned C1 workspace mutation preparation failed",
                        ))
                    })?;
                self.mutation_operation = Some(owned.operation_reference());
                let outcome = gateway.apply_prepared_patch(owned).map_err(|error| {
                    tool_failure(&FsToolError::from_workspace(
                        FsToolErrorKind::Workspace,
                        operation,
                        &error,
                        "target-owned C1 workspace mutation failed",
                    ))
                })?;
                let reference = outcome.reference();
                let rendered = RenderedOutput::mutation(&outcome);
                *self
                    .mutation_outcome_reference
                    .lock()
                    .map_err(|_| tool_failure(&ownership_error("mutation outcome handoff lock is poisoned")))? =
                    Some(reference);
                self.mutation_outcome = Some(outcome);
                rendered
            }
            DispatchContext::MutationOwned { gateway, authorization, artifacts } => {
                let version = {
                    let target = gateway.lock().map_err(|_| {
                        tool_failure(&ownership_error("workspace mutation owner lock is poisoned"))
                    })?;
                    workspace_version(&target)
                };
                let compiled = compile_mutation(
                    self.kind,
                    version,
                    *artifacts,
                    prepared.arguments(),
                )
                .map_err(|error| tool_failure(&error))?;
                let operation = compiled.operation();
                let owned = {
                    let mut target = gateway.lock().map_err(|_| {
                        tool_failure(&ownership_error("workspace mutation owner lock is poisoned"))
                    })?;
                    target
                        .prepare_patch(authorization, compiled.into_patch())
                        .map_err(|error| {
                            tool_failure(&FsToolError::from_workspace(
                                FsToolErrorKind::Workspace,
                                operation,
                                &error,
                                "target-owned C1 workspace mutation preparation failed",
                            ))
                        })?
                };
                self.mutation_operation = Some(owned.operation_reference());
                let execution = FsExecution::mutation(
                    prepared,
                    started_at,
                    operation,
                    Arc::clone(gateway),
                    owned,
                    Arc::clone(&self.mutation_outcome_reference),
                );
                return Ok(ToolStart::Active(Box::new(execution)));
            }
        }
        .map_err(|error| tool_failure(&error))?;
        finish(&prepared, &rendered, started_at, started_at, 0).map(ToolStart::Completed)
    }
}

enum FsExecutionWork {
    Read {
        kind: FsDispatchKind,
        workspace: Arc<ReadOnlyWorkspace>,
        arguments: peritus_tool_protocol::BoundedJson,
        maximum_output_bytes: u64,
    },
    Mutation {
        operation: FsToolOperation,
        gateway: Arc<Mutex<WorkspaceGateway>>,
        prepared: PreparedWorkspaceMutation,
        outcome: Arc<Mutex<Option<MutationOutcomeReference>>>,
    },
}

struct FsExecution {
    prepared: peritus_tool_protocol::PreparedToolCall,
    started_at: AuthorityInstant,
    work: Option<FsExecutionWork>,
    accepted: bool,
    next_sequence: u64,
    terminal: Option<ToolResult>,
}

impl FsExecution {
    fn read(
        prepared: peritus_tool_protocol::PreparedToolCall,
        started_at: AuthorityInstant,
        kind: FsDispatchKind,
        workspace: Arc<ReadOnlyWorkspace>,
    ) -> Self {
        let maximum_output_bytes = selected_output_bytes(&prepared);
        let arguments = prepared.arguments().clone();
        Self {
            prepared,
            started_at,
            work: Some(FsExecutionWork::Read {
                kind,
                workspace,
                arguments,
                maximum_output_bytes,
            }),
            accepted: false,
            next_sequence: 0,
            terminal: None,
        }
    }

    fn mutation(
        prepared: peritus_tool_protocol::PreparedToolCall,
        started_at: AuthorityInstant,
        operation: FsToolOperation,
        gateway: Arc<Mutex<WorkspaceGateway>>,
        mutation: PreparedWorkspaceMutation,
        outcome: Arc<Mutex<Option<MutationOutcomeReference>>>,
    ) -> Self {
        Self {
            prepared,
            started_at,
            work: Some(FsExecutionWork::Mutation {
                operation,
                gateway,
                prepared: mutation,
                outcome,
            }),
            accepted: false,
            next_sequence: 0,
            terminal: None,
        }
    }

    fn observe_time(&self, observed_at: AuthorityInstant) -> Result<(), DispatchFailure> {
        if observed_at.epoch() != self.started_at.epoch()
            || observed_at.tick_millis() < self.started_at.tick_millis()
        {
            return Err(protocol_failure(
                "filesystem execution observation regresses or crosses authority epochs",
            ));
        }
        Ok(())
    }

    fn progress(
        &self,
        kind: ProgressKind,
        observed_at: AuthorityInstant,
        detail: &'static str,
    ) -> Result<ToolProgress, DispatchFailure> {
        ToolProgress::new(
            &self.prepared,
            self.next_sequence,
            kind,
            observed_at,
            None,
            bounded(detail),
        )
        .map_err(|_| protocol_failure("filesystem progress envelope is invalid"))
    }

    fn poll_owned(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.observe_time(observed_at)?;
        if let Some(terminal) = &self.terminal {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(terminal.clone()))
                .map_err(|_| protocol_failure("filesystem terminal replay is invalid"));
        }
        if !self.accepted {
            let progress = self.progress(
                ProgressKind::Started,
                observed_at,
                "filesystem execution accepted by its target owner",
            )?;
            self.accepted = true;
            self.next_sequence += 1;
            return ExecutionUpdate::new(&self.prepared, vec![progress], None)
                .map_err(|_| protocol_failure("filesystem start progress is invalid"));
        }

        let work = self.work.take().ok_or_else(|| {
            protocol_failure("filesystem execution lost its retained phase ownership")
        })?;
        let rendered = match work {
            FsExecutionWork::Read {
                kind,
                workspace,
                arguments,
                maximum_output_bytes,
            } => execute_read(kind, &workspace, &arguments, maximum_output_bytes),
            FsExecutionWork::Mutation { operation, gateway, prepared, outcome } => {
                let result = match gateway.lock() {
                    Ok(mut target) => target.apply_prepared_patch(prepared).map_err(|error| {
                        FsToolError::from_workspace(
                            FsToolErrorKind::Workspace,
                            operation,
                            &error,
                            "target-owned C1 workspace mutation failed",
                        )
                    }),
                    Err(_) => Err(ownership_error("workspace mutation owner lock is poisoned")),
                };
                result.and_then(|mutation| {
                    let reference = mutation.reference();
                    *outcome
                        .lock()
                        .map_err(|_| ownership_error("mutation outcome handoff lock is poisoned"))? =
                        Some(reference);
                    RenderedOutput::mutation_reference(reference)
                })
            }
        };

        let progress = self.progress(
            ProgressKind::Update,
            observed_at,
            if rendered.is_ok() {
                "filesystem execution completed"
            } else {
                "filesystem execution reached a typed terminal failure"
            },
        )?;
        self.next_sequence += 1;
        let terminal = match rendered {
            Ok(rendered) => finish(
                &self.prepared,
                &rendered,
                self.started_at,
                observed_at,
                self.next_sequence,
            )?,
            Err(error) => terminal_failure(
                &self.prepared,
                self.started_at,
                observed_at,
                &tool_failure(&error),
                self.next_sequence,
            )?,
        };
        self.terminal = Some(terminal.clone());
        ExecutionUpdate::new(&self.prepared, vec![progress], Some(terminal))
            .map_err(|_| protocol_failure("filesystem terminal update is invalid"))
    }
}

impl ToolExecution for FsExecution {
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll_owned(observed_at)
    }

    fn control(
        &mut self,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.observe_time(observed_at)
            .map_err(|error| error.rejecting_control(ControlRetryability::CorrectRequest))?;
        match control {
            ToolControl::Poll => self.poll_owned(observed_at),
            ToolControl::Cancel(reason) => self.cancel(reason, observed_at),
            ToolControl::Stdin(_) | ToolControl::Resize { .. } | ToolControl::Signal(_) => {
                Err(protocol_failure("filesystem execution supports only poll and cancel")
                    .rejecting_control(ControlRetryability::CorrectRequest))
            }
        }
    }

    fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.observe_time(observed_at)
            .map_err(|error| error.rejecting_control(ControlRetryability::CorrectRequest))?;
        if let Some(terminal) = &self.terminal {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(terminal.clone()))
                .map_err(|_| protocol_failure("filesystem terminal replay is invalid"));
        }
        self.work = None;
        let progress = self.progress(
            ProgressKind::Stopping,
            observed_at,
            "filesystem execution cancelled before its effect boundary",
        )?;
        self.next_sequence += 1;
        let failure = cancellation_failure(reason);
        let terminal = terminal_failure(
            &self.prepared,
            self.started_at,
            observed_at,
            &failure,
            self.next_sequence,
        )?;
        self.terminal = Some(terminal.clone());
        ExecutionUpdate::new(&self.prepared, vec![progress], Some(terminal))
            .map_err(|_| protocol_failure("filesystem cancellation update is invalid"))
    }

    fn recover(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure> {
        let update = self.poll_owned(observed_at)?;
        if update.terminal().is_some() {
            Ok(RecoveryObservation::Completed(update))
        } else {
            Ok(RecoveryObservation::Active(update))
        }
    }
}

fn execute_read(
    kind: FsDispatchKind,
    workspace: &ReadOnlyWorkspace,
    arguments: &peritus_tool_protocol::BoundedJson,
    maximum_output_bytes: u64,
) -> Result<RenderedOutput, FsToolError> {
    let service = FsReadService::new(workspace);
    let maximum_output_bytes = usize::try_from(maximum_output_bytes).unwrap_or(usize::MAX);
    match kind {
        FsDispatchKind::Discover => render_discover(
            &service,
            decoder::discover(arguments)?,
            maximum_output_bytes,
        ),
        FsDispatchKind::Metadata => {
            let rendered =
                RenderedOutput::metadata(&service.metadata(&decoder::metadata(arguments)?)?)?;
            if rendered.encoded_bytes() <= maximum_output_bytes {
                Ok(rendered)
            } else {
                RenderedOutput::deferred("fs.metadata", None, rendered.encoded_bytes())
            }
        }
        FsDispatchKind::Read => {
            render_file(&service, decoder::read(arguments)?, maximum_output_bytes)
        }
        FsDispatchKind::Search => render_search(
            &service,
            decoder::search(arguments)?,
            maximum_output_bytes,
        ),
        _ => Err(FsToolError::invalid(
            FsToolOperation::Catalog,
            "mutation kind reached immutable dispatcher",
        )),
    }
}

fn selected_output_bytes(prepared: &peritus_tool_protocol::PreparedToolCall) -> u64 {
    let json_bytes = u64::try_from(prepared.call().limits().json_limits().max_bytes())
        .unwrap_or(u64::MAX);
    prepared.call().limits().output_limit().map_or(json_bytes, |output| output.min(json_bytes))
}

fn render_discover(
    service: &FsReadService<'_>,
    mut input: crate::DiscoverInput,
    maximum_output_bytes: usize,
) -> Result<RenderedOutput, FsToolError> {
    loop {
        let observation = service.discover(&input)?;
        let rendered = RenderedOutput::discover(&observation);
        match rendered {
            Ok(rendered) if rendered.encoded_bytes() <= maximum_output_bytes => return Ok(rendered),
            rendered => {
                if input.narrow_result_page() {
                    continue;
                }
                return match rendered {
                    Ok(rendered) => deferred_result(
                        "fs.discover",
                        Some(observation.cursor()),
                        rendered.encoded_bytes(),
                        maximum_output_bytes,
                    ),
                    Err(error) => Err(error),
                };
            }
        }
    }
}

fn render_file(
    service: &FsReadService<'_>,
    mut input: crate::ReadInput,
    maximum_output_bytes: usize,
) -> Result<RenderedOutput, FsToolError> {
    loop {
        let observation = service.read(&input)?;
        let rendered = RenderedOutput::file(&observation);
        match rendered {
            Ok(rendered) if rendered.encoded_bytes() <= maximum_output_bytes => return Ok(rendered),
            rendered => {
                if input.narrow_result_page() {
                    continue;
                }
                return match rendered {
                    Ok(rendered) => deferred_result(
                        "fs.read",
                        Some(observation.cursor()),
                        rendered.encoded_bytes(),
                        maximum_output_bytes,
                    ),
                    Err(error) => Err(error),
                };
            }
        }
    }
}

fn render_search(
    service: &FsReadService<'_>,
    mut input: crate::SearchInput,
    maximum_output_bytes: usize,
) -> Result<RenderedOutput, FsToolError> {
    loop {
        let observation = service.search(&input)?;
        let rendered = RenderedOutput::search(&observation);
        match rendered {
            Ok(rendered) if rendered.encoded_bytes() <= maximum_output_bytes => return Ok(rendered),
            rendered => {
                if input.narrow_result_page() {
                    continue;
                }
                return match rendered {
                    Ok(rendered) => deferred_result(
                        "fs.search",
                        Some(observation.cursor()),
                        rendered.encoded_bytes(),
                        maximum_output_bytes,
                    ),
                    Err(error) => Err(error),
                };
            }
        }
    }
}

fn deferred_result(
    operation: &'static str,
    cursor: Option<&str>,
    minimum_output_bytes: usize,
    maximum_output_bytes: usize,
) -> Result<RenderedOutput, FsToolError> {
    let with_cursor = RenderedOutput::deferred(operation, cursor, minimum_output_bytes)?;
    if with_cursor.encoded_bytes() <= maximum_output_bytes {
        Ok(with_cursor)
    } else {
        RenderedOutput::deferred(operation, None, minimum_output_bytes)
    }
}

fn workspace_version(gateway: &WorkspaceGateway) -> WorkspaceVersion {
    let state = gateway.state();
    WorkspaceVersion::new(state.binding().workspace_id(), state.generation(), state.revision())
}

fn compile_mutation(
    kind: FsDispatchKind,
    version: WorkspaceVersion,
    artifacts: Option<&ArtifactStore>,
    arguments: &peritus_tool_protocol::BoundedJson,
) -> Result<CompiledMutation, FsToolError> {
    let compiled = match kind {
        FsDispatchKind::Create => CompiledMutation::create_with_artifacts(
            version,
            decoder::create(arguments)?,
            artifacts,
        ),
        FsDispatchKind::Patch => CompiledMutation::patch_with_artifacts(
            version,
            decoder::patch(arguments)?,
            artifacts,
        ),
        FsDispatchKind::Remove => CompiledMutation::remove(version, decoder::remove(arguments)?),
        FsDispatchKind::Replace => CompiledMutation::replace_with_artifacts(
            version,
            decoder::replace(arguments)?,
            artifacts,
        ),
        FsDispatchKind::Write => CompiledMutation::write_with_artifacts(
            version,
            decoder::write(arguments)?,
            artifacts,
        ),
        _ => Err(FsToolError::invalid(
            FsToolOperation::Catalog,
            "read kind reached mutation dispatcher",
        )),
    }?;
    Ok(compiled)
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
        DispatchContext::Read(workspace) => read_context_matches(workspace, caller),
        DispatchContext::ReadOwned(workspace) => read_context_matches(workspace, caller),
        DispatchContext::Mutation { authorization, .. }
        | DispatchContext::MutationOwned { authorization, .. } => {
            authorization.caller_binding() == Some(caller)
        }
    }
}

fn read_context_matches(workspace: &ReadOnlyWorkspace, caller: &WorkspaceCallerBinding) -> bool {
    workspace.target_binding().is_some_and(|target| {
        target.workspace_id() == caller.workspace_id()
            && target.environment_id() == caller.environment_id()
            && target.resource_id() == caller.resource_id()
    })
}

fn finish(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    rendered: &RenderedOutput,
    started_at: AuthorityInstant,
    completed_at: AuthorityInstant,
    progress_count: u64,
) -> Result<ToolResult, DispatchFailure> {
    if rendered.structured().canonical_bytes().len() as u64
        > prepared.call().limits().output_bytes()
    {
        return Err(protocol_failure("structured result exceeds the selected call output bound"));
    }
    let timing = ToolTiming::new(started_at, completed_at)
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
        progress_count,
    )
    .map_err(|_| protocol_failure("terminal filesystem result is invalid"))
}

fn terminal_failure(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    started_at: AuthorityInstant,
    completed_at: AuthorityInstant,
    failure: &DispatchFailure,
    progress_count: u64,
) -> Result<ToolResult, DispatchFailure> {
    let timing = ToolTiming::new(started_at, completed_at)
        .map_err(|_| protocol_failure("filesystem failure timing is invalid"))?;
    ToolResult::failure(
        prepared,
        failure.status(),
        failure.failure().clone(),
        None,
        failure.failure().detail().clone(),
        failure.failure().detail().clone(),
        Vec::new(),
        timing,
        TruncationMetadata {
            output: Truncation::Complete,
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        progress_count,
    )
    .map_err(|_| protocol_failure("terminal filesystem failure is invalid"))
}

const fn minimum_result_capacity(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    kind: FsDispatchKind,
) -> bool {
    let limits = prepared.call().limits();
    let minimum_output = if kind.is_mutation() { 2_048 } else { 512 };
    limits.output_bytes() >= minimum_output
        && limits.model_bytes() >= 128
        && limits.human_bytes() >= 128
        && limits.progress_events() >= 2
}

fn tool_failure(error: &FsToolError) -> DispatchFailure {
    let category = match error.kind() {
        FsToolErrorKind::Inspection | FsToolErrorKind::Patch | FsToolErrorKind::Workspace => {
            FailureCategory::Workspace
        }
        FsToolErrorKind::Unsupported => FailureCategory::Infrastructure,
        FsToolErrorKind::InvalidInput | FsToolErrorKind::Protocol => FailureCategory::Protocol,
    };
    failure(category, error.code(), error.detail(), error.recovery())
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
    let (status, retryability, route) = match recovery {
        RecoveryClass::CorrectInput | RecoveryClass::SelectSupportedOperation => {
            (ResultStatus::Failed, Retryability::NewAction, RecoveryRoute::None)
        }
        RecoveryClass::Reauthorize => {
            (ResultStatus::Failed, Retryability::NewAction, RecoveryRoute::Reauthorize)
        }
        RecoveryClass::Reobserve => {
            (
                ResultStatus::Indeterminate,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileWorkspace,
            )
        }
        RecoveryClass::Reconcile => {
            (
                ResultStatus::Indeterminate,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileWorkspace,
            )
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
    DispatchFailure::new(status, failure)
        .expect("non-success static dispatch failure is valid")
}

fn cancellation_failure(reason: CancellationReason) -> DispatchFailure {
    let (status, category, code, detail) = match reason {
        CancellationReason::Deadline => (
            ResultStatus::TimedOut,
            FailureCategory::Timeout,
            "PERITUS-FS-TOOL-DEADLINE",
            "filesystem execution deadline elapsed before its effect boundary",
        ),
        CancellationReason::Requested | CancellationReason::Shutdown | CancellationReason::Recovery => (
            ResultStatus::Cancelled,
            FailureCategory::Cancelled,
            "PERITUS-FS-TOOL-CANCELLED",
            "filesystem execution was cancelled before its effect boundary",
        ),
    };
    DispatchFailure::new(
        status,
        ToolFailure::new(
            category,
            bounded(code),
            ResponsibleSubsystem::Tool,
            Retryability::Never,
            RecoveryRoute::None,
            bounded(detail),
        ),
    )
    .expect("filesystem cancellation is a non-success result")
}

const fn ownership_error(detail: &'static str) -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::Workspace,
        FsToolOperation::Catalog,
        RecoveryClass::Reconcile,
        detail,
    )
}

fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value.to_owned()).expect("static filesystem failure text is bounded")
}
