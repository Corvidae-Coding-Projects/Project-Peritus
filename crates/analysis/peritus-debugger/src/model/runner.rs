//! Pull-based provider execution with durable failure evidence and cumulative accounting.

use peritus_model_protocol::{
    Continuation, EventEnvelope, FailureCategory, FinishReason, ModelFailure, OutcomeCertainty,
    ProtocolLimits, ReducedItem, ResponseId, ResponseReducer, Retryability, TerminalOutcome,
    TransportPhase,
};
use peritus_provider_core::{
    CancellationToken, ModelProvider, ProviderCoreError, ProviderCoreErrorKind,
    ProviderRecoveryDisposition, ProviderTerminal, validate_request_profile,
};
use peritus_types::Sha256Digest;

use crate::{
    DebuggerErrorKind, DebuggerLimit, DebuggerLimits, DebuggerRecovery,
    ModelAcceptanceCertainty, ModelAttemptFailureCode, ModelFailureContext, ModelFailureOrigin,
    ModelFailurePhase, ModelFailureRecovery, ModelProviderFailureCause, TraceSelectionManifest,
    ValidatedModelProposal,
};

use super::{
    ModelAnalysisPlan,
    proposal::{ModelProposalRejection, ModelProposalRejectionCause},
};

#[cfg(test)]
mod tests;

/// Cumulative work already charged to the logical model task.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ModelPriorUsage {
    pub(crate) events: u64,
    pub(crate) output_bytes: u64,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) total_tokens: u64,
}

/// Complete successful attempt accounting and validated inert proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRunSuccess {
    proposal: ValidatedModelProposal,
    output_digest: Sha256Digest,
    output_bytes: u64,
    event_count: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

impl ModelRunSuccess {
    #[must_use]
    pub const fn proposal(&self) -> &ValidatedModelProposal {
        &self.proposal
    }
    #[must_use]
    pub const fn output_digest(&self) -> Sha256Digest {
        self.output_digest
    }
    #[must_use]
    pub const fn output_bytes(&self) -> u64 {
        self.output_bytes
    }
    #[must_use]
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }
    #[must_use]
    pub const fn input_tokens(&self) -> u64 {
        self.input_tokens
    }
    #[must_use]
    pub const fn output_tokens(&self) -> u64 {
        self.output_tokens
    }
    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.total_tokens
    }
}

/// Complete redaction-safe evidence needed to settle one failed attempt durably.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRunFailure {
    code: ModelAttemptFailureCode,
    context: ModelFailureContext,
    diagnostic_digest: Sha256Digest,
    event_count: u64,
    output_bytes: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

impl ModelRunFailure {
    /// Stable debugger-facing failure class.
    #[must_use]
    pub const fn kind(&self) -> DebuggerErrorKind {
        match self.code {
            ModelAttemptFailureCode::InvalidProposal
            | ModelAttemptFailureCode::InvalidOutputShape
            | ModelAttemptFailureCode::UnsuccessfulTerminal => DebuggerErrorKind::ModelRejected,
            ModelAttemptFailureCode::BudgetExceeded => DebuggerErrorKind::Budget,
            ModelAttemptFailureCode::Cancelled => DebuggerErrorKind::Cancelled,
            ModelAttemptFailureCode::ProviderStart
            | ModelAttemptFailureCode::ProviderStream
            | ModelAttemptFailureCode::MalformedStream
            | ModelAttemptFailureCode::ProviderFailure => DebuggerErrorKind::ModelProtocol,
        }
    }
    /// Stable caller action derived from the retained recovery evidence.
    #[must_use]
    pub const fn recovery(&self) -> DebuggerRecovery {
        match self.context.recovery() {
            ModelFailureRecovery::RetrySameRoute
            | ModelFailureRecovery::ResumeExact
            | ModelFailureRecovery::CallerDecision => DebuggerRecovery::Retry,
            ModelFailureRecovery::AwaitCredentialRepair
            | ModelFailureRecovery::TryAuthorizedFallback
            | ModelFailureRecovery::CompactThenRetry => DebuggerRecovery::RepairDependency,
            ModelFailureRecovery::Stop => DebuggerRecovery::None,
        }
    }
    /// Stable attempt failure code.
    #[must_use]
    pub const fn code(&self) -> ModelAttemptFailureCode {
        self.code
    }
    /// Exact provider identity, acceptance, recovery, and continuation evidence.
    #[must_use]
    pub const fn context(&self) -> &ModelFailureContext {
        &self.context
    }
    /// Digest of redaction-safe diagnostic metadata.
    #[must_use]
    pub const fn diagnostic_digest(&self) -> Sha256Digest {
        self.diagnostic_digest
    }
    /// Normalized provider observations consumed by the failed attempt.
    #[must_use]
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }
    /// Output bytes assembled before failure.
    #[must_use]
    pub const fn output_bytes(&self) -> u64 {
        self.output_bytes
    }
    /// Input-token high water observed before failure.
    #[must_use]
    pub const fn input_tokens(&self) -> u64 {
        self.input_tokens
    }
    /// Output-token high water observed before failure.
    #[must_use]
    pub const fn output_tokens(&self) -> u64 {
        self.output_tokens
    }
    /// Total-token high water observed before failure.
    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.total_tokens
    }
}

