//! Local read of one run's durable private provider trace.
//!
//! The trace is the daemon's private record: it retains provider envelopes, retry decisions and
//! rejected model replies that no public activity carries. Reading it needs no daemon session.

use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use peritus_model_protocol::{ModelEvent, ProtocolLimits, decode_event_envelope};
use peritus_product_runner::{DeveloperTraceFrameKind, TraceFrame, TraceReadError, read_frames};
use peritus_types::RunId;
use serde_json::Value;

use crate::{error::CliError, id::hex, output::Output};

/// One described frame: whether it is a provider extension, and its human and JSON forms.
struct Described {
    provider_extension: bool,
    json: Value,
    human: String,
}

pub(crate) fn execute(
    run_id: Option<RunId>,
    extensions_only: bool,
    output: &Output,
) -> Result<(), CliError> {
    let layout = peritus_launcher::AppLayout::discover()
        .map_err(|error| CliError::runtime("find the local Peritus state", error.to_string()))?;
    let root = layout.daemon_product_runs_root();
    let Some(run_id) = run_id else {
        return list(&root, output);
    };
    let path = root.join(format!("{}.trace", hex(run_id.as_bytes())));
    if !path.is_file() {
        return Err(CliError::local_io(
            "read the run private trace",
            Some(path),
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("run {} has no retained private trace", hex(run_id.as_bytes())),
            ),
        ));
    }
    let frames = read_frames(&path).map_err(|error| match error {
        TraceReadError::Read(error) => {
            CliError::local_io("read the run private trace", Some(path.clone()), error)
        }
        TraceReadError::Framing(detail) => {
            CliError::protocol("decode the run private trace", detail)
        }
    })?;
    let mut human =
        format!("run {}  {} records  {}\n", hex(run_id.as_bytes()), frames.len(), path.display());
    let mut json = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let described = describe(index + 1, frame)?;
        if extensions_only && !described.provider_extension {
            continue;
        }
        human.push_str(&described.human);
        human.push('\n');
        json.push(described.json);
    }
    output.success("product-run-trace", Value::Array(json), &human)
}

/// Lists retained run traces newest first so an operator can find a run identifier locally.
fn list(root: &Path, output: &Output) -> Result<(), CliError> {
    let entries = fs::read_dir(root).map_err(|error| {
        CliError::local_io("list retained run traces", Some(root.to_path_buf()), error)
    })?;
    let mut runs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            CliError::local_io("list retained run traces", Some(root.to_path_buf()), error)
        })?;
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) != Some("trace") {
            continue;
        }
        let Some(identity) = path.file_stem().and_then(OsStr::to_str) else {
            continue;
        };
        if identity.len() != 32 || !identity.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| {
            CliError::local_io("inspect a retained run trace", Some(path.clone()), error)
        })?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |value| value.as_secs());
        runs.push(RetainedRun {
            identity: identity.to_owned(),
            bytes: metadata.len(),
            modified,
            path,
        });
    }
    runs.sort_by(|left, right| {
        right.modified.cmp(&left.modified).then_with(|| left.identity.cmp(&right.identity))
    });
    let mut human =
        format!("retained run traces  {}  newest first  {}\n", runs.len(), root.display());
    let json = runs
        .iter()
        .map(|run| {
            human.push_str(&format!(
                "{}  {} bytes  {}\n",
                run.identity,
                run.bytes,
                run.path.display()
            ));
            serde_json::json!({
                "run_id": run.identity,
                "bytes": run.bytes,
                "modified_unix_seconds": run.modified,
                "path": run.path.display().to_string(),
            })
        })
        .collect::<Vec<_>>();
    output.success("product-run-traces", Value::Array(json), &human)
}

/// One trace discovered by [`list`].
struct RetainedRun {
    identity: String,
    bytes: u64,
    modified: u64,
    path: PathBuf,
}

