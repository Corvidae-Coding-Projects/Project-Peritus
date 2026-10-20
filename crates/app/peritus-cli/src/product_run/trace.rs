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
use peritus_product_runner::{DeveloperTraceFrameKind, TraceFrame, TraceReadError, TraceReader};
use peritus_types::RunId;
use serde_json::Value;

use crate::{error::CliError, id::hex, output::Output};

/// Only the representation selected by this invocation is built for the current record.
enum Described {
    Json(Value),
    Human(String),
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
    let read_error = |error| match error {
        TraceReadError::Read(error) => {
            CliError::local_io("read the run private trace", Some(path.clone()), error)
        }
        TraceReadError::Framing(detail) => {
            CliError::protocol("decode the run private trace", detail)
        }
    };
    let mut reader = TraceReader::open(&path).map_err(&read_error)?;
    // A system temporary directory can be RAM-backed. Keep potentially large output in the
    // normal per-user cache so filtering/streaming does not move whole-trace growth into tmpfs.
    let spool_directory = layout.cache_root().join("trace-output");
    fs::create_dir_all(&spool_directory).map_err(|error| {
        CliError::local_io("prepare trace output cache", Some(spool_directory.clone()), error)
    })?;
    output.success_stream("product-run-trace", &spool_directory, |writer, json| {
        if json {
            writer.write_all(b"[").map_err(CliError::output)?;
        } else {
            // Preserve the human header's total count with a header-only pass over the same
            // snapshot. No payload is loaded or retained to obtain this count.
            reader.next_matching(|_| false).map_err(&read_error)?;
            writeln!(
                writer,
                "run {}  {} records  {}",
                hex(run_id.as_bytes()),
                reader.records_read(),
                path.display()
            )
            .map_err(CliError::output)?;
            reader.rewind().map_err(&read_error)?;
        }
        let mut first = true;
        while let Some(frame) = reader
            .next_matching(|kind| {
                !extensions_only || kind == DeveloperTraceFrameKind::ProviderEnvelope
            })
            .map_err(&read_error)?
        {
            let Some(described) = describe(reader.records_read(), &frame, extensions_only, json)?
            else {
                continue;
            };
            match described {
                Described::Json(value) => {
                    if !first {
                        writer.write_all(b",").map_err(CliError::output)?;
                    }
                    serde_json::to_writer(&mut *writer, &value)
                        .map_err(|error| CliError::output(io::Error::other(error)))?;
                }
                Described::Human(text) => writeln!(writer, "{text}").map_err(CliError::output)?,
            }
            first = false;
        }
        if json {
            writer.write_all(b"]").map_err(CliError::output)?;
        }
        Ok(())
    })
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

fn describe(
    index: u64,
    frame: &TraceFrame,
    extensions_only: bool,
    json: bool,
) -> Result<Option<Described>, CliError> {
    let kind = frame.kind();
    if kind != DeveloperTraceFrameKind::ProviderEnvelope {
        return Ok(Some(if json {
            Described::Json(serde_json::json!({
                "index": index, "kind": kind_name(kind), "payload": payload_value(frame.payload()),
            }))
        } else {
            Described::Human(format!(
                "{index:>6}  {:<22}  {}",
                kind_name(kind),
                payload_text(frame.payload())
            ))
        }));
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
        let name = extension.name().as_str();
        return Ok(Some(if json {
            Described::Json(serde_json::json!({
                "index": index, "kind": "provider-envelope", "event": "extension",
                "extension": name, "value": payload_value(extension.value().canonical_bytes()),
            }))
        } else {
            Described::Human(format!(
                "{index:>6}  {:<22}  extension {name} {}",
                kind_name(kind),
                payload_text(extension.value().canonical_bytes())
            ))
        }));
    }
    if extensions_only {
        return Ok(None);
    }
    let summary = event_summary(event);
    Ok(Some(if json {
        Described::Json(serde_json::json!({
            "index": index, "kind": "provider-envelope", "event": summary.0, "detail": summary.1,
        }))
    } else {
        Described::Human(format!(
            "{index:>6}  {:<22}  {} {}",
            kind_name(kind),
            summary.0,
            summary.1
        ))
    }))
}

fn payload_value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
}

fn payload_text(bytes: &[u8]) -> String {
    serde_json::from_slice::<Value>(bytes)
        .map_or_else(|_| String::from_utf8_lossy(bytes).into_owned(), |value| value.to_string())
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
