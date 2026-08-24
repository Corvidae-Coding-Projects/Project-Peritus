//! Quality terminal, artifact, and candidate B2 evidence projection.

use core::fmt::Write;

use peritus_policy::AuthorityInstant;
use peritus_process::{OutputCompleteness, OutputStream, TerminalDisposition, TerminalResult};
use peritus_quality_policy::{GateFailure, GateOutcome};
use peritus_tool_protocol::{
    ArtifactCompleteness, ArtifactProvenance, ArtifactReference, BoundedJson, BoundedText,
    FailureCategory, JsonLimits, PreparedToolCall, RecoveryRoute, ResponsibleSubsystem,
    ResultStatus, Retryability, ToolFailure, ToolResult, ToolTiming, Truncation,
    TruncationMetadata,
};

use crate::{
    CheckDefinition,
    dispatcher::adapter_failure,
    json_value::object,
    observation::{CandidateGateObservation, QualityExecutionObservation, classify},
    render,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    prepared: &PreparedToolCall,
    definition: &CheckDefinition,
    terminal: &TerminalResult,
    parser_complete: bool,
    retained: &[u8],
    started_at: AuthorityInstant,
    finished_at: AuthorityInstant,
    progress_count: u32,
    progress_truncated: bool,
) -> Result<ToolResult, peritus_tool_router::DispatchFailure> {
    let (observation, candidate) = classify(definition, terminal, parser_complete);
    let structured = structured(&observation, candidate, progress_truncated)
        .map_err(|error| adapter_failure("quality-result-structure", &error.to_string()))?;
    let artifacts = artifacts(prepared, terminal)
        .map_err(|error| adapter_failure("quality-result-artifacts", &error.to_string()))?;
    let limits = prepared.call().limits();
    let (model, model_truncation) = render::output(retained, limits.model_bytes());
    let (human, human_truncation) = render::output(retained, limits.human_bytes());
    let timing = ToolTiming::new(started_at, finished_at)
        .map_err(|error| adapter_failure("quality-result-timing", &error.to_string()))?;
    let truncation = TruncationMetadata {
        output: output_truncation(terminal),
        model: model_truncation,
        human: human_truncation,
    };
    if candidate.outcome() == GateOutcome::Passed {
        ToolResult::success(
            prepared,
            structured,
            human,
            model,
            artifacts,
            timing,
            truncation,
            progress_count,
        )
        .map_err(|error| adapter_failure("quality-result-envelope", &error.to_string()))
    } else {
        let (status, failure) = quality_failure(candidate.outcome(), terminal);
        ToolResult::failure(
            prepared,
            status,
            failure,
            Some(structured),
            human,
            model,
            artifacts,
            timing,
            truncation,
            progress_count,
        )
        .map_err(|error| adapter_failure("quality-result-envelope", &error.to_string()))
    }
}

fn structured(
    observation: &QualityExecutionObservation,
    candidate: CandidateGateObservation,
    progress_truncated: bool,
) -> Result<BoundedJson, peritus_tool_protocol::ProtocolError> {
    let candidate = object([
        ("gate_id", serde_json::Value::String(hex(candidate.gate_id().as_bytes()))),
        ("outcome", serde_json::Value::String(outcome_name(candidate.outcome()).to_owned())),
        ("result_digest", serde_json::Value::String(hex(candidate.result_digest().as_bytes()))),
    ]);
    let execution = object([
        ("complete", serde_json::Value::Bool(observation.complete())),
        ("disposition", serde_json::Value::String(format!("{:?}", observation.disposition()))),
        ("os_exit", serde_json::Value::String(format!("{:?}", observation.os_exit()))),
        ("plan_digest", serde_json::Value::String(hex(observation.plan_digest().as_bytes()))),
        ("process_id", serde_json::Value::String(hex(observation.process_id().as_bytes()))),
    ]);
    let value = object([
        ("candidate", candidate),
        ("execution", execution),
        ("progress_truncated", serde_json::Value::Bool(progress_truncated)),
    ]);
    BoundedJson::parse(&value.to_string(), JsonLimits::PRODUCTION)
}

