//! Ordered C2 event projection into C4 progress.

use peritus_policy::AuthorityInstant;
use peritus_process::{OutputStream, ProcessEvent, ProcessEventKind};
use peritus_tool_protocol::{
    BoundedJson, JsonLimits, PreparedToolCall, ProgressKind, ToolProgress,
};

use crate::{json_value::object, render::checked_text};

pub(super) fn started(
    prepared: &PreparedToolCall,
    sequence: u32,
    observed_at: AuthorityInstant,
) -> Result<ToolProgress, peritus_tool_protocol::ProtocolError> {
    ToolProgress::new(
        prepared,
        sequence,
        ProgressKind::Started,
        observed_at,
        None,
        checked_text("execution accepted by C2".to_owned()),
    )
}

pub(super) fn event(
    prepared: &PreparedToolCall,
    sequence: u32,
    event: &ProcessEvent,
    observed_at: AuthorityInstant,
) -> Result<ToolProgress, peritus_tool_protocol::ProtocolError> {
    let (kind, label) = classify(event.kind());
    let structured = object([
        ("bytes", serde_json::Value::from(event.data().len())),
        ("kind", serde_json::Value::String(label.to_owned())),
        ("process_sequence", serde_json::Value::String(event.sequence().to_string())),
        (
            "stream_offset",
            event.stream_offset().map_or(serde_json::Value::Null, |value| {
                serde_json::Value::String(value.to_string())
            }),
        ),
    ]);
    let structured = BoundedJson::parse(&structured.to_string(), JsonLimits::PRODUCTION)?;
    ToolProgress::new(
        prepared,
        sequence,
        kind,
        observed_at,
        Some(structured),
        checked_text(format!("process event {}: {label}", event.sequence())),
    )
}

const fn classify(kind: &ProcessEventKind) -> (ProgressKind, &'static str) {
    match kind {
        ProcessEventKind::IntentPersisted => (ProgressKind::Started, "intent-persisted"),
        ProcessEventKind::SpawnAttempt => (ProgressKind::Started, "spawn-attempt"),
        ProcessEventKind::Started { .. } => (ProgressKind::Started, "started"),
        ProcessEventKind::Output(stream) => (ProgressKind::Update, stream_name(*stream)),
        ProcessEventKind::StdinAccepted { .. } => (ProgressKind::Control, "stdin-accepted"),
        ProcessEventKind::StdinClosed => (ProgressKind::Control, "stdin-closed"),
        ProcessEventKind::Resized(_) => (ProgressKind::Control, "terminal-resized"),
        ProcessEventKind::Signalled(_) => (ProgressKind::Control, "signal-delivered"),
        ProcessEventKind::Cancellation(_) => (ProgressKind::Stopping, "cancellation"),
        ProcessEventKind::Escalated => (ProgressKind::Stopping, "forced-stop"),
        ProcessEventKind::ResourceSample => (ProgressKind::Update, "resource-sample"),
        ProcessEventKind::ResourceLimit => (ProgressKind::Stopping, "resource-limit"),
        ProcessEventKind::SandboxObservation => (ProgressKind::Update, "sandbox-observation"),
        ProcessEventKind::OsExit => (ProgressKind::Update, "os-exit"),
        ProcessEventKind::TreeQuiescent => (ProgressKind::Update, "tree-quiescent"),
        ProcessEventKind::OutputClosed => (ProgressKind::Update, "output-closed"),
        ProcessEventKind::ArtifactPublished => (ProgressKind::Update, "artifact-published"),
        ProcessEventKind::TerminalPublished => (ProgressKind::Update, "terminal-published"),
    }
}

const fn stream_name(stream: OutputStream) -> &'static str {
    match stream {
        OutputStream::Stdout => "stdout",
        OutputStream::Stderr => "stderr",
        OutputStream::Terminal => "terminal",
    }
}
