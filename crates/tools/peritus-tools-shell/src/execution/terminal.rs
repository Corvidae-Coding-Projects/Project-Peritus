//! Truthful C2 terminal and artifact projection.

use core::fmt::Write;

use peritus_policy::AuthorityInstant;
use peritus_process::{
    OsExitObservation, OutputCompleteness, OutputStream, TerminalDisposition, TerminalResult,
};
use peritus_tool_protocol::{
    ArtifactCompleteness, ArtifactProvenance, ArtifactReference, BoundedJson, BoundedText,
    FailureCategory, JsonLimits, PreparedToolCall, RecoveryRoute, ResponsibleSubsystem,
    ResultStatus, Retryability, ToolFailure, ToolResult, ToolTiming, Truncation,
    TruncationMetadata,
};

use crate::render::{self, checked_text};
use crate::{execution::failure, json_value::object};

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    prepared: &PreparedToolCall,
    terminal: &TerminalResult,
    retained: &[u8],
    started_at: AuthorityInstant,
    finished_at: AuthorityInstant,
    progress_count: u32,
    progress_truncated: bool,
) -> Result<ToolResult, peritus_tool_router::DispatchFailure> {
    let limits = prepared.call().limits();
    let rendering = render::output(retained, limits.model_bytes(), limits.human_bytes());
    let artifacts = artifacts(prepared, terminal)
        .map_err(|error| failure::adapter("shell-artifact-envelope", &error.to_string()))?;
    let structured = structured(terminal, progress_truncated)
        .map_err(|error| failure::adapter("shell-terminal-envelope", &error.to_string()))?;
    let timing = ToolTiming::new(started_at, finished_at)
        .map_err(|error| failure::adapter("shell-terminal-timing", &error.to_string()))?;
    let output_truncation = output_truncation(terminal);
    let truncation = TruncationMetadata {
        output: output_truncation,
        model: rendering.model_truncation,
        human: rendering.human_truncation,
    };
    match classify(terminal) {
        None => ToolResult::success(
            prepared,
            structured,
            rendering.human,
            rendering.model,
            artifacts,
            timing,
            truncation,
            progress_count,
        ),
        Some((status, category, code, subsystem, retryability, recovery, detail)) => {
            ToolResult::failure(
                prepared,
                status,
                ToolFailure::new(
                    category,
                    checked_text(code.to_owned()),
                    subsystem,
                    retryability,
                    recovery,
                    checked_text(detail.to_owned()),
                ),
                Some(structured),
                rendering.human,
                rendering.model,
                artifacts,
                timing,
                truncation,
                progress_count,
            )
        }
    }
    .map_err(|error| failure::adapter("shell-result-envelope", &error.to_string()))
}

