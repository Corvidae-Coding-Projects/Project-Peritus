//! Bounded pull-based provider execution and strict terminal reduction.

use peritus_model_protocol::{FinishReason, ReducedItem, ResponseReducer, TerminalOutcome};
use peritus_provider_core::{CancellationToken, ModelProvider, validate_request_profile};
use peritus_types::Sha256Digest;

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery, TraceSelectionManifest,
    ValidatedModelProposal,
};

use super::ModelAnalysisPlan;

#[cfg(test)]
mod tests;

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
    /// Validated proposal.
    #[must_use]
    pub const fn proposal(&self) -> &ValidatedModelProposal {
        &self.proposal
    }
    /// Canonical structured-item digest.
    #[must_use]
    pub const fn output_digest(&self) -> Sha256Digest {
        self.output_digest
    }
    /// Canonical structured-item bytes.
    #[must_use]
    pub const fn output_bytes(&self) -> u64 {
        self.output_bytes
    }
    /// Normalized event count.
    #[must_use]
    pub const fn event_count(&self) -> u64 {
        self.event_count
    }
    /// Provider input-token high water, or zero when unreported.
    #[must_use]
    pub const fn input_tokens(&self) -> u64 {
        self.input_tokens
    }
    /// Provider output-token high water, or zero when unreported.
    #[must_use]
    pub const fn output_tokens(&self) -> u64 {
        self.output_tokens
    }
    /// Provider total-token high water, or zero when unreported.
    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.total_tokens
    }
}

/// Runs one already-durable model attempt through the C5 provider boundary.
///
/// Exactly one structured item and a normal stop are accepted. Text, tool calls, reasoning,
/// refusals, native items, incomplete/cancelled/action terminals, malformed streams, profile
/// drift, and every budget overrun fail closed.
///
/// # Errors
/// Returns a typed redaction-safe provider/protocol/cancellation/budget/rejection error.
pub async fn run_model_analysis(
    provider: &dyn ModelProvider,
    plan: &ModelAnalysisPlan,
    manifest: &TraceSelectionManifest,
    debugger_limits: crate::DebuggerLimits,
    cancellation: CancellationToken,
) -> Result<ModelRunSuccess, DebuggerError> {
    if manifest.id() != plan.manifest_id() || manifest.digest() != plan.manifest_digest() {
        return Err(rejected("model plan and selection manifest differ"));
    }
    validate_request_profile(provider.profile(), plan.request()).map_err(provider_error)?;
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }
    let mut reducer =
        ResponseReducer::new(plan.request().provider().clone(), plan.protocol_limits());
    let mut stream = provider
        .start(plan.request().clone(), cancellation.clone())
        .await
        .map_err(provider_error)?;
    let mut event_count = 0_u64;
    while reducer.terminal().is_none() {
        if cancellation.is_cancelled() {
            stream.cancel();
            return Err(cancelled());
        }
        let envelope = stream.pull().await.map_err(provider_error)?.ok_or_else(|| {
            protocol_error("provider stream closed without a normalized terminal")
        })?;
        event_count = event_count
            .checked_add(1)
            .ok_or_else(|| budget_error("normalized model event count overflowed"))?;
        if event_count > plan.budget().max_events() {
            stream.cancel();
            return Err(budget_error("normalized model event budget was exceeded"));
        }
        reducer.push(envelope).map_err(protocol)?;
    }
    let terminal = reducer
        .terminal()
        .ok_or_else(|| protocol_error("model reducer has no terminal outcome"))?;
    if !matches!(terminal, TerminalOutcome::Succeeded { reason: FinishReason::Stop }) {
        return Err(rejected("model response did not end in a normal successful stop"));
    }
    let [ReducedItem::Structured { value, .. }] = reducer.completed_items() else {
        return Err(rejected(
            "model response must contain exactly one structured item and nothing else",
        ));
    };
    let output_bytes = u64::try_from(value.canonical_bytes().len())
        .map_err(|_| budget_error("structured model output length cannot be represented"))?;
    let usage = reducer.usage_high_water();
    let input_tokens = usage.input_tokens().unwrap_or(0);
    let output_tokens = usage.output_tokens().unwrap_or(0);
    let total_tokens = usage.total_tokens().unwrap_or(0);
    let budget = plan.budget();
    if output_bytes > budget.max_output_bytes()
        || input_tokens > budget.max_input_tokens()
        || output_tokens > budget.max_output_tokens()
        || total_tokens > budget.max_total_tokens()
    {
        return Err(budget_error("model output or usage exceeds the frozen attempt budget"));
    }
    let proposal = ValidatedModelProposal::validate(
        value,
        manifest,
        plan.deterministic_digest(),
        debugger_limits,
    )?;
    Ok(ModelRunSuccess {
        proposal,
        output_digest: value.digest(),
        output_bytes,
        event_count,
        input_tokens,
        output_tokens,
        total_tokens,
    })
}

fn protocol(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::ModelProtocol,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::Retry,
        error.to_string(),
    )
}
fn provider_error(error: impl core::fmt::Display) -> DebuggerError {
    protocol(error)
}
fn protocol_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::ModelProtocol,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::Retry,
        detail,
    )
}
fn rejected(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::ModelRejected,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::None,
        detail,
    )
}
fn budget_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Budget,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::None,
        detail,
    )
}
fn cancelled() -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Cancelled,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::None,
        "model analysis was cancelled",
    )
}
