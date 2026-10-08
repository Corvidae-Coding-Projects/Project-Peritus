//! Quality terminal, artifact, and candidate B2 evidence projection.

use core::fmt::Write;

use peritus_artifact_store::ArtifactStore;
use peritus_policy::AuthorityInstant;
use peritus_process::{OutputCompleteness, OutputStream, TerminalDisposition, TerminalResult};
use peritus_quality_policy::{GateFailure, GateOutcome};
use peritus_tool_protocol::{
    ArtifactCompleteness, ArtifactProvenance, ArtifactReference, BoundedJson, BoundedText,
    FailureCategory, PreparedToolCall, RecoveryRoute, ResponsibleSubsystem,
    ResultStatus, Retryability, ToolFailure, ToolResult, ToolTiming, Truncation,
    TruncationMetadata,
};

use crate::{
    CheckDefinition,
    dispatcher::adapter_failure,
    json_value::object,
    observation::{
        CandidateGateObservation, ClassifiedQualityExecution, QualityExecutionObservation,
        QualityTerminalClass, classify, classify_legacy,
    },
    render,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    prepared: &PreparedToolCall,
    definition: &CheckDefinition,
    terminal: &TerminalResult,
    parser_complete: bool,
    predicate_satisfied: bool,
    store: &ArtifactStore,
    started_at: AuthorityInstant,
    finished_at: AuthorityInstant,
    progress_count: u64,
    progress_truncated: bool,
) -> Result<ToolResult, peritus_tool_router::DispatchFailure> {
    build_classified(
        prepared,
        terminal,
        classify(definition, terminal, parser_complete, predicate_satisfied),
        store,
        started_at,
        finished_at,
        progress_count,
        progress_truncated,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_legacy(
    prepared: &PreparedToolCall,
    definition: &CheckDefinition,
    terminal: &TerminalResult,
    parser_complete: bool,
    predicate_satisfied: bool,
    store: &ArtifactStore,
    started_at: AuthorityInstant,
    finished_at: AuthorityInstant,
    progress_count: u64,
    progress_truncated: bool,
) -> Result<ToolResult, peritus_tool_router::DispatchFailure> {
    build_classified(
        prepared,
        terminal,
        classify_legacy(definition, terminal, parser_complete, predicate_satisfied),
        store,
        started_at,
        finished_at,
        progress_count,
        progress_truncated,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_classified(
    prepared: &PreparedToolCall,
    terminal: &TerminalResult,
    classified: ClassifiedQualityExecution,
    store: &ArtifactStore,
    started_at: AuthorityInstant,
    finished_at: AuthorityInstant,
    progress_count: u64,
    progress_truncated: bool,
    legacy: bool,
) -> Result<ToolResult, peritus_tool_router::DispatchFailure> {
    let candidate = classified.candidate();
    let structured = structured(
        prepared,
        classified.observation(),
        candidate,
        progress_truncated,
        legacy,
    )
    .map_err(|error| adapter_failure("quality-result-structure", &error.to_string()))?;
    let artifacts = artifacts(prepared, terminal)
        .map_err(|error| adapter_failure("quality-result-artifacts", &error.to_string()))?;
    let limits = prepared.call().limits();
    let rendering = render::output(
        store,
        terminal,
        limits.model_bytes(),
        limits.human_bytes(),
    )?;
    let timing = ToolTiming::new(started_at, finished_at)
        .map_err(|error| adapter_failure("quality-result-timing", &error.to_string()))?;
    let truncation = TruncationMetadata {
        output: output_truncation(terminal),
        model: rendering.model_truncation,
        human: rendering.human_truncation,
    };
    if classified.terminal_class() == QualityTerminalClass::Passed {
        ToolResult::success(
            prepared,
            structured,
            rendering.human,
            rendering.model,
            artifacts,
            timing,
            truncation,
            progress_count,
        )
        .map_err(|error| adapter_failure("quality-result-envelope", &error.to_string()))
    } else {
        let Some((status, failure)) = quality_failure(classified.terminal_class()) else {
            return Err(adapter_failure(
                "quality-result-classification",
                "a passing quality classification cannot construct a failure envelope",
            ));
        };
        ToolResult::failure(
            prepared,
            status,
            failure,
            Some(structured),
            rendering.human,
            rendering.model,
            artifacts,
            timing,
            truncation,
            progress_count,
        )
        .map_err(|error| adapter_failure("quality-result-envelope", &error.to_string()))
    }
}

fn structured(
    prepared: &PreparedToolCall,
    observation: &QualityExecutionObservation,
    candidate: CandidateGateObservation,
    progress_truncated: bool,
    legacy: bool,
) -> Result<BoundedJson, peritus_tool_protocol::ProtocolError> {
    let candidate = object([
        ("gate_id", serde_json::Value::String(hex(candidate.gate_id().as_bytes()))),
        ("outcome", serde_json::Value::String(outcome_name(candidate.outcome()).to_owned())),
        ("result_digest", serde_json::Value::String(hex(candidate.result_digest().as_bytes()))),
    ]);
    let execution = if legacy {
        object([
            ("complete", serde_json::Value::Bool(observation.complete())),
            ("disposition", serde_json::Value::String(format!("{:?}", observation.disposition()))),
            ("os_exit", serde_json::Value::String(format!("{:?}", observation.os_exit()))),
            ("plan_digest", serde_json::Value::String(hex(observation.plan_digest().as_bytes()))),
            ("process_id", serde_json::Value::String(hex(observation.process_id().as_bytes()))),
        ])
    } else {
        object([
            (
                "artifact_publication_complete",
                serde_json::Value::Bool(observation.artifact_publication_complete()),
            ),
            ("cleanup_complete", serde_json::Value::Bool(observation.cleanup_complete())),
            ("complete", serde_json::Value::Bool(observation.complete())),
            (
                "disposition",
                serde_json::Value::String(disposition_name(observation.disposition()).to_owned()),
            ),
            (
                "expected_exit_satisfied",
                serde_json::Value::Bool(observation.expected_exit_satisfied()),
            ),
            ("os_exit", serde_json::Value::String(format!("{:?}", observation.os_exit()))),
            ("output_complete", serde_json::Value::Bool(observation.output_complete())),
            ("parser_complete", serde_json::Value::Bool(observation.parser_complete())),
            ("plan_digest", serde_json::Value::String(hex(observation.plan_digest().as_bytes()))),
            (
                "predicate_satisfied",
                serde_json::Value::Bool(observation.predicate_satisfied()),
            ),
            ("process_id", serde_json::Value::String(hex(observation.process_id().as_bytes()))),
        ])
    };
    let value = if legacy {
        object([
            ("candidate", candidate),
            ("execution", execution),
            ("progress_truncated", serde_json::Value::Bool(progress_truncated)),
        ])
    } else {
        object([
            ("candidate", candidate),
            ("classification_version", serde_json::Value::Number(2_u64.into())),
            ("execution", execution),
            ("progress_truncated", serde_json::Value::Bool(progress_truncated)),
        ])
    };
    BoundedJson::parse(&value.to_string(), prepared.call().limits().json_limits())
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

fn quality_failure(classification: QualityTerminalClass) -> Option<(ResultStatus, ToolFailure)> {
    let (status, category, code, subsystem, retryability, recovery, detail) =
        match classification {
            QualityTerminalClass::Passed => return None,
            QualityTerminalClass::TimedOut => (
                ResultStatus::TimedOut,
                FailureCategory::Timeout,
                "quality-timeout",
                ResponsibleSubsystem::Process,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
                "the quality check deadline elapsed",
            ),
            QualityTerminalClass::Cancelled => (
                ResultStatus::Cancelled,
                FailureCategory::Cancelled,
                "quality-cancelled",
                ResponsibleSubsystem::Process,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
                "the quality check was cancelled",
            ),
            QualityTerminalClass::Indeterminate => (
                ResultStatus::Indeterminate,
                FailureCategory::Indeterminate,
                "quality-recovery-indeterminate",
                ResponsibleSubsystem::Process,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileProcess,
                "C2 could not establish the quality process outcome",
            ),
            QualityTerminalClass::PredicateFailed => (
                ResultStatus::Failed,
                FailureCategory::Execution,
                "quality-predicate-failed",
                ResponsibleSubsystem::Tool,
                Retryability::Never,
                RecoveryRoute::None,
                "the frozen check success predicate did not pass",
            ),
            QualityTerminalClass::UnsuccessfulExit => (
                ResultStatus::Failed,
                FailureCategory::Execution,
                "quality-unsuccessful-exit",
                ResponsibleSubsystem::Tool,
                Retryability::Never,
                RecoveryRoute::None,
                "the frozen check exit predicate did not pass",
            ),
            QualityTerminalClass::InvalidResult => (
                ResultStatus::Failed,
                FailureCategory::Infrastructure,
                "quality-parser-invalid",
                ResponsibleSubsystem::Tool,
                Retryability::NewAction,
                RecoveryRoute::Reauthorize,
                "the configured output parser did not complete",
            ),
            QualityTerminalClass::Infrastructure => (
                ResultStatus::Failed,
                FailureCategory::Infrastructure,
                "quality-infrastructure",
                ResponsibleSubsystem::Process,
                Retryability::AfterRecovery,
                RecoveryRoute::ReconcileProcess,
                "complete trustworthy quality execution evidence is unavailable",
            ),
        };
    Some((
        status,
        ToolFailure::new(
            category,
            render::text(code),
            subsystem,
            retryability,
            recovery,
            render::text(detail),
        ),
    ))
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

const fn disposition_name(value: TerminalDisposition) -> &'static str {
    match value {
        TerminalDisposition::Exited => "exited",
        TerminalDisposition::Signalled => "signalled",
        TerminalDisposition::SpawnFailed => "spawn-failed",
        TerminalDisposition::Cancelled => "cancelled",
        TerminalDisposition::TimedOut => "timed-out",
        TerminalDisposition::OutputLimit => "output-limit",
        TerminalDisposition::ResourceLimit => "resource-limit",
        TerminalDisposition::SandboxDenied => "sandbox-denied",
        TerminalDisposition::SupervisorFailed => "supervisor-failed",
        TerminalDisposition::RecoveryIndeterminate => "recovery-indeterminate",
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
