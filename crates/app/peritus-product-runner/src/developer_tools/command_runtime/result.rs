//! Product-facing projection of C4 progress and terminal results.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, StoreConfig};
use peritus_process::{
    OsExitObservation, OutputCompleteness, OutputStream, TerminalDisposition, TerminalRecovery,
    TerminalResult,
};
use peritus_tool_protocol::{ArtifactCompleteness, ResultStatus, ToolProgress, ToolResult};
use serde_json::Value;

use crate::developer_tools::wire::object;

pub(super) fn active(handle: &str, progress: &[ToolProgress]) -> Value {
    object(vec![
        ("handle", Value::String(handle.to_owned())),
        ("state", Value::String("running".to_owned())),
        ("success", Value::Bool(true)),
        ("progress", Value::Array(progress_values(progress))),
    ])
}

pub(super) fn terminal(
    handle: &str,
    result: &ToolResult,
    artifact_config: &StoreConfig,
    progress: &[ToolProgress],
) -> Result<Value, String> {
    let store = ArtifactStore::open(artifact_config.clone())
        .map_err(|error| format!("reopen command artifact store: {error}"))?;
    let mut stdout = String::new();
    let mut stderr = String::new();
    for artifact in result.artifacts() {
        let bytes = store
            .read(ArtifactDigest::from_sha256(artifact.digest()), artifact.size())
            .map_err(|error| format!("read command output artifact: {error}"))?;
        let output = output_text(&bytes, artifact.completeness() != ArtifactCompleteness::Complete);
        match artifact.label().as_str() {
            "stdout" | "terminal" => stdout = output,
            "stderr" => stderr = output,
            _ => {}
        }
    }
    let status = status_name(result.status());
    let structured = result
        .structured()
        .and_then(|value| serde_json::from_slice::<Value>(value.canonical_bytes()).ok())
        .unwrap_or(Value::Null);
    let exit_code = structured
        .get("os_exit")
        .and_then(Value::as_str)
        .and_then(|value| value.strip_prefix("code:"))
        .and_then(|value| value.parse::<i64>().ok())
        .map_or(Value::Null, Value::from);
    let disposition =
        structured.get("disposition").and_then(Value::as_str).unwrap_or("unknown").to_owned();
    let failure = result.failure_value().map_or(Value::Null, |failure| {
        object(vec![
            ("category", Value::String(format!("{:?}", failure.category()).to_ascii_lowercase())),
            ("code", Value::String(failure.code().as_str().to_owned())),
            ("detail", Value::String(failure.detail().as_str().to_owned())),
            ("recovery", Value::String(format!("{:?}", failure.recovery()).to_ascii_lowercase())),
            (
                "retryability",
                Value::String(format!("{:?}", failure.retryability()).to_ascii_lowercase()),
            ),
        ])
    });
    Ok(object(vec![
        ("disposition", Value::String(disposition)),
        ("exit_code", exit_code),
        ("failure", failure),
        ("handle", Value::String(handle.to_owned())),
        ("progress", Value::Array(progress_values(progress))),
        ("state", Value::String("completed".to_owned())),
        ("status", Value::String(status.to_owned())),
        ("stderr", Value::String(stderr)),
        ("stdout", Value::String(stdout)),
        ("success", Value::Bool(result.status() == ResultStatus::Succeeded)),
        ("timed_out", Value::Bool(result.status() == ResultStatus::TimedOut)),
        ("tool_result", structured),
    ]))
}

pub(super) fn indeterminate(handle: &str, detail: &str) -> Value {
    object(vec![
        ("error", Value::String(detail.to_owned())),
        ("handle", Value::String(handle.to_owned())),
        ("state", Value::String("indeterminate".to_owned())),
        ("success", Value::Bool(false)),
    ])
}

pub(super) fn terminal_from_durable_owner(
    handle: &str,
    terminal: &TerminalResult,
    artifact_config: &StoreConfig,
) -> Result<Value, String> {
    let (stdout, stderr) = durable_output(terminal, artifact_config)?;
    let status = durable_status(terminal);
    let timed_out = terminal.disposition() == TerminalDisposition::TimedOut;
    let exit_code = match terminal.os_exit() {
        OsExitObservation::Code(code) => Value::from(*code),
        _ => Value::Null,
    };
    let disposition = disposition_name(terminal.disposition());
    let tool_result = object(vec![
        ("artifact_publication_complete", Value::Bool(terminal.artifact_publication_complete())),
        ("disposition", Value::String(disposition.to_owned())),
        ("os_exit", Value::String(exit_name(terminal.os_exit()))),
        ("process_id", Value::String(hex(terminal.process_id().as_bytes()))),
        ("progress_truncated", Value::Bool(false)),
        ("recovery", Value::String(format!("{:?}", terminal.recovery()))),
        ("streams", Value::Array(durable_stream_values(terminal))),
        ("support_tasks_joined", Value::Bool(terminal.support_tasks_joined())),
        ("tree_cleanup_complete", Value::Bool(terminal.tree_cleanup_complete())),
    ]);
    Ok(object(vec![
        ("disposition", Value::String(disposition.to_owned())),
        ("exit_code", exit_code),
        ("failure", durable_failure(status)),
        ("handle", Value::String(handle.to_owned())),
        ("progress", Value::Array(Vec::new())),
        ("state", Value::String("completed".to_owned())),
        ("status", Value::String(status.to_owned())),
        ("stderr", Value::String(stderr)),
        ("stdout", Value::String(stdout)),
        ("success", Value::Bool(status == "succeeded")),
        ("timed_out", Value::Bool(timed_out)),
        ("tool_result", tool_result),
    ]))
}