fn describe(index: usize, frame: &TraceFrame) -> Result<Described, CliError> {
    let kind = frame.kind();
    if kind != DeveloperTraceFrameKind::ProviderEnvelope {
        let (value, text) = payload(frame.payload());
        return Ok(Described {
            provider_extension: false,
            json: serde_json::json!({
                "index": index, "kind": kind_name(kind), "payload": value,
            }),
            human: format!("{index:>6}  {:<22}  {text}", kind_name(kind)),
        });
    }
    let envelope =
        decode_event_envelope(frame.payload(), ProtocolLimits::PRODUCTION).map_err(|error| {
            CliError::protocol(
                "decode a retained provider envelope",
                format!("record {index}: {error}"),
            )
        })?;
    let event = envelope.event();
    if let ModelEvent::ProviderEvent(extension) = event {
        let (value, text) = payload(extension.value().canonical_bytes());
        let name = extension.name().as_str().to_owned();
        return Ok(Described {
            provider_extension: true,
            json: serde_json::json!({
                "index": index, "kind": "provider-envelope", "event": "extension",
                "extension": name, "value": value,
            }),
            human: format!("{index:>6}  {:<22}  extension {name} {text}", kind_name(kind)),
        });
    }
    let summary = event_summary(event);
    Ok(Described {
        provider_extension: false,
        json: serde_json::json!({
            "index": index, "kind": "provider-envelope", "event": summary.0, "detail": summary.1,
        }),
        human: format!("{index:>6}  {:<22}  {} {}", kind_name(kind), summary.0, summary.1),
    })
}

fn payload(bytes: &[u8]) -> (Value, String) {
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        let text = serde_json::to_string(&value).unwrap_or_default();
        return (value, text);
    }
    let text = String::from_utf8_lossy(bytes).into_owned();
    (Value::String(text.clone()), text)
}

fn event_summary(event: &ModelEvent) -> (&'static str, String) {
    use ModelEvent as E;
    match event {
        E::ResponseStarted { model, .. } => (
            "response-started",
            model.as_ref().map_or_else(String::new, |model| model.as_str().to_owned()),
        ),
        E::ResponseIdentity(response_id) => {
            ("response-identity", response_id.expose_for_wire().to_owned())
        }
        E::ItemStarted { item_id, index, kind } => {
            ("item-started", format!("{index} {kind:?} {}", item_id.expose_for_wire()))
        }
        E::TextDelta { item_id, fragment } => {
            ("text-delta", format!("{} {} bytes", item_id.expose_for_wire(), fragment.len()))
        }
        E::ReasoningSummaryDelta { item_id, fragment } => (
            "reasoning-summary-delta",
            format!("{} {} bytes", item_id.expose_for_wire(), fragment.len()),
        ),
        E::ReasoningReplayDelta { item_id, fragment } => (
            "reasoning-replay-delta",
            format!("{} {} bytes", item_id.expose_for_wire(), fragment.len()),
        ),
        E::RefusalDelta { item_id, fragment } => {
            ("refusal-delta", format!("{} {} bytes", item_id.expose_for_wire(), fragment.len()))
        }
        E::ToolCallStarted { item_id, call_id, name } => (
            "tool-call-started",
            format!(
                "{} {} {}",
                name.as_str(),
                call_id.expose_for_wire(),
                item_id.expose_for_wire()
            ),
        ),
        E::ToolArgumentDelta { call_id, fragment } => (
            "tool-argument-delta",
            format!("{} {} bytes", call_id.expose_for_wire(), fragment.len()),
        ),
        E::ItemCompleted(item_id) => ("item-completed", item_id.expose_for_wire().to_owned()),
        E::Usage(_) => ("usage", String::new()),
        E::RateLimit(_) => ("rate-limit", String::new()),
        E::Cache(_) => ("cache", String::new()),
        E::Finish(reason) => ("finish", format!("{reason:?}")),
        E::ProviderEvent(extension) => ("extension", extension.name().as_str().to_owned()),
        E::Heartbeat => ("heartbeat", String::new()),
        E::ResponseCompleted => ("response-completed", String::new()),
        E::ResponseFailed(failure) => (
            "response-failed",
            format!(
                "{:?} {} {:?}",
                failure.category(),
                failure.diagnostic().code(),
                failure.retryability()
            ),
        ),
        E::ResponseCancelled => ("response-cancelled", String::new()),
        _ => ("event", String::new()),
    }
}

const fn kind_name(kind: DeveloperTraceFrameKind) -> &'static str {
    match kind {
        DeveloperTraceFrameKind::ProviderEnvelope => "provider-envelope",
        DeveloperTraceFrameKind::ToolObservation => "tool-observation",
        DeveloperTraceFrameKind::ContextCompaction => "context-compaction",
        DeveloperTraceFrameKind::RetryScheduled => "retry-scheduled",
        DeveloperTraceFrameKind::ProviderSwitch => "provider-switch",
        DeveloperTraceFrameKind::LocalMemoryCheckpoint => "local-memory-checkpoint",
        DeveloperTraceFrameKind::LocalMemoryObservation => "local-memory-observation",
    }
}