fn artifacts(
    prepared: &PreparedToolCall,
    terminal: &TerminalResult,
) -> Result<Vec<ArtifactReference>, peritus_tool_protocol::ProtocolError> {
    let provenance =
        ArtifactProvenance::new(prepared.call().action_id(), prepared.prepared_digest());
    terminal
        .artifacts()
        .iter()
        .map(|artifact| {
            ArtifactReference::new(
                artifact.digest(),
                artifact.size(),
                BoundedText::new("application/octet-stream".to_owned())?,
                BoundedText::new(stream_name(artifact.stream()).to_owned())?,
                match artifact.completeness() {
                    OutputCompleteness::Complete => ArtifactCompleteness::Complete,
                    OutputCompleteness::Truncated => ArtifactCompleteness::Truncated,
                    OutputCompleteness::Incomplete => ArtifactCompleteness::Indeterminate,
                },
                provenance,
            )
        })
        .collect()
}

fn quality_failure(outcome: GateOutcome, terminal: &TerminalResult) -> (ResultStatus, ToolFailure) {
    let (status, category, code, subsystem, retryability, recovery, detail) =
        match (terminal.disposition(), outcome) {
            (TerminalDisposition::TimedOut, _) => (
                ResultStatus::TimedOut,
                FailureCategory::Timeout,
                "quality-timeout",
                ResponsibleSubsystem::Process,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
                "the quality check deadline elapsed",
            ),
            (TerminalDisposition::Cancelled, _) => (
                ResultStatus::Cancelled,
                FailureCategory::Cancelled,
                "quality-cancelled",
                ResponsibleSubsystem::Process,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
                "the quality check was cancelled",
            ),
            (TerminalDisposition::RecoveryIndeterminate, _) => (
                ResultStatus::Indeterminate,
                FailureCategory::Indeterminate,
                "quality-recovery-indeterminate",
                ResponsibleSubsystem::Process,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileProcess,
                "C2 could not establish the quality process outcome",
            ),
            (_, GateOutcome::Failed(GateFailure::UnsuccessfulExit)) => (
                ResultStatus::Failed,
                FailureCategory::Execution,
                "quality-unsuccessful-exit",
                ResponsibleSubsystem::Tool,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
                "the frozen check exit predicate did not pass",
            ),
            (_, GateOutcome::Failed(GateFailure::InvalidResult)) => (
                ResultStatus::Failed,
                FailureCategory::Infrastructure,
                "quality-parser-invalid",
                ResponsibleSubsystem::Tool,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
                "the configured output parser did not complete",
            ),
            _ => (
                ResultStatus::Failed,
                FailureCategory::Infrastructure,
                "quality-infrastructure",
                ResponsibleSubsystem::Process,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileProcess,
                "complete trustworthy quality execution evidence is unavailable",
            ),
        };
    (
        status,
        ToolFailure::new(
            category,
            render::text(code),
            subsystem,
            retryability,
            recovery,
            render::text(detail),
        ),
    )
}

fn output_truncation(terminal: &TerminalResult) -> Truncation {
    if !terminal.artifact_publication_complete()
        || terminal
            .output()
            .streams()
            .iter()
            .any(|stream| stream.completeness() == OutputCompleteness::Incomplete)
    {
        Truncation::Indeterminate
    } else if terminal
        .output()
        .streams()
        .iter()
        .any(|stream| stream.completeness() == OutputCompleteness::Truncated)
    {
        Truncation::TailDropped
    } else {
        Truncation::Complete
    }
}

const fn outcome_name(value: GateOutcome) -> &'static str {
    match value {
        GateOutcome::Passed => "passed",
        GateOutcome::Failed(GateFailure::PredicateFailed) => "predicate-failed",
        GateOutcome::Failed(GateFailure::UnsuccessfulExit) => "unsuccessful-exit",
        GateOutcome::Failed(GateFailure::InvalidResult) => "invalid-result",
        GateOutcome::Failed(GateFailure::Infrastructure) => "infrastructure",
    }
}

const fn stream_name(stream: OutputStream) -> &'static str {
    match stream {
        OutputStream::Stdout => "stdout",
        OutputStream::Stderr => "stderr",
        OutputStream::Terminal => "terminal",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut value, byte| {
        write!(value, "{byte:02x}").expect("writing to a string cannot fail");
        value
    })
}