fn durable_output(
    terminal: &TerminalResult,
    artifact_config: &StoreConfig,
) -> Result<(String, String), String> {
    let store = ArtifactStore::open(artifact_config.clone())
        .map_err(|error| format!("reopen command artifact store: {error}"))?;
    let mut stdout = String::new();
    let mut stderr = String::new();
    for artifact in terminal.artifacts() {
        let bytes = store
            .read(ArtifactDigest::from_sha256(artifact.digest()), artifact.size())
            .map_err(|error| format!("read command output artifact: {error}"))?;
        let output = output_text(&bytes, artifact.completeness() != OutputCompleteness::Complete);
        match artifact.stream() {
            OutputStream::Stdout | OutputStream::Terminal => stdout = output,
            OutputStream::Stderr => stderr = output,
        }
    }
    Ok((stdout, stderr))
}

pub(super) fn durable_output_contains(
    terminal: &TerminalResult,
    artifact_config: &StoreConfig,
    needle: &str,
) -> Result<bool, String> {
    let store = ArtifactStore::open(artifact_config.clone())
        .map_err(|error| format!("reopen command artifact store: {error}"))?;
    for artifact in terminal.artifacts() {
        let bytes = store
            .read(ArtifactDigest::from_sha256(artifact.digest()), artifact.size())
            .map_err(|error| format!("read command output artifact: {error}"))?;
        if String::from_utf8_lossy(&bytes).contains(needle) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn durable_status(terminal: &TerminalResult) -> &'static str {
    if !terminal.tree_cleanup_complete()
        || !terminal.support_tasks_joined()
        || terminal.recovery() == TerminalRecovery::Indeterminate
    {
        "indeterminate"
    } else if !terminal.artifact_publication_complete()
        || terminal
            .output()
            .streams()
            .iter()
            .any(|stream| stream.completeness() == OutputCompleteness::Incomplete)
    {
        "failed"
    } else {
        match (terminal.disposition(), terminal.os_exit()) {
            (TerminalDisposition::Exited, OsExitObservation::Code(0)) => "succeeded",
            (TerminalDisposition::TimedOut, _) => "timed_out",
            (TerminalDisposition::Cancelled, _) => "cancelled",
            _ => "failed",
        }
    }
}

fn durable_stream_values(terminal: &TerminalResult) -> Vec<Value> {
    terminal
        .output()
        .streams()
        .iter()
        .map(|stream| {
            object(vec![
                (
                    "completeness",
                    Value::String(format!("{:?}", stream.completeness()).to_ascii_lowercase()),
                ),
                ("dropped", Value::String(stream.dropped().to_string())),
                ("observed", Value::String(stream.observed().to_string())),
                ("retained", Value::String(stream.retained().to_string())),
                (
                    "stream",
                    Value::String(
                        match stream.stream() {
                            OutputStream::Stdout => "stdout",
                            OutputStream::Stderr => "stderr",
                            OutputStream::Terminal => "terminal",
                        }
                        .to_owned(),
                    ),
                ),
            ])
        })
        .collect()
}

const fn disposition_name(disposition: TerminalDisposition) -> &'static str {
    match disposition {
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

fn durable_failure(status: &str) -> Value {
    if status == "succeeded" {
        return Value::Null;
    }
    object(vec![
        (
            "category",
            Value::String(
                if status == "indeterminate" { "indeterminate" } else { "execution" }.to_owned(),
            ),
        ),
        (
            "detail",
            Value::String("Recovered from the exact durable native command owner.".to_owned()),
        ),
    ])
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

fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write as _;

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn progress_values(progress: &[ToolProgress]) -> Vec<Value> {
    progress
        .iter()
        .map(|event| {
            object(vec![
                ("kind", Value::String(format!("{:?}", event.kind()).to_ascii_lowercase())),
                ("message", Value::String(event.model_rendering().as_str().to_owned())),
                ("sequence", Value::from(event.sequence())),
            ])
        })
        .collect()
}

const fn status_name(status: ResultStatus) -> &'static str {
    match status {
        ResultStatus::Succeeded => "succeeded",
        ResultStatus::Failed => "failed",
        ResultStatus::Cancelled => "cancelled",
        ResultStatus::TimedOut => "timed_out",
        ResultStatus::Indeterminate => "indeterminate",
    }
}

fn output_text(bytes: &[u8], incomplete: bool) -> String {
    let text = String::from_utf8_lossy(bytes);
    if incomplete {
        format!(
            "{text}\n[retained output artifact is incomplete; see structured stream accounting]\n"
        )
    } else {
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_stream_projection_has_no_arbitrary_midstream_cap() {
        let bytes = vec![b'x'; 2 * 1024 * 1024];
        let value = output_text(&bytes, false);
        assert_eq!(value.len(), bytes.len());
        assert!(value.bytes().all(|byte| byte == b'x'));
    }

    #[test]
    fn incomplete_artifact_remains_explicitly_marked() {
        let value = output_text(b"retained", true);
        assert!(value.starts_with("retained"));
        assert!(value.contains("artifact is incomplete"));
    }
}