impl core::fmt::Display for ModelRunFailure {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "model attempt failed with {:?} during {:?}",
            self.code,
            self.context.origin(),
        )
    }
}

impl std::error::Error for ModelRunFailure {}

/// Runs one model attempt without prior failed-attempt consumption.
///
/// Runtime owners that retain attempt history use the crate-private cumulative entry point.
///
/// # Errors
/// Returns a durable typed failure containing provider recovery truth and observed accounting.
pub async fn run_model_analysis(
    provider: &dyn ModelProvider,
    plan: &ModelAnalysisPlan,
    manifest: &TraceSelectionManifest,
    debugger_limits: DebuggerLimits,
    cancellation: CancellationToken,
) -> Result<ModelRunSuccess, ModelRunFailure> {
    run_model_analysis_with_usage(
        provider,
        plan,
        manifest,
        debugger_limits,
        cancellation,
        ModelPriorUsage::default(),
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments, reason = "durable usage and continuation remain explicit")]
pub(crate) async fn run_model_analysis_with_usage(
    provider: &dyn ModelProvider,
    plan: &ModelAnalysisPlan,
    manifest: &TraceSelectionManifest,
    debugger_limits: DebuggerLimits,
    cancellation: CancellationToken,
    prior: ModelPriorUsage,
    continuation: Option<&Continuation>,
) -> Result<ModelRunSuccess, ModelRunFailure> {
    let profile = provider.profile();
    let mut observations = ObservationDigest::new();
    if cancellation.is_cancelled() {
        return Err(cancelled_failure(
            profile,
            None,
            &observations,
            AttemptUsage::default(),
            false,
            false,
        ));
    }
    if manifest.id() != plan.manifest_id() || manifest.digest() != plan.manifest_digest() {
        return Err(local_failure(
            profile,
            ModelAttemptFailureCode::InvalidProposal,
            ModelFailureOrigin::OutputValidation,
            ModelProviderFailureCause::InvalidRequest,
            ModelFailurePhase::BeforeSend,
            ModelAcceptanceCertainty::DefinitelyNotAccepted,
            None,
            "model plan and selection manifest differ",
            &observations,
            AttemptUsage::default(),
        ));
    }
    validate_request_profile(profile, plan.request()).map_err(|error| {
        core_failure(
            profile,
            ModelFailureOrigin::ProfileValidation,
            &error,
            None,
            &observations,
            AttemptUsage::default(),
        )
    })?;
    let budget = plan.budget();
    if prior.events >= budget.max_events()
        || prior.output_bytes >= budget.max_output_bytes()
        || prior.output_bytes >= debugger_limits.get(DebuggerLimit::ModelOutputBytes)
        || prior.input_tokens >= budget.max_input_tokens()
        || prior.output_tokens >= budget.max_output_tokens()
        || prior.total_tokens >= budget.max_total_tokens()
    {
        return Err(local_failure(
            profile,
            ModelAttemptFailureCode::BudgetExceeded,
            ModelFailureOrigin::Budget,
            ModelProviderFailureCause::LimitExceeded,
            ModelFailurePhase::BeforeSend,
            ModelAcceptanceCertainty::DefinitelyNotAccepted,
            None,
            "cumulative model allowance is exhausted",
            &observations,
            AttemptUsage::default(),
        ));
    }
    let reducer_limits = selected_structured_limits(plan, debugger_limits, prior);
    let request = if let Some(continuation) = continuation {
        plan.request()
            .clone()
            .with_continuation(continuation.clone(), plan.protocol_limits())
            .map_err(|error| {
                local_failure(
                    profile,
                    ModelAttemptFailureCode::ProviderFailure,
                    ModelFailureOrigin::ProfileValidation,
                    ModelProviderFailureCause::InvalidRequest,
                    ModelFailurePhase::BeforeSend,
                    ModelAcceptanceCertainty::DefinitelyNotAccepted,
                    None,
                    &error.to_string(),
                    &observations,
                    AttemptUsage::default(),
                )
            })?
    } else {
        plan.request().clone()
    };
    let mut reducer = ResponseReducer::new(request.provider().clone(), reducer_limits);
    let mut stream = provider
        .start(request, cancellation.clone())
        .await
        .map_err(|error| {
            core_failure(
                profile,
                ModelFailureOrigin::ProviderStart,
                &error,
                None,
                &observations,
                AttemptUsage::default(),
            )
        })?;
    while reducer.terminal().is_none() {
        let usage = AttemptUsage::from_reducer(&reducer, observations.count);
        if cancellation.is_cancelled() {
            stream.cancel();
            return Err(cancelled_failure(
                profile,
                continuation_from(profile, &reducer, None),
                &observations,
                usage,
                true,
                false,
            ));
        }
        let envelope = stream.pull().await.map_err(|error| {
            core_failure(
                profile,
                ModelFailureOrigin::ProviderStream,
                &error,
                continuation_from(profile, &reducer, None),
                &observations,
                AttemptUsage::from_reducer(&reducer, observations.count),
            )
        })?;
        let Some(envelope) = envelope else {
            return Err(local_failure(
                profile,
                ModelAttemptFailureCode::MalformedStream,
                ModelFailureOrigin::ModelProtocol,
                ModelProviderFailureCause::IncompleteStream,
                ModelFailurePhase::StreamObserved,
                acceptance_for(&reducer),
                continuation_from(profile, &reducer, None),
                "provider stream closed without a normalized terminal",
                &observations,
                AttemptUsage::from_reducer(&reducer, observations.count),
            ));
        };
        observations.observe(&envelope);
        if let Err(error) = reducer.push(envelope) {
            let usage = AttemptUsage::from_reducer(&reducer, observations.count);
            if let Some(TerminalOutcome::Failed(failure)) = reducer.terminal() {
                return Err(terminal_failure(
                    profile,
                    failure,
                    &reducer,
                    &observations,
                    usage,
                    ModelFailureOrigin::ModelProtocol,
                    ModelAttemptFailureCode::MalformedStream,
                ));
            }
            return Err(local_failure(
                profile,
                ModelAttemptFailureCode::MalformedStream,
                ModelFailureOrigin::ModelProtocol,
                ModelProviderFailureCause::MalformedPayload,
                ModelFailurePhase::StreamObserved,
                acceptance_for(&reducer),
                continuation_from(profile, &reducer, None),
                &error.to_string(),
                &observations,
                usage,
            ));
        }
        if reducer.output_bytes() > plan.budget().max_output_bytes()
            || cumulative_exceeds(
                prior,
                AttemptUsage::from_reducer(&reducer, observations.count),
                plan,
                debugger_limits,
            )
        {
            stream.cancel();
            return Err(budget_failure(profile, &reducer, &observations));
        }
    }
    let usage = AttemptUsage::from_reducer(&reducer, observations.count);
    match reducer.terminal().cloned() {
        Some(TerminalOutcome::Succeeded { reason: FinishReason::Stop }) => {}
        Some(TerminalOutcome::Failed(failure)) => {
            return Err(terminal_failure(
                profile,
                &failure,
                &reducer,
                &observations,
                usage,
                ModelFailureOrigin::ProviderTerminal,
                ModelAttemptFailureCode::ProviderFailure,
            ));
        }
        Some(TerminalOutcome::Cancelled) => {
            return Err(cancelled_failure(
                profile,
                continuation_from(profile, &reducer, None),
                &observations,
                usage,
                true,
                true,
            ));
        }
        Some(TerminalOutcome::Refused { .. }) => {
            return Err(local_failure(
                profile,
                ModelAttemptFailureCode::UnsuccessfulTerminal,
                ModelFailureOrigin::ProviderTerminal,
                ModelProviderFailureCause::Refusal,
                ModelFailurePhase::Completed,
                ModelAcceptanceCertainty::Terminal,
                continuation_from(profile, &reducer, None),
                "model response ended in refusal",
                &observations,
                usage,
            ));
        }
        Some(TerminalOutcome::Incomplete { reason }) => {
            let (cause, recovery) = if reason == FinishReason::ContextLimit {
                (
                    ModelProviderFailureCause::LimitExceeded,
                    ModelFailureRecovery::CompactThenRetry,
                )
            } else {
                (ModelProviderFailureCause::IncompleteStream, ModelFailureRecovery::Stop)
            };
            return Err(local_failure_with_recovery(
                profile,
                ModelAttemptFailureCode::UnsuccessfulTerminal,
                ModelFailureOrigin::ProviderTerminal,
                cause,
                recovery,
                ModelFailurePhase::Completed,
                ModelAcceptanceCertainty::Terminal,
                continuation_from(profile, &reducer, None),
                "model response ended incomplete",
                &observations,
                usage,
            ));
        }
        Some(TerminalOutcome::RequiresAction { .. }) => {
            return Err(local_failure(
                profile,
                ModelAttemptFailureCode::InvalidOutputShape,
                ModelFailureOrigin::OutputValidation,
                ModelProviderFailureCause::Provider,
                ModelFailurePhase::Completed,
                ModelAcceptanceCertainty::Terminal,
                continuation_from(profile, &reducer, None),
                "tool or continuation output is invalid for the tool-free analysis",
                &observations,
                usage,
            ));
        }
        Some(TerminalOutcome::Succeeded { .. }) => {
            return Err(local_failure(
                profile,
                ModelAttemptFailureCode::UnsuccessfulTerminal,
                ModelFailureOrigin::OutputValidation,
                ModelProviderFailureCause::Provider,
                ModelFailurePhase::Completed,
                ModelAcceptanceCertainty::Terminal,
                continuation_from(profile, &reducer, None),
                "model response did not end in a normal successful stop",
                &observations,
                usage,
            ));
        }
        None => {
            return Err(local_failure(
                profile,
                ModelAttemptFailureCode::MalformedStream,
                ModelFailureOrigin::ModelProtocol,
                ModelProviderFailureCause::IncompleteStream,
                ModelFailurePhase::StreamObserved,
                acceptance_for(&reducer),
                continuation_from(profile, &reducer, None),
                "model reducer has no terminal outcome",
                &observations,
                usage,
            ));
        }
    }
    let [ReducedItem::Structured { value, .. }] = reducer.completed_items() else {
        return Err(local_failure(
            profile,
            ModelAttemptFailureCode::InvalidOutputShape,
            ModelFailureOrigin::OutputValidation,
            ModelProviderFailureCause::Provider,
            ModelFailurePhase::Completed,
            ModelAcceptanceCertainty::Terminal,
            continuation_from(profile, &reducer, None),
            "model response must contain exactly one structured item and nothing else",
            &observations,
            usage,
        ));
    };
    let output_bytes = u64::try_from(value.canonical_bytes().len()).unwrap_or(u64::MAX);
    if output_bytes > plan.budget().max_output_bytes() {
        return Err(budget_failure(profile, &reducer, &observations));
    }
    let proposal = ValidatedModelProposal::validate_classified(
        value,
        manifest,
        plan.deterministic_digest(),
        debugger_limits,
    )
    .map_err(|error| {
        proposal_failure(
            profile,
            &reducer,
            &observations,
            usage,
            &error,
        )
    })?;
    Ok(ModelRunSuccess {
        proposal,
        output_digest: value.digest(),
        output_bytes,
        event_count: usage.event_count,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct AttemptUsage {
    event_count: u64,
    output_bytes: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

impl AttemptUsage {
    fn from_reducer(reducer: &ResponseReducer, event_count: u64) -> Self {
        let usage = reducer.usage_high_water();
        Self {
            event_count,
            output_bytes: reducer.output_bytes(),
            input_tokens: usage.input_tokens().unwrap_or(0),
            output_tokens: usage.output_tokens().unwrap_or(0),
            total_tokens: usage.total_tokens().unwrap_or(0),
        }
    }
}

struct ObservationDigest {
    count: u64,
    digest: Sha256Digest,
}

impl ObservationDigest {
    fn new() -> Self {
        Self {
            count: 0,
            digest: peritus_codec::sha256(b"peritus.debugger.model-observations.v2\0"),
        }
    }

    fn observe(&mut self, envelope: &EventEnvelope) {
        self.count = self.count.saturating_add(1);
        let mut bytes = Vec::with_capacity(128);
        bytes.extend_from_slice(self.digest.as_bytes());
        bytes.extend_from_slice(&envelope.sequence().to_be_bytes());
        bytes.extend_from_slice(&envelope.provider_sequence().unwrap_or(0).to_be_bytes());
        if let Some(event_id) = envelope.provider_event_id() {
            bytes.extend_from_slice(event_id.expose_for_wire().as_bytes());
        }
        bytes.extend_from_slice(envelope.provider_digest().as_bytes());
        self.digest = peritus_codec::sha256(&bytes);
    }
}

fn cumulative_exceeds(
    prior: ModelPriorUsage,
    current: AttemptUsage,
    plan: &ModelAnalysisPlan,
    debugger_limits: DebuggerLimits,
) -> bool {
    let budget = plan.budget();
    let output_bytes = prior.output_bytes.saturating_add(current.output_bytes);
    prior.events.saturating_add(current.event_count) > budget.max_events()
        || output_bytes > budget.max_output_bytes()
        || output_bytes > debugger_limits.get(DebuggerLimit::ModelOutputBytes)
        || prior.input_tokens.saturating_add(current.input_tokens) > budget.max_input_tokens()
        || prior.output_tokens.saturating_add(current.output_tokens) > budget.max_output_tokens()
        || prior.total_tokens.saturating_add(current.total_tokens) > budget.max_total_tokens()
}

fn selected_structured_limits(
    plan: &ModelAnalysisPlan,
    debugger_limits: DebuggerLimits,
    prior: ModelPriorUsage,
) -> ProtocolLimits {
    const MAX_TOOL_ARGUMENT_BYTES_INDEX: usize = 11;

    let selected = plan
        .budget()
        .max_output_bytes()
        .saturating_sub(prior.output_bytes)
        .min(
            debugger_limits
                .get(DebuggerLimit::ModelOutputBytes)
                .saturating_sub(prior.output_bytes),
        );
    let selected = usize::try_from(selected).unwrap_or(usize::MAX);
    let mut values = plan.protocol_limits().as_array();
    values[MAX_TOOL_ARGUMENT_BYTES_INDEX] =
        values[MAX_TOOL_ARGUMENT_BYTES_INDEX].min(selected);
    ProtocolLimits::new(values)
        .expect("validated model plan and nonexhausted usage produce valid protocol limits")
}

fn budget_failure(
    profile: &peritus_model_protocol::ProviderProfile,
    reducer: &ResponseReducer,
    observations: &ObservationDigest,
) -> ModelRunFailure {
    local_failure(
        profile,
        ModelAttemptFailureCode::BudgetExceeded,
        ModelFailureOrigin::Budget,
        ModelProviderFailureCause::LimitExceeded,
        if reducer.terminal().is_some() {
            ModelFailurePhase::Completed
        } else {
            ModelFailurePhase::StreamObserved
        },
        acceptance_for(reducer),
        continuation_from(profile, reducer, None),
        "model event or token allowance was exceeded",
        observations,
        AttemptUsage::from_reducer(reducer, observations.count),
    )
}

fn proposal_failure(
    profile: &peritus_model_protocol::ProviderProfile,
    reducer: &ResponseReducer,
    observations: &ObservationDigest,
    usage: AttemptUsage,
    rejection: &ModelProposalRejection,
) -> ModelRunFailure {
    let (code, origin, cause) = match rejection.cause() {
        ModelProposalRejectionCause::Schema => (
            ModelAttemptFailureCode::InvalidProposal,
            ModelFailureOrigin::OutputValidation,
            ModelProviderFailureCause::InvalidOutputSchema,
        ),
        ModelProposalRejectionCause::Binding => (
            ModelAttemptFailureCode::InvalidProposal,
            ModelFailureOrigin::OutputValidation,
            ModelProviderFailureCause::InvalidOutputBinding,
        ),
        ModelProposalRejectionCause::Text => (
            ModelAttemptFailureCode::InvalidProposal,
            ModelFailureOrigin::OutputValidation,
            ModelProviderFailureCause::InvalidOutputText,
        ),
        ModelProposalRejectionCause::Citation => (
            ModelAttemptFailureCode::InvalidProposal,
            ModelFailureOrigin::OutputValidation,
            ModelProviderFailureCause::InvalidOutputCitation,
        ),
        ModelProposalRejectionCause::Collection => (
            ModelAttemptFailureCode::InvalidProposal,
            ModelFailureOrigin::OutputValidation,
            ModelProviderFailureCause::InvalidOutputCollection,
        ),
        ModelProposalRejectionCause::Budget => (
            ModelAttemptFailureCode::BudgetExceeded,
            ModelFailureOrigin::Budget,
            ModelProviderFailureCause::LimitExceeded,
        ),
    };
    local_failure(
        profile,
        code,
        origin,
        cause,
        ModelFailurePhase::Completed,
        ModelAcceptanceCertainty::Terminal,
        continuation_from(profile, reducer, None),
        &rejection.error().to_string(),
        observations,
        usage,
    )
}

fn cancelled_failure(
    profile: &peritus_model_protocol::ProviderProfile,
    continuation: Option<Continuation>,
    observations: &ObservationDigest,
    usage: AttemptUsage,
    request_started: bool,
    terminal: bool,
) -> ModelRunFailure {
    let (phase, acceptance) = if terminal {
        (ModelFailurePhase::Completed, ModelAcceptanceCertainty::Terminal)
    } else if !request_started {
        (
            ModelFailurePhase::BeforeSend,
            ModelAcceptanceCertainty::DefinitelyNotAccepted,
        )
    } else if usage.event_count == 0 {
        (ModelFailurePhase::ReadingBody, ModelAcceptanceCertainty::MaybeAccepted)
    } else if usage.output_bytes == 0 {
        (ModelFailurePhase::StreamObserved, ModelAcceptanceCertainty::MaybeAccepted)
    } else {
        (ModelFailurePhase::StreamObserved, ModelAcceptanceCertainty::AcceptedPartial)
    };
    local_failure(
        profile,
        ModelAttemptFailureCode::Cancelled,
        ModelFailureOrigin::Cancellation,
        ModelProviderFailureCause::Cancelled,
        phase,
        acceptance,
        continuation,
        "model analysis was cancelled",
        observations,
        usage,
    )
}

#[allow(clippy::too_many_arguments, reason = "failure evidence remains explicit")]
fn local_failure(
    profile: &peritus_model_protocol::ProviderProfile,
    code: ModelAttemptFailureCode,
    origin: ModelFailureOrigin,
    cause: ModelProviderFailureCause,
    phase: ModelFailurePhase,
    acceptance: ModelAcceptanceCertainty,
    continuation: Option<Continuation>,
    detail: &str,
    observations: &ObservationDigest,
    usage: AttemptUsage,
) -> ModelRunFailure {
    local_failure_with_recovery(
        profile,
        code,
        origin,
        cause,
        ModelFailureRecovery::Stop,
        phase,
        acceptance,
        continuation,
        detail,
        observations,
        usage,
    )
}

#[allow(clippy::too_many_arguments, reason = "failure evidence remains explicit")]
fn local_failure_with_recovery(
    profile: &peritus_model_protocol::ProviderProfile,
    code: ModelAttemptFailureCode,
    origin: ModelFailureOrigin,
    cause: ModelProviderFailureCause,
    recovery: ModelFailureRecovery,
    phase: ModelFailurePhase,
    acceptance: ModelAcceptanceCertainty,
    continuation: Option<Continuation>,
    detail: &str,
    observations: &ObservationDigest,
    usage: AttemptUsage,
) -> ModelRunFailure {
    let context = ModelFailureContext::new(
        profile.profile_id(),
        profile.revision(),
        origin,
        cause,
        recovery,
        phase,
        acceptance,
        continuation,
        None,
        None,
        observations.digest,
    )
    .expect("checked provider profile and continuation produce valid failure context");
    ModelRunFailure {
        code,
        context,
        diagnostic_digest: peritus_codec::sha256(detail.as_bytes()),
        event_count: usage.event_count,
        output_bytes: usage.output_bytes,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
    }
}

fn core_failure(
    profile: &peritus_model_protocol::ProviderProfile,
    origin: ModelFailureOrigin,
    error: &ProviderCoreError,
    continuation: Option<Continuation>,
    observations: &ObservationDigest,
    usage: AttemptUsage,
) -> ModelRunFailure {
    let terminal = ProviderTerminal::from_core_error(error);
    let (phase, acceptance) = core_failure_position(origin, error.kind(), usage);
    let recovery = normalize_recovery(
        recovery(terminal.recovery()),
        acceptance,
        continuation.as_ref(),
    );
    let context = ModelFailureContext::new(
        profile.profile_id(),
        profile.revision(),
        origin,
        core_cause(error.kind()),
        recovery,
        phase,
        acceptance,
        continuation,
        None,
        None,
        observations.digest,
    )
    .expect("checked provider profile and continuation produce valid failure context");
    ModelRunFailure {
        code: if error.kind() == ProviderCoreErrorKind::Cancelled {
            ModelAttemptFailureCode::Cancelled
        } else {
            ModelAttemptFailureCode::ProviderFailure
        },
        context,
        diagnostic_digest: peritus_codec::sha256(error.to_string().as_bytes()),
        event_count: usage.event_count,
        output_bytes: usage.output_bytes,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
    }
}

fn terminal_failure(
    profile: &peritus_model_protocol::ProviderProfile,
    failure: &ModelFailure,
    reducer: &ResponseReducer,
    observations: &ObservationDigest,
    usage: AttemptUsage,
    origin: ModelFailureOrigin,
    code: ModelAttemptFailureCode,
) -> ModelRunFailure {
    let continuation = continuation_from(profile, reducer, failure.response_id());
    let terminal = ProviderTerminal::from_model_failure(failure);
    let acceptance = acceptance(failure.certainty());
    let requested_recovery = match failure.retryability() {
        Retryability::Never => ModelFailureRecovery::Stop,
        Retryability::SafeNewRequest => recovery(terminal.recovery()),
        Retryability::ExactResumeOnly => ModelFailureRecovery::ResumeExact,
        Retryability::CallerDecision => ModelFailureRecovery::CallerDecision,
    };
    let recovery = normalize_recovery(requested_recovery, acceptance, continuation.as_ref());
    let context = ModelFailureContext::new(
        profile.profile_id(),
        profile.revision(),
        origin,
        terminal_cause(failure.category()),
        recovery,
        failure_phase(failure.phase()),
        acceptance,
        continuation,
        failure.http_status(),
        failure.retry_after_millis(),
        observations.digest,
    )
    .expect("checked provider terminal produces valid failure context");
    ModelRunFailure {
        code: if failure.category() == FailureCategory::Cancellation {
            ModelAttemptFailureCode::Cancelled
        } else {
            code
        },
        context,
        diagnostic_digest: peritus_codec::sha256(failure.diagnostic().code().as_bytes()),
        event_count: usage.event_count,
        output_bytes: usage.output_bytes,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
    }
}

fn core_failure_position(
    origin: ModelFailureOrigin,
    kind: ProviderCoreErrorKind,
    usage: AttemptUsage,
) -> (ModelFailurePhase, ModelAcceptanceCertainty) {
    let before_send = matches!(
        kind,
        ProviderCoreErrorKind::InvalidEndpoint
            | ProviderCoreErrorKind::InvalidCredential
            | ProviderCoreErrorKind::InvalidRequest
            | ProviderCoreErrorKind::InvalidHttp
            | ProviderCoreErrorKind::LimitExceeded
            | ProviderCoreErrorKind::InvalidRetry
            | ProviderCoreErrorKind::Configuration
            | ProviderCoreErrorKind::UnsupportedCapability
            | ProviderCoreErrorKind::Unavailable
    );
    match origin {
        ModelFailureOrigin::ProfileValidation => (
            ModelFailurePhase::BeforeSend,
            ModelAcceptanceCertainty::DefinitelyNotAccepted,
        ),
        ModelFailureOrigin::ProviderStart if before_send => (
            ModelFailurePhase::BeforeSend,
            ModelAcceptanceCertainty::DefinitelyNotAccepted,
        ),
        ModelFailureOrigin::ProviderStart if kind == ProviderCoreErrorKind::Connect => (
            ModelFailurePhase::Connecting,
            ModelAcceptanceCertainty::DefinitelyNotAccepted,
        ),
        ModelFailureOrigin::ProviderStart => (
            ModelFailurePhase::SendingBody,
            ModelAcceptanceCertainty::MaybeAccepted,
        ),
        ModelFailureOrigin::ProviderStream if usage.event_count == 0 => (
            ModelFailurePhase::ReadingBody,
            ModelAcceptanceCertainty::MaybeAccepted,
        ),
        ModelFailureOrigin::ProviderStream if usage.output_bytes == 0 => (
            ModelFailurePhase::StreamObserved,
            ModelAcceptanceCertainty::MaybeAccepted,
        ),
        ModelFailureOrigin::ProviderStream => (
            ModelFailurePhase::StreamObserved,
            ModelAcceptanceCertainty::AcceptedPartial,
        ),
        _ => (
            ModelFailurePhase::StreamObserved,
            ModelAcceptanceCertainty::MaybeAccepted,
        ),
    }
}

fn normalize_recovery(
    requested: ModelFailureRecovery,
    acceptance: ModelAcceptanceCertainty,
    continuation: Option<&Continuation>,
) -> ModelFailureRecovery {
    let exact = continuation.is_some_and(|value| {
        value.event_id().is_some() && value.sequence().is_some()
    });
    match requested {
        ModelFailureRecovery::Stop => ModelFailureRecovery::Stop,
        ModelFailureRecovery::ResumeExact if exact => ModelFailureRecovery::ResumeExact,
        ModelFailureRecovery::ResumeExact => ModelFailureRecovery::Stop,
        recovery
            if matches!(
                acceptance,
                ModelAcceptanceCertainty::DefinitelyNotAccepted
                    | ModelAcceptanceCertainty::Terminal
            ) => recovery,
        _ if exact => ModelFailureRecovery::ResumeExact,
        _ => ModelFailureRecovery::Stop,
    }
}

fn continuation_from(
    profile: &peritus_model_protocol::ProviderProfile,
    reducer: &ResponseReducer,
    terminal_response: Option<&ResponseId>,
) -> Option<Continuation> {
    reducer.continuation(profile.resume_kind()).or_else(|| match profile.resume_kind() {
        peritus_model_protocol::ResumeKind::SemanticContinuation => terminal_response
            .cloned()
            .and_then(|response| Continuation::new(response, None, None).ok()),
        peritus_model_protocol::ResumeKind::Unsupported
        | peritus_model_protocol::ResumeKind::ExactCursor => None,
    })
}

fn acceptance_for(reducer: &ResponseReducer) -> ModelAcceptanceCertainty {
    if reducer.terminal().is_some() {
        ModelAcceptanceCertainty::Terminal
    } else if reducer.output_bytes() > 0 {
        ModelAcceptanceCertainty::AcceptedPartial
    } else if reducer.response_id().is_some() || reducer.event_count() > 0 {
        ModelAcceptanceCertainty::MaybeAccepted
    } else {
        ModelAcceptanceCertainty::DefinitelyNotAccepted
    }
}

const fn recovery(value: ProviderRecoveryDisposition) -> ModelFailureRecovery {
    match value {
        ProviderRecoveryDisposition::RetrySameRoute => ModelFailureRecovery::RetrySameRoute,
        ProviderRecoveryDisposition::TryAuthorizedFallback => {
            ModelFailureRecovery::TryAuthorizedFallback
        }
        ProviderRecoveryDisposition::CompactThenRetry => ModelFailureRecovery::CompactThenRetry,
        ProviderRecoveryDisposition::AwaitCredentialRepair => {
            ModelFailureRecovery::AwaitCredentialRepair
        }
        ProviderRecoveryDisposition::Stop => ModelFailureRecovery::Stop,
    }
}

const fn core_cause(kind: ProviderCoreErrorKind) -> ModelProviderFailureCause {
    match kind {
        ProviderCoreErrorKind::InvalidEndpoint => ModelProviderFailureCause::InvalidEndpoint,
        ProviderCoreErrorKind::InvalidCredential => ModelProviderFailureCause::InvalidCredential,
        ProviderCoreErrorKind::InvalidRequest => ModelProviderFailureCause::InvalidRequest,
        ProviderCoreErrorKind::InvalidHttp => ModelProviderFailureCause::InvalidHttp,
        ProviderCoreErrorKind::LimitExceeded => ModelProviderFailureCause::LimitExceeded,
        ProviderCoreErrorKind::Cancelled => ModelProviderFailureCause::Cancelled,
        ProviderCoreErrorKind::Connect => ModelProviderFailureCause::Connect,
        ProviderCoreErrorKind::Transport => ModelProviderFailureCause::Transport,
        ProviderCoreErrorKind::MalformedStream => ModelProviderFailureCause::MalformedStream,
        ProviderCoreErrorKind::InvalidRetry => ModelProviderFailureCause::InvalidRetry,
        ProviderCoreErrorKind::Configuration => ModelProviderFailureCause::Configuration,
        ProviderCoreErrorKind::UnsupportedCapability => {
            ModelProviderFailureCause::UnsupportedCapability
        }
        ProviderCoreErrorKind::Unavailable => ModelProviderFailureCause::Unavailable,
        _ => ModelProviderFailureCause::Provider,
    }
}

const fn terminal_cause(category: FailureCategory) -> ModelProviderFailureCause {
    match category {
        FailureCategory::InvalidRequest => ModelProviderFailureCause::InvalidRequest,
        FailureCategory::Authentication => ModelProviderFailureCause::Authentication,
        FailureCategory::Permission => ModelProviderFailureCause::Permission,
        FailureCategory::NotFound => ModelProviderFailureCause::NotFound,
        FailureCategory::RateLimited => ModelProviderFailureCause::RateLimited,
        FailureCategory::QuotaExhausted => ModelProviderFailureCause::QuotaExhausted,
        FailureCategory::TransientProvider => ModelProviderFailureCause::TransientProvider,
        FailureCategory::Transport => ModelProviderFailureCause::Transport,
        FailureCategory::AmbiguousAcceptance => {
            ModelProviderFailureCause::AmbiguousAcceptance
        }
        FailureCategory::MalformedPayload => ModelProviderFailureCause::MalformedPayload,
        FailureCategory::IncompleteStream => ModelProviderFailureCause::IncompleteStream,
        FailureCategory::Timeout => ModelProviderFailureCause::Timeout,
        FailureCategory::Refusal => ModelProviderFailureCause::Refusal,
        FailureCategory::Safety => ModelProviderFailureCause::Safety,
        FailureCategory::Cancellation => ModelProviderFailureCause::Cancelled,
        FailureCategory::Provider => ModelProviderFailureCause::Provider,
        _ => ModelProviderFailureCause::Provider,
    }
}

const fn failure_phase(phase: TransportPhase) -> ModelFailurePhase {
    match phase {
        TransportPhase::BeforeSend => ModelFailurePhase::BeforeSend,
        TransportPhase::Connecting => ModelFailurePhase::Connecting,
        TransportPhase::SendingHeaders => ModelFailurePhase::SendingHeaders,
        TransportPhase::SendingBody => ModelFailurePhase::SendingBody,
        TransportPhase::AwaitingHeaders => ModelFailurePhase::AwaitingHeaders,
        TransportPhase::ReadingBody => ModelFailurePhase::ReadingBody,
        TransportPhase::StreamObserved => ModelFailurePhase::StreamObserved,
        TransportPhase::Completed => ModelFailurePhase::Completed,
    }
}

const fn acceptance(certainty: OutcomeCertainty) -> ModelAcceptanceCertainty {
    match certainty {
        OutcomeCertainty::DefinitelyNotAccepted => {
            ModelAcceptanceCertainty::DefinitelyNotAccepted
        }
        OutcomeCertainty::MaybeAccepted => ModelAcceptanceCertainty::MaybeAccepted,
        OutcomeCertainty::AcceptedPartial => ModelAcceptanceCertainty::AcceptedPartial,
        OutcomeCertainty::Terminal => ModelAcceptanceCertainty::Terminal,
    }
}
