//! Router-authorized Git dispatcher adapters.

use std::sync::Arc;

use peritus_artifact_store::ArtifactStore;
use peritus_git::CandidateSnapshot;
use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    BoundedText, CancellationReason, ImplementationIdentity, ProgressKind, SchemaDigest,
    ToolControl, ToolProgress,
};
use peritus_tool_router::{
    AuthorizedInvocation, ControlRetryability, DispatchFailure, ExecutionUpdate,
    RecoveryObservation, ToolDispatcher, ToolExecution, ToolStart,
};
use peritus_types::Sha256Digest;
use peritus_workspace::{
    CandidateOutcome, MutationOutcome, MutationOutcomeReference, ReadOnlyWorkspace,
    RepositoryMutationKind, RepositoryMutationOperationReference,
    RepositoryMutationOutcomeReference, RollbackOutcome, RollbackRequest,
    WorkspaceAuthorizationRequest, WorkspaceCallerBinding, WorkspaceGateway,
};

use crate::{
    GitReadService, GitToolError, GitToolErrorKind, GitToolOperation, RecoveryClass,
    RenderedOutput, SnapshotInput,
    decoder, descriptor_catalog, legacy_merge_descriptor,
    dispatch_support::{
        caller_binding, cancellation_failure, finish, minimum_result_capacity, protocol_failure,
        terminal_failure, tool_failure, unsupported_failure, workspace_failure,
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
    ReadOwned {
        workspace: Arc<ReadOnlyWorkspace>,
        retained: Option<CandidateSnapshot>,
    },
    Candidate {
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        mutation: &'a MutationOutcome,
        artifacts: &'a ArtifactStore,
    },
    CandidateReference {
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        mutation: MutationOutcomeReference,
        artifacts: &'a ArtifactStore,
    },
    Rollback {
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        target: &'a CandidateSnapshot,
        artifacts: &'a ArtifactStore,
    },
    Adopted {
        outcome: Box<RepositoryMutationOutcomeReference>,
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
    repository_operation: Option<RepositoryMutationOperationReference>,
    repository_outcome: Option<RepositoryMutationOutcomeReference>,
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

    /// Creates an owned status, diff, history, or snapshot dispatcher with observable progress,
    /// cancellation before observation, and completion time supplied by the router clock.
    ///
    /// # Errors
    /// Rejects an effectful kind or invalid frozen descriptor catalog.
    pub fn read_owned(
        kind: GitDispatchKind,
        workspace: Arc<ReadOnlyWorkspace>,
        retained: Option<CandidateSnapshot>,
    ) -> Result<Self, GitToolError> {
        if matches!(
            kind,
            GitDispatchKind::Candidate | GitDispatchKind::Rollback | GitDispatchKind::Merge
        ) {
            return Err(GitToolError::invalid(
                GitToolOperation::Catalog,
                "effectful Git kind cannot use an owned observation dispatcher",
            ));
        }
        Self::build(kind, DispatchContext::ReadOwned { workspace, retained })
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

    /// Creates the authorized candidate dispatcher from a restart-visible mutation handoff.
    ///
    /// # Errors
    /// Returns a typed frozen-catalog construction failure.
    pub fn candidate_reference(
        gateway: &'a mut WorkspaceGateway,
        authorization: &'a WorkspaceAuthorizationRequest<'a>,
        mutation: MutationOutcomeReference,
        artifacts: &'a ArtifactStore,
    ) -> Result<Self, GitToolError> {
        Self::build(
            GitDispatchKind::Candidate,
            DispatchContext::CandidateReference {
                gateway,
                authorization,
                mutation,
                artifacts,
            },
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

    /// Creates a no-effect dispatcher for one previously completed durable repository mutation.
    ///
    /// # Errors
    /// Rejects a read/merge kind, kind mismatch, or invalid frozen descriptor catalog.
    pub fn adopted(
        kind: GitDispatchKind,
        outcome: RepositoryMutationOutcomeReference,
    ) -> Result<Self, GitToolError> {
        let expected = match kind {
            GitDispatchKind::Candidate => RepositoryMutationKind::Candidate,
            GitDispatchKind::Rollback => RepositoryMutationKind::Rollback,
            _ => {
                return Err(GitToolError::invalid(
                    GitToolOperation::Catalog,
                    "only completed candidate or rollback results can be adopted",
                ));
            }
        };
        if outcome.operation().kind() != expected {
            return Err(GitToolError::invalid(
                GitToolOperation::Catalog,
                "adopted repository result kind differs from the dispatcher",
            ));
        }
        Self::build(kind, DispatchContext::Adopted { outcome: Box::new(outcome) })
    }

    /// Creates the authorized-but-unsupported merge dispatcher with no target mutation handle.
    ///
    /// # Errors
    /// Returns a typed frozen-catalog construction failure.
    pub fn merge_unsupported() -> Result<Self, GitToolError> {
        Self::build(GitDispatchKind::Merge, DispatchContext::MergeUnsupported)
    }

    fn build(kind: GitDispatchKind, context: DispatchContext<'a>) -> Result<Self, GitToolError> {
        let descriptor = if kind == GitDispatchKind::Merge {
            legacy_merge_descriptor()?
        } else {
            descriptor_catalog()?
                .into_iter()
                .find(|descriptor| descriptor.name().as_str() == kind.name())
                .ok_or_else(|| {
                    GitToolError::invalid(
                        GitToolOperation::Catalog,
                        "dispatcher descriptor is absent",
                    )
                })?
        };
        Ok(Self {
            kind,
            identity: descriptor.implementation_identity().clone(),
            descriptor_digest: descriptor.descriptor_digest(),
            context,
            mutation_outcome: None,
            repository_operation: None,
            repository_outcome: None,
        })
    }

    /// Takes a successful C1 Git mutation outcome after router dispatch.
    #[must_use]
    pub const fn take_mutation_outcome(&mut self) -> Option<GitMutationOutcome> {
        self.mutation_outcome.take()
    }

    /// Borrows the durable operation after authority has been consumed and before any Git effect.
    #[must_use]
    pub const fn repository_operation_reference(
        &self,
    ) -> Option<&RepositoryMutationOperationReference> {
        self.repository_operation.as_ref()
    }

    /// Borrows the durable exact outcome after successful repository publication.
    #[must_use]
    pub const fn repository_outcome_reference(
        &self,
    ) -> Option<&RepositoryMutationOutcomeReference> {
        self.repository_outcome.as_ref()
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
        let started_at = invocation.observed_at();
        let caller = caller_binding(&invocation);
        if !context_matches(&self.context, &caller) {
            return Err(protocol_failure("authorized caller differs from the opened C1 target"));
        }
        let prepared = invocation.into_prepared();
        let owned_observation = matches!(&self.context, DispatchContext::ReadOwned { .. });
        if prepared.descriptor().name().as_str() != self.kind.name()
            || prepared.descriptor_digest() != self.descriptor_digest
            || !minimum_result_capacity(&prepared, owned_observation)
        {
            return Err(protocol_failure("dispatcher identity or result capacity differs"));
        }
        let rendered = match &mut self.context {
            DispatchContext::Read { workspace, retained } => {
                execute_read(
                    self.kind,
                    workspace,
                    *retained,
                    prepared.arguments(),
                    selected_output_bytes(&prepared),
                )
            }
            DispatchContext::ReadOwned { workspace, retained } => {
                let execution = GitObservationExecution::new(
                    prepared,
                    started_at,
                    self.kind,
                    Arc::clone(workspace),
                    retained.clone(),
                );
                return Ok(ToolStart::Active(Box::new(execution)));
            }
            DispatchContext::Candidate { gateway, authorization, mutation, artifacts } => {
                let input = decoder::candidate(prepared.arguments())
                    .map_err(|error| tool_failure(&error))?;
                admit_mutation_result(&prepared, started_at)?;
                let mutation = gateway
                    .prepare_candidate(authorization, mutation, input.snapshot_id())
                    .map_err(|error| workspace_failure(&error))?;
                self.repository_operation = Some(mutation.operation_reference().clone());
                let terminal = mutation_terminal(
                    &prepared,
                    mutation.operation_reference(),
                    started_at,
                )?;
                let outcome = gateway
                    .create_prepared_candidate(mutation, artifacts)
                    .map_err(|error| workspace_failure(&error))?;
                self.repository_outcome = Some(outcome.receipt().clone());
                self.mutation_outcome = Some(GitMutationOutcome::Candidate(outcome));
                return Ok(ToolStart::Completed(terminal));
            }
            DispatchContext::CandidateReference {
                gateway,
                authorization,
                mutation,
                artifacts,
            } => {
                let input = decoder::candidate(prepared.arguments())
                    .map_err(|error| tool_failure(&error))?;
                admit_mutation_result(&prepared, started_at)?;
                let mutation = gateway
                    .prepare_candidate_from_reference(
                        authorization,
                        *mutation,
                        input.snapshot_id(),
                    )
                    .map_err(|error| workspace_failure(&error))?;
                self.repository_operation = Some(mutation.operation_reference().clone());
                let terminal = mutation_terminal(
                    &prepared,
                    mutation.operation_reference(),
                    started_at,
                )?;
                let outcome = gateway
                    .create_prepared_candidate(mutation, artifacts)
                    .map_err(|error| workspace_failure(&error))?;
                self.repository_outcome = Some(outcome.receipt().clone());
                self.mutation_outcome = Some(GitMutationOutcome::Candidate(outcome));
                return Ok(ToolStart::Completed(terminal));
            }
            DispatchContext::Rollback { gateway, authorization, target, artifacts } => {
                let input = decoder::rollback(prepared.arguments())
                    .map_err(|error| tool_failure(&error))?;
                if input.target_snapshot_id() != target.snapshot_id() {
                    return Err(protocol_failure("rollback target differs from prepared input"));
                }
                admit_mutation_result(&prepared, started_at)?;
                let mutation = gateway
                    .prepare_rollback(
                        authorization,
                        RollbackRequest::new(target, input.successor_snapshot_id()),
                    )
                    .map_err(|error| workspace_failure(&error))?;
                self.repository_operation = Some(mutation.operation_reference().clone());
                let terminal = mutation_terminal(
                    &prepared,
                    mutation.operation_reference(),
                    started_at,
                )?;
                let outcome = gateway
                    .apply_prepared_rollback(mutation, artifacts)
                    .map_err(|error| workspace_failure(&error))?;
                self.repository_outcome = Some(outcome.receipt().clone());
                self.mutation_outcome = Some(GitMutationOutcome::Rollback(outcome));
                return Ok(ToolStart::Completed(terminal));
            }
            DispatchContext::Adopted { outcome } => {
                let terminal = mutation_terminal(&prepared, outcome.operation(), started_at)?;
                self.repository_operation = Some(outcome.operation().clone());
                self.repository_outcome = Some((**outcome).clone());
                return Ok(ToolStart::Completed(terminal));
            }
            DispatchContext::MergeUnsupported => return Err(unsupported_failure()),
        }
        .map_err(|error| tool_failure(&error))?;
        finish(&prepared, &rendered, started_at, started_at, 0).map(ToolStart::Completed)
    }
}

fn admit_mutation_result(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    completed_at: peritus_policy::AuthorityInstant,
) -> Result<(), DispatchFailure> {
    let rendered = RenderedOutput::mutation_receipt(Sha256Digest::new([0_u8; 32]))
        .map_err(|error| tool_failure(&error))?;
    finish(prepared, &rendered, completed_at, completed_at, 0).map(drop)
}

fn mutation_terminal(
    prepared: &peritus_tool_protocol::PreparedToolCall,
    operation: &RepositoryMutationOperationReference,
    completed_at: peritus_policy::AuthorityInstant,
) -> Result<peritus_tool_protocol::ToolResult, DispatchFailure> {
    if operation.action_id() != prepared.call().action_id()
        || operation.descriptor_digest() != Some(prepared.descriptor_digest().get())
        || operation.prepared_digest() != Some(prepared.prepared_digest())
    {
        return Err(protocol_failure(
            "durable repository operation differs from the prepared invocation",
        ));
    }
    let rendered = RenderedOutput::mutation_receipt(operation.digest())
        .map_err(|error| tool_failure(&error))?;
    finish(prepared, &rendered, completed_at, completed_at, 0)
}

struct GitObservationWork {
    kind: GitDispatchKind,
    workspace: Arc<ReadOnlyWorkspace>,
    retained: Option<CandidateSnapshot>,
    arguments: peritus_tool_protocol::BoundedJson,
    maximum_output_bytes: u64,
}

struct GitObservationExecution {
    prepared: peritus_tool_protocol::PreparedToolCall,
    started_at: AuthorityInstant,
    work: Option<GitObservationWork>,
    accepted: bool,
    next_sequence: u64,
    terminal: Option<peritus_tool_protocol::ToolResult>,
}

impl GitObservationExecution {
    fn new(
        prepared: peritus_tool_protocol::PreparedToolCall,
        started_at: AuthorityInstant,
        kind: GitDispatchKind,
        workspace: Arc<ReadOnlyWorkspace>,
        retained: Option<CandidateSnapshot>,
    ) -> Self {
        let arguments = prepared.arguments().clone();
        let maximum_output_bytes = selected_output_bytes(&prepared);
        Self {
            prepared,
            started_at,
            work: Some(GitObservationWork {
                kind,
                workspace,
                retained,
                arguments,
                maximum_output_bytes,
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
                "Git observation time regresses or crosses authority epochs",
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
            BoundedText::new(detail.to_owned()).expect("static Git progress text is bounded"),
        )
        .map_err(|_| protocol_failure("Git observation progress envelope is invalid"))
    }

    fn poll_owned(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.observe_time(observed_at)?;
        if let Some(terminal) = &self.terminal {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(terminal.clone()))
                .map_err(|_| protocol_failure("Git observation terminal replay is invalid"));
        }
        if !self.accepted {
            let progress = self.progress(
                ProgressKind::Started,
                observed_at,
                "immutable Git observation accepted by its target owner",
            )?;
            self.accepted = true;
            self.next_sequence += 1;
            return ExecutionUpdate::new(&self.prepared, vec![progress], None)
                .map_err(|_| protocol_failure("Git observation start progress is invalid"));
        }

        let work = self.work.take().ok_or_else(|| {
            protocol_failure("Git observation lost its retained phase ownership")
        })?;
        let rendered = execute_read(
            work.kind,
            &work.workspace,
            work.retained.as_ref(),
            &work.arguments,
            work.maximum_output_bytes,
        );
        let progress = self.progress(
            ProgressKind::Update,
            observed_at,
            if rendered.is_ok() {
                "immutable Git observation completed"
            } else {
                "immutable Git observation reached a typed terminal failure"
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
            .map_err(|_| protocol_failure("Git observation terminal update is invalid"))
    }
}

impl ToolExecution for GitObservationExecution {
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
                Err(protocol_failure("Git observation supports only poll and cancel")
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
                .map_err(|_| protocol_failure("Git observation terminal replay is invalid"));
        }
        self.work = None;
        let progress = self.progress(
            ProgressKind::Stopping,
            observed_at,
            "immutable Git observation cancelled before execution",
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
            .map_err(|_| protocol_failure("Git observation cancellation update is invalid"))
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
    kind: GitDispatchKind,
    workspace: &ReadOnlyWorkspace,
    retained: Option<&CandidateSnapshot>,
    arguments: &peritus_tool_protocol::BoundedJson,
    maximum_output_bytes: u64,
) -> Result<RenderedOutput, GitToolError> {
    let service = GitReadService::new(workspace);
    let maximum_output_bytes = usize::try_from(maximum_output_bytes).unwrap_or(usize::MAX);
    match kind {
        GitDispatchKind::Status => render_status_page(
            service.status_page(decoder::status(arguments)?)?,
            maximum_output_bytes,
        ),
        GitDispatchKind::Diff => render_diff_page(
            service.diff_page(&decoder::diff(arguments)?)?,
            maximum_output_bytes,
        ),
        GitDispatchKind::History => render_history_page(
            service.history_page(decoder::history(arguments)?)?,
            maximum_output_bytes,
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

fn selected_output_bytes(prepared: &peritus_tool_protocol::PreparedToolCall) -> u64 {
    let json_bytes = u64::try_from(prepared.call().limits().json_limits().max_bytes())
        .unwrap_or(u64::MAX);
    prepared.call().limits().output_limit().map_or(json_bytes, |output| output.min(json_bytes))
}

fn render_status_page(
    mut page: crate::StatusObservationPage,
    maximum_output_bytes: usize,
) -> Result<RenderedOutput, GitToolError> {
    loop {
        let rendered = RenderedOutput::status_page(&page);
        match rendered {
            Ok(rendered) if rendered.encoded_bytes() <= maximum_output_bytes => {
                return Ok(rendered);
            }
            rendered => {
                if page.narrow_result_page() {
                    continue;
                }
                return final_or_deferred(
                    "git.status",
                    page.cursor(),
                    rendered,
                    maximum_output_bytes,
                );
            }
        }
    }
}

fn render_diff_page(
    mut page: crate::DiffObservationPage,
    maximum_output_bytes: usize,
) -> Result<RenderedOutput, GitToolError> {
    loop {
        let rendered = RenderedOutput::diff_page(&page);
        match rendered {
            Ok(rendered) if rendered.encoded_bytes() <= maximum_output_bytes => {
                return Ok(rendered);
            }
            rendered => {
                if page.narrow_result_page() {
                    continue;
                }
                return final_or_deferred(
                    "git.diff",
                    page.cursor(),
                    rendered,
                    maximum_output_bytes,
                );
            }
        }
    }
}

fn render_history_page(
    mut page: crate::HistoryObservationPage,
    maximum_output_bytes: usize,
) -> Result<RenderedOutput, GitToolError> {
    loop {
        let rendered = RenderedOutput::history_page(&page);
        match rendered {
            Ok(rendered) if rendered.encoded_bytes() <= maximum_output_bytes => {
                return Ok(rendered);
            }
            rendered => {
                if page.narrow_result_page() {
                    continue;
                }
                return final_or_deferred(
                    "git.history",
                    page.cursor(),
                    rendered,
                    maximum_output_bytes,
                );
            }
        }
    }
}

fn final_or_deferred(
    operation: &'static str,
    cursor: &str,
    rendered: Result<RenderedOutput, GitToolError>,
    maximum_output_bytes: usize,
) -> Result<RenderedOutput, GitToolError> {
    let rendered = rendered?;
    let minimum_output_bytes = rendered.encoded_bytes();
    let deferred = RenderedOutput::deferred(operation, cursor, minimum_output_bytes)?;
    if deferred.encoded_bytes() <= maximum_output_bytes {
        Ok(deferred)
    } else {
        Err(GitToolError::new(
            GitToolErrorKind::Protocol,
            GitToolOperation::Catalog,
            RecoveryClass::CorrectInput,
            "selected output capacity cannot carry a Git continuation",
        ))
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
        DispatchContext::ReadOwned { workspace, .. } => {
            workspace.target_binding().is_some_and(|target| {
                target.workspace_id() == caller.workspace_id()
                    && target.environment_id() == caller.environment_id()
                    && target.resource_id() == caller.resource_id()
            })
        }
        DispatchContext::Candidate { authorization, .. }
        | DispatchContext::CandidateReference { authorization, .. }
        | DispatchContext::Rollback { authorization, .. } => {
            authorization.caller_binding() == Some(caller)
        }
        DispatchContext::Adopted { outcome } => {
            let operation = outcome.operation();
            operation.action_id() == caller.action_id()
                && operation.workspace_id() == caller.workspace_id()
                && operation.resource_id() == caller.resource_id()
                && operation.environment_id() == Some(caller.environment_id())
                && operation.descriptor_digest() == Some(caller.descriptor_digest())
                && operation.prepared_digest() == Some(caller.prepared_digest())
        }
        DispatchContext::MergeUnsupported => true,
    }
}