fn structured(
    terminal: &TerminalResult,
    progress_truncated: bool,
) -> Result<BoundedJson, peritus_tool_protocol::ProtocolError> {
    let streams: Vec<_> = terminal
        .output()
        .streams()
        .iter()
        .map(|stream| {
            object([
                (
                    "completeness",
                    serde_json::Value::String(completeness_name(stream.completeness()).to_owned()),
                ),
                ("dropped", serde_json::Value::String(stream.dropped().to_string())),
                ("observed", serde_json::Value::String(stream.observed().to_string())),
                ("retained", serde_json::Value::String(stream.retained().to_string())),
                ("stream", serde_json::Value::String(stream_name(stream.stream()).to_owned())),
            ])
        })
        .collect();
    let resources: Vec<_> = terminal
        .resources()
        .iter()
        .map(|resource| {
            object([
                ("ceiling", serde_json::Value::String(resource.ceiling().to_string())),
                ("dimension", serde_json::Value::String(format!("{:?}", resource.dimension()))),
                ("fidelity", serde_json::Value::String(format!("{:?}", resource.fidelity()))),
                ("value", serde_json::Value::String(resource.value().to_string())),
            ])
        })
        .collect();
    let value = object([
        (
            "artifact_publication_complete",
            serde_json::Value::Bool(terminal.artifact_publication_complete()),
        ),
        (
            "disposition",
            serde_json::Value::String(disposition_name(terminal.disposition()).to_owned()),
        ),
        ("os_exit", serde_json::Value::String(exit_name(terminal.os_exit()))),
        ("plan_digest", serde_json::Value::String(hex(terminal.plan_digest().as_bytes()))),
        ("process_id", serde_json::Value::String(hex(terminal.process_id().as_bytes()))),
        ("progress_truncated", serde_json::Value::Bool(progress_truncated)),
        ("recovery", serde_json::Value::String(format!("{:?}", terminal.recovery()))),
        ("resources", serde_json::Value::Array(resources)),
        ("streams", serde_json::Value::Array(streams)),
        ("support_tasks_joined", serde_json::Value::Bool(terminal.support_tasks_joined())),
        ("tree_cleanup_complete", serde_json::Value::Bool(terminal.tree_cleanup_complete())),
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

type Failure = (
    ResultStatus,
    FailureCategory,
    &'static str,
    ResponsibleSubsystem,
    Retryability,
    RecoveryRoute,
    &'static str,
);

fn classify(terminal: &TerminalResult) -> Option<Failure> {
    if !terminal.tree_cleanup_complete() || !terminal.support_tasks_joined() {
        return Some((
            ResultStatus::Indeterminate,
            FailureCategory::Indeterminate,
            "process-cleanup-indeterminate",
            ResponsibleSubsystem::Process,
            Retryability::AfterRecovery,
            RecoveryRoute::ReconcileProcess,
            "the owned process tree or support tasks did not reach complete cleanup",
        ));
    }
    if !terminal.artifact_publication_complete()
        || has_incomplete_output(
            terminal.output().streams().iter().map(|stream| stream.completeness()),
        )
    {
        return Some((
            ResultStatus::Failed,
            FailureCategory::Artifact,
            "process-output-incomplete",
            ResponsibleSubsystem::ArtifactStore,
            Retryability::AfterRecovery,
            RecoveryRoute::RepublishArtifact,
            "complete process output or artifact publication is unavailable",
        ));
    }
    match (terminal.disposition(), terminal.os_exit()) {
        (TerminalDisposition::Exited, OsExitObservation::Code(0)) => None,
        (TerminalDisposition::TimedOut, _) => Some((
            ResultStatus::TimedOut,
            FailureCategory::Timeout,
            "process-timeout",
            ResponsibleSubsystem::Process,
            Retryability::NewAction,
            RecoveryRoute::Reauthorize,
            "the immutable execution deadline elapsed",
        )),
        (TerminalDisposition::Cancelled, _) => Some((
            ResultStatus::Cancelled,
            FailureCategory::Cancelled,
            "process-cancelled",
            ResponsibleSubsystem::Process,
            Retryability::NewAction,
            RecoveryRoute::Reauthorize,
            "the owned execution was cancelled",
        )),
        (TerminalDisposition::RecoveryIndeterminate, _) => Some((
            ResultStatus::Indeterminate,
            FailureCategory::Indeterminate,
            "process-recovery-indeterminate",
            ResponsibleSubsystem::Process,
            Retryability::AfterRecovery,
            RecoveryRoute::ReconcileProcess,
            "C2 could not establish an exact process outcome",
        )),
        (TerminalDisposition::SandboxDenied, _) => Some((
            ResultStatus::Failed,
            FailureCategory::Execution,
            "sandbox-denied",
            ResponsibleSubsystem::Sandbox,
            Retryability::NewAction,
            RecoveryRoute::Reauthorize,
            "the native sandbox denied or failed the execution",
        )),
        (TerminalDisposition::OutputLimit, _) => Some((
            ResultStatus::Failed,
            FailureCategory::Artifact,
            "process-output-limit",
            ResponsibleSubsystem::Process,
            Retryability::NewAction,
            RecoveryRoute::Reauthorize,
            "the configured output limit terminated the process",
        )),
        _ => Some((
            ResultStatus::Failed,
            FailureCategory::Execution,
            "process-unsuccessful",
            ResponsibleSubsystem::Process,
            Retryability::NewAction,
            RecoveryRoute::Reauthorize,
            "the process did not exit successfully",
        )),
    }
}

fn has_incomplete_output(values: impl IntoIterator<Item = OutputCompleteness>) -> bool {
    values.into_iter().any(|value| value == OutputCompleteness::Incomplete)
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

fn exit_name(value: &OsExitObservation) -> String {
    match value {
        OsExitObservation::Code(code) => format!("code:{code}"),
        OsExitObservation::Signal(signal) => format!("signal:{signal}"),
        OsExitObservation::SignalName(signal) => format!("signal:{signal}"),
        OsExitObservation::PlatformException(code) => format!("exception:{code}"),
        OsExitObservation::Unavailable => "unavailable".to_owned(),
    }
}

const fn stream_name(stream: OutputStream) -> &'static str {
    match stream {
        OutputStream::Stdout => "stdout",
        OutputStream::Stderr => "stderr",
        OutputStream::Terminal => "terminal",
    }
}

const fn completeness_name(value: OutputCompleteness) -> &'static str {
    match value {
        OutputCompleteness::Complete => "complete",
        OutputCompleteness::Truncated => "truncated",
        OutputCompleteness::Incomplete => "incomplete",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut value, byte| {
        write!(value, "{byte:02x}").expect("writing to a string cannot fail");
        value
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_and_script_accept_counted_truncation_but_not_incomplete_output() {
        for descriptor in [crate::exec_descriptor(), crate::script_descriptor()] {
            let descriptor = descriptor.expect("shell descriptor");
            assert!(matches!(descriptor.name().as_str(), "shell.exec" | "shell.script"));
            assert!(!has_incomplete_output([
                OutputCompleteness::Complete,
                OutputCompleteness::Truncated,
            ]));
            assert!(has_incomplete_output([OutputCompleteness::Incomplete]));
        }
    }
}
