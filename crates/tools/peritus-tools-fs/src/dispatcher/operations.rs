use std::sync::{Arc, Mutex};

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    ArtifactCompleteness, BoundedJson, BoundedText, FailureCategory, PreparedToolCall,
    RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability, ToolFailure, ToolResult,
    ToolTiming, Truncation, TruncationMetadata,
};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure};
use peritus_workspace::{
    ReadOnlyWorkspace, RecoveryClass as WorkspaceRecoveryClass, WorkspaceCallerBinding,
    WorkspaceError, WorkspaceGateway,
};

use super::{ArtifactInputResolver, DispatchContext, FsDispatchKind};
use crate::{
    ArtifactInputFailure, CompiledMutation, FsReadService, FsToolError, FsToolErrorKind,
    FsToolOperation, MutationContent, RecoveryClass, RenderedOutput, WorkspaceVersion, decoder,
};
use peritus_patch::PatchSet;
use peritus_workspace::ErrorCode as WorkspaceErrorCode;

pub(super) fn execute_read_cancellable(
    kind: FsDispatchKind,
    workspace: &ReadOnlyWorkspace,
    arguments: &BoundedJson,
    output_bytes: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<RenderedOutput>, FsToolError> {
    let service = FsReadService::new(workspace);
    match kind {
        FsDispatchKind::Read => {
            let input = decoder::read(arguments)?;
            let mut page_size = input.maximum_bytes.min(output_bytes.max(1));
            loop {
                if cancelled() {
                    return Ok(None);
                }
                let page = crate::ReadInput::range(input.path.as_str(), input.offset, page_size)?;
                let Some(observation) = service.read_cancellable(&page, cancelled)? else {
                    return Ok(None);
                };
                match RenderedOutput::file_with_output_bound(&observation, output_bytes) {
                    Ok(rendered) => return Ok(Some(rendered)),
                    Err(error) if error.kind() == FsToolErrorKind::Protocol && page_size > 1 => {
                        page_size = (page_size / 2).max(1);
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        FsDispatchKind::Discover => {
            let input = decoder::discover(arguments)?;
            service
                .discover_cancellable(&input, cancelled)?
                .map(|observation| {
                    RenderedOutput::discover_page_with_path_range(
                        &observation,
                        input.continuation_offset,
                        input.maximum_entries,
                        input.path_offset,
                        output_bytes,
                    )
                })
                .transpose()
        }
        FsDispatchKind::Search => {
            let input = decoder::search(arguments)?;
            service
                .search_cancellable(&input, cancelled)?
                .map(|observation| {
                    RenderedOutput::search_page_with_field_ranges(
                        &observation,
                        input.continuation_offset,
                        input.maximum_matches,
                        input.omission_offset,
                        input.match_field.map(|field| (field, input.match_field_offset)),
                        input.omission_path_offset,
                        output_bytes,
                    )
                })
                .transpose()
        }
        FsDispatchKind::Metadata => {
            if cancelled() {
                Ok(None)
            } else {
                RenderedOutput::metadata_with_output_bound(
                    &service.metadata(&decoder::metadata(arguments)?)?,
                    output_bytes,
                )
                .map(Some)
            }
        }
        _ => Err(FsToolError::invalid(
            FsToolOperation::Catalog,
            "mutation kind reached immutable dispatcher",
        )),
    }
}

pub(super) fn compile_mutation(
    kind: FsDispatchKind,
    gateway: &Arc<Mutex<WorkspaceGateway>>,
    artifact_resolver: Option<&dyn ArtifactInputResolver>,
    caller: &WorkspaceCallerBinding,
    arguments: &BoundedJson,
) -> Result<PatchSet, FsToolError> {
    let (workspace_id, generation, revision) = {
        let gateway = gateway.lock().map_err(|_| {
            FsToolError::new(
                FsToolErrorKind::Workspace,
                compiled_operation(kind),
                RecoveryClass::Reconcile,
                "workspace gateway lock is poisoned",
            )
        })?;
        let state = gateway.state();
        let version = (state.binding().workspace_id(), state.generation(), state.revision());
        drop(gateway);
        version
    };
    let version = WorkspaceVersion::new(workspace_id, generation, revision);
    let compiled = match kind {
        FsDispatchKind::Create => {
            let mut input = decoder::create(arguments)?;
            resolve_content(&mut input.0.content, caller, artifact_resolver, kind)?;
            CompiledMutation::create(version, input)
        }
        FsDispatchKind::Patch => {
            let mut input = decoder::patch(arguments)?;
            for edit in &mut input.edits {
                match edit {
                    crate::PatchEdit::Create(input) => {
                        resolve_content(&mut input.0.content, caller, artifact_resolver, kind)?;
                    }
                    crate::PatchEdit::Replace(input) => {
                        resolve_content(&mut input.0.content, caller, artifact_resolver, kind)?;
                    }
                    crate::PatchEdit::Remove(_) => {}
                }
            }
            CompiledMutation::patch(version, input)
        }
        FsDispatchKind::Remove => CompiledMutation::remove(version, decoder::remove(arguments)?),
        FsDispatchKind::Replace => {
            let mut input = decoder::replace(arguments)?;
            resolve_content(&mut input.0.content, caller, artifact_resolver, kind)?;
            CompiledMutation::replace(version, input)
        }
        FsDispatchKind::Write => {
            let mut input = decoder::write(arguments)?;
            resolve_content(&mut input.0.final_input.content, caller, artifact_resolver, kind)?;
            CompiledMutation::write(version, input)
        }
        _ => Err(FsToolError::invalid(
            FsToolOperation::Catalog,
            "read kind reached mutation dispatcher",
        )),
    }?;
    Ok(compiled.into_patch())
}

pub(super) fn resolve_content(
    content: &mut MutationContent,
    caller: &WorkspaceCallerBinding,
    resolver: Option<&dyn ArtifactInputResolver>,
    kind: FsDispatchKind,
) -> Result<(), FsToolError> {
    let MutationContent::Artifact(reference) = content else {
        return Ok(());
    };
    if reference.completeness() != ArtifactCompleteness::Complete {
        return Err(FsToolError::new(
            FsToolErrorKind::Artifact,
            compiled_operation(kind),
            RecoveryClass::CorrectInput,
            "mutation artifact is not marked complete",
        ));
    }
    let resolver = resolver.ok_or_else(|| {
        FsToolError::new(
            FsToolErrorKind::Artifact,
            compiled_operation(kind),
            RecoveryClass::Reauthorize,
            "no caller-scoped artifact authority is available",
        )
    })?;
    let bytes = resolver.resolve_complete(caller, reference).map_err(|failure| {
        let (recovery, detail) = match failure {
            ArtifactInputFailure::Unauthorized => (
                RecoveryClass::Reauthorize,
                "artifact reference is not authorized for this workspace caller",
            ),
            ArtifactInputFailure::Missing => {
                (RecoveryClass::CorrectInput, "complete mutation artifact is unavailable")
            }
            ArtifactInputFailure::Corrupt => {
                (RecoveryClass::Reconcile, "mutation artifact failed integrity verification")
            }
            ArtifactInputFailure::Indeterminate => (
                RecoveryClass::Reconcile,
                "artifact authority could not establish a complete immutable read",
            ),
        };
        FsToolError::new(FsToolErrorKind::Artifact, compiled_operation(kind), recovery, detail)
    })?;
    if u64::try_from(bytes.len()).ok() != Some(reference.size())
        || peritus_codec::sha256(&bytes) != reference.digest()
    {
        return Err(FsToolError::new(
            FsToolErrorKind::Artifact,
            compiled_operation(kind),
            RecoveryClass::Reconcile,
            "resolved mutation artifact differs from its declared digest or length",
        ));
    }
    *content = MutationContent::Inline(bytes);
    Ok(())
}

pub(super) const fn workspace_error(kind: FsDispatchKind, error: &WorkspaceError) -> FsToolError {
    let recovery = match error.recovery() {
        WorkspaceRecoveryClass::CorrectRequest => RecoveryClass::CorrectInput,
        WorkspaceRecoveryClass::Reauthorize => RecoveryClass::Reauthorize,
        WorkspaceRecoveryClass::Reobserve => RecoveryClass::Reobserve,
        WorkspaceRecoveryClass::Reconcile | WorkspaceRecoveryClass::Quarantine => {
            RecoveryClass::Reconcile
        }
    };
    FsToolError::new(FsToolErrorKind::Workspace, compiled_operation(kind), recovery, error.detail())
        .with_external_code(error.code().as_str())
}

pub(super) const fn retained_action_failure(
    kind: FsDispatchKind,
    indeterminate: bool,
) -> FsToolError {
    let (recovery, code, detail) = if indeterminate {
        (
            RecoveryClass::Reconcile,
            WorkspaceErrorCode::Indeterminate.as_str(),
            "retained patch evidence does not prove a safe terminal workspace state",
        )
    } else {
        (
            RecoveryClass::Reauthorize,
            WorkspaceErrorCode::ReceiptReused.as_str(),
            "retained patch evidence proves this action did not install its requested mutation",
        )
    };
    FsToolError::new(FsToolErrorKind::Workspace, compiled_operation(kind), recovery, detail)
        .with_external_code(code)
}

pub(super) fn caller_binding(invocation: &AuthorizedInvocation) -> WorkspaceCallerBinding {
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

pub(super) fn context_matches(
    context: &DispatchContext<'_>,
    caller: &WorkspaceCallerBinding,
) -> bool {
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

pub(super) const fn compiled_operation(kind: FsDispatchKind) -> FsToolOperation {
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

pub(super) fn finish(
    prepared: &PreparedToolCall,
    rendered: &RenderedOutput,
    started_at: AuthorityInstant,
    completed_at: AuthorityInstant,
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
        0,
    )
    .map_err(|_| protocol_failure("terminal filesystem result is invalid"))
}

pub(super) const fn minimum_result_capacity(prepared: &PreparedToolCall) -> bool {
    let limits = prepared.call().limits();
    limits.output_bytes() >= 512 && limits.model_bytes() >= 128 && limits.human_bytes() >= 128
}

pub(super) fn tool_failure(error: &FsToolError) -> DispatchFailure {
    let category = match error.kind() {
        FsToolErrorKind::Inspection | FsToolErrorKind::Patch | FsToolErrorKind::Workspace => {
            FailureCategory::Workspace
        }
        FsToolErrorKind::Artifact => FailureCategory::Workspace,
        FsToolErrorKind::Unsupported => FailureCategory::Infrastructure,
        FsToolErrorKind::InvalidInput | FsToolErrorKind::Protocol => FailureCategory::Protocol,
    };
    failure(category, error.code(), error.detail(), error.recovery())
}

pub(super) const fn poisoned_gateway(kind: FsDispatchKind) -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::Workspace,
        compiled_operation(kind),
        RecoveryClass::Reconcile,
        "workspace gateway lock is poisoned",
    )
}

pub(super) fn cancelled_failure() -> DispatchFailure {
    cancellation_failure(ResultStatus::Cancelled)
}

pub(super) fn cancellation_failure(status: ResultStatus) -> DispatchFailure {
    let (category, code, detail) = if status == ResultStatus::TimedOut {
        (
            FailureCategory::Timeout,
            "PERITUS-FS-TIMEOUT",
            "deadline elapsed before the mutation transaction started",
        )
    } else {
        (
            FailureCategory::Cancelled,
            "PERITUS-FS-CANCELLED",
            "mutation was cancelled before its patch transaction started",
        )
    };
    let failure = ToolFailure::new(
        category,
        bounded(code),
        ResponsibleSubsystem::Workspace,
        Retryability::NewAction,
        RecoveryRoute::None,
        bounded(detail),
    );
    DispatchFailure::new(status, failure).expect("filesystem cancellation is non-success")
}

pub(super) fn protocol_failure(detail: &'static str) -> DispatchFailure {
    failure(
        FailureCategory::Protocol,
        FsToolErrorKind::Protocol.code(),
        detail,
        RecoveryClass::CorrectInput,
    )
}

pub(super) fn failure(
    category: FailureCategory,
    code: &'static str,
    detail: &'static str,
    recovery: RecoveryClass,
) -> DispatchFailure {
    let (retryability, route) = match recovery {
        RecoveryClass::CorrectInput | RecoveryClass::SelectSupportedOperation => {
            (Retryability::NewAction, RecoveryRoute::None)
        }
        RecoveryClass::Reobserve => (Retryability::NewAction, RecoveryRoute::ReconcileWorkspace),
        RecoveryClass::Reauthorize => (Retryability::NewAction, RecoveryRoute::Reauthorize),
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

pub(super) fn bounded(value: &str) -> BoundedText {
    BoundedText::new(value.to_owned()).expect("static filesystem failure text is bounded")
}
