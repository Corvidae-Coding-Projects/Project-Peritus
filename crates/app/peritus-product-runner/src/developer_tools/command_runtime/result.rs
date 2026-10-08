//! Product-facing projection of C4 progress and terminal results.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, StoreConfig};
use peritus_tool_protocol::{
    ArtifactCompleteness, ArtifactReference, ProgressContract, ResultStatus, ToolProgress,
    ToolResult,
};
use peritus_tool_router::{ControlRetryability, DispatchFailure, RouterError};
use serde_json::Value;

use crate::developer_tools::wire::object;
use super::ProgressBatch;

const MODEL_STREAM_BYTES: usize = 512 * 1_024;
const HALF_STREAM_BYTES: usize = MODEL_STREAM_BYTES / 2;
const OUTPUT_PAGE_BYTES: usize = 16 * 1_024;

pub(super) fn active(handle: &str, progress: &ProgressBatch) -> Value {
    object(vec![
        ("handle", Value::String(handle.to_owned())),
        ("state", Value::String("running".to_owned())),
        ("success", Value::Bool(true)),
        ("progress", Value::Array(progress_values(&progress.events))),
        ("progress_page", progress_page_value(progress)),
    ])
}

pub(super) fn operation_failed(handle: &str, error: &RouterError) -> Value {
    let (admission, retry) = match error.control_retryability() {
        Some(ControlRetryability::WhenReady) => ("rejected", "same_control_when_ready"),
        Some(ControlRetryability::CorrectRequest) => ("rejected", "correct_control"),
        Some(ControlRetryability::ObserveOnly) => ("rejected", "observe_invocation"),
        None => ("unknown", "observe_invocation"),
    };
    let failure = error.dispatch_failure().map_or_else(
        || object(vec![
            ("detail", Value::String(error.detail().to_owned())),
            ("kind", Value::String(format!("{:?}", error.kind()).to_ascii_lowercase())),
        ]),
        |failure| {
            let typed = failure.failure();
            object(vec![
                ("category", Value::String(format!("{:?}", typed.category()).to_ascii_lowercase())),
                ("code", Value::String(typed.code().as_str().to_owned())),
                ("detail", Value::String(typed.detail().as_str().to_owned())),
                ("subsystem", Value::String(format!("{:?}", typed.subsystem()).to_ascii_lowercase())),
            ])
        },
    );
    object(vec![
        ("failure", failure),
        ("handle", Value::String(handle.to_owned())),
        ("operation", Value::String(error.operation().to_owned())),
        ("operation_failed", Value::Bool(true)),
        ("owner_retained", Value::Bool(true)),
        ("progress", Value::Array(Vec::new())),
        ("reconciliation_pending", Value::Bool(true)),
        ("request_admission", Value::String(admission.to_owned())),
        ("request_retry", Value::String(retry.to_owned())),
        ("state", Value::String("running".to_owned())),
        ("success", Value::Bool(false)),
    ])
}

pub(super) fn settlement_pending(
    handle: &str,
    failure: &DispatchFailure,
    progress: &ProgressBatch,
) -> Value {
    let typed = failure.failure();
    object(vec![
        (
            "failure",
            object(vec![
                ("category", Value::String(format!("{:?}", typed.category()).to_ascii_lowercase())),
                ("code", Value::String(typed.code().as_str().to_owned())),
                ("detail", Value::String(typed.detail().as_str().to_owned())),
                (
                    "recovery",
                    Value::String(format!("{:?}", typed.recovery()).to_ascii_lowercase()),
                ),
                (
                    "retryability",
                    Value::String(format!("{:?}", typed.retryability()).to_ascii_lowercase()),
                ),
                (
                    "subsystem",
                    Value::String(format!("{:?}", typed.subsystem()).to_ascii_lowercase()),
                ),
            ]),
        ),
        ("handle", Value::String(handle.to_owned())),
        ("progress", Value::Array(progress_values(&progress.events))),
        ("progress_page", progress_page_value(progress)),
        ("settlement_pending", Value::Bool(true)),
        ("state", Value::String("running".to_owned())),
        (
            "status",
            Value::String(status_name(failure.status()).to_owned()),
        ),
        ("success", Value::Bool(false)),
    ])
}

pub(super) fn starting(
    handle: &str,
    process_id: peritus_types::ProcessId,
    interactive: bool,
    elapsed_millis: u64,
) -> Value {
    object(vec![
        ("elapsed_millis", Value::from(elapsed_millis)),
        ("handle", Value::String(handle.to_owned())),
        ("interactive", Value::Bool(interactive)),
        (
            "process_id",
            Value::String(super::identity::process_hex(process_id)),
        ),
        ("progress", Value::Array(Vec::new())),
        ("reconciliation_pending", Value::Bool(true)),
        ("state", Value::String("starting".to_owned())),
        ("success", Value::Bool(true)),
    ])
}

pub(super) fn cancellation_pending(
    handle: &str,
    cancellation_admitted: bool,
    process_terminal: bool,
) -> Value {
    object(vec![
        ("cancellation_admitted", Value::Bool(cancellation_admitted)),
        ("handle", Value::String(handle.to_owned())),
        ("process_terminal", Value::Bool(process_terminal)),
        ("progress", Value::Array(Vec::new())),
        ("publication_pending", Value::Bool(process_terminal)),
        ("settlement_pending", Value::Bool(true)),
        ("state", Value::String("running".to_owned())),
        ("success", Value::Bool(true)),
    ])
}

pub(super) fn terminal(
    handle: &str,
    result: &ToolResult,
    artifact_config: &StoreConfig,
    progress: &ProgressBatch,
) -> Result<Value, String> {
    let store = ArtifactStore::open(artifact_config.clone())
        .map_err(|error| format!("reopen command artifact store: {error}"))?;
    let mut stdout = String::new();
    let mut stderr = String::new();
    for artifact in result.artifacts() {
        let output = read_bounded_stream(&store, artifact)?;
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
        ("artifacts", Value::Array(artifact_values(result))),
        ("disposition", Value::String(disposition)),
        ("exit_code", exit_code),
        ("failure", failure),
        ("handle", Value::String(handle.to_owned())),
        ("prepared_digest", Value::String(hex(result.prepared_digest().as_bytes()))),
        ("progress", Value::Array(progress_values(&progress.events))),
        ("progress_page", progress_page_value(progress)),
        ("state", Value::String("completed".to_owned())),
        ("status", Value::String(status.to_owned())),
        ("stderr", Value::String(stderr)),
        ("stdout", Value::String(stdout)),
        ("success", Value::Bool(result.status() == ResultStatus::Succeeded)),
        ("timed_out", Value::Bool(result.status() == ResultStatus::TimedOut)),
        ("tool_result", structured),
        (
            "output_coverage",
            object(vec![
                ("preview_bytes", Value::from(MODEL_STREAM_BYTES)),
                ("retrieval", Value::String("command_output_read".to_owned())),
                ("retrievable_exact", Value::Bool(true)),
            ]),
        ),
    ]))
}

pub(super) fn terminal_deferred(
    handle: &str,
    result: &ToolResult,
    progress: &ProgressBatch,
) -> Value {
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
            (
                "subsystem",
                Value::String(format!("{:?}", failure.subsystem()).to_ascii_lowercase()),
            ),
        ])
    });
    let artifacts = artifact_values(result);
    object(vec![
        ("artifacts", Value::Array(artifacts)),
        ("disposition", Value::String(disposition)),
        ("exit_code", exit_code),
        ("failure", failure),
        ("handle", Value::String(handle.to_owned())),
        (
            "output_preview",
            object(vec![
                (
                    "detail",
                    Value::String(
                        "output preview is deferred to a foreground artifact reader".to_owned(),
                    ),
                ),
                ("state", Value::String("deferred".to_owned())),
            ]),
        ),
        ("preview_pending", Value::Bool(true)),
        ("prepared_digest", Value::String(hex(result.prepared_digest().as_bytes()))),
        ("progress", Value::Array(progress_values(&progress.events))),
        ("progress_page", progress_page_value(progress)),
        ("state", Value::String("completed".to_owned())),
        ("status", Value::String(status.to_owned())),
        ("success", Value::Bool(result.status() == ResultStatus::Succeeded)),
        ("timed_out", Value::Bool(result.status() == ResultStatus::TimedOut)),
        ("tool_result", structured),
        (
            "output_coverage",
            object(vec![
                ("preview_bytes", Value::from(MODEL_STREAM_BYTES)),
                ("retrieval", Value::String("command_output_read".to_owned())),
                ("retrievable_exact", Value::Bool(true)),
            ]),
        ),
    ])
}

fn artifact_values(result: &ToolResult) -> Vec<Value> {
    result
        .artifacts()
        .iter()
        .map(|artifact| {
            let provenance = artifact.provenance();
            object(vec![
                (
                    "action_id",
                    Value::String(super::identity::action_hex(provenance.action_id())),
                ),
                ("completeness", Value::String(completeness_name(artifact.completeness()).to_owned())),
                ("digest", Value::String(hex(artifact.digest().as_bytes()))),
                ("label", Value::String(artifact.label().as_str().to_owned())),
                ("media_type", Value::String(artifact.media_type().as_str().to_owned())),
                (
                    "prepared_digest",
                    Value::String(hex(provenance.prepared_digest().as_bytes())),
                ),
                ("size", Value::String(artifact.size().to_string())),
            ])
        })
        .collect()
}

pub(super) fn read_output_page(
    projection: &Value,
    artifact_config: &StoreConfig,
    handle: &str,
    label: &str,
    digest_text: &str,
    prepared_text: &str,
    size: u64,
    offset: u64,
) -> Result<Value, String> {
    if projection.get("state").and_then(Value::as_str) != Some("completed") {
        return Err("command output is available only after terminal completion".to_owned());
    }
    if projection.get("handle").and_then(Value::as_str) != Some(handle) {
        return Err("command output projection differs from its retained handle".to_owned());
    }
    let artifacts = projection
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "this legacy command projection has no retained exact output descriptors".to_owned()
        })?;
    let size_text = size.to_string();
    let descriptor = artifacts
        .iter()
        .find(|artifact| {
            artifact.get("label").and_then(Value::as_str) == Some(label)
                && artifact.get("digest").and_then(Value::as_str) == Some(digest_text)
                && artifact.get("prepared_digest").and_then(Value::as_str) == Some(prepared_text)
                && artifact.get("size").and_then(Value::as_str) == Some(size_text.as_str())
        })
        .ok_or_else(|| "command output request does not match a retained artifact".to_owned())?;
    if descriptor.get("action_id").and_then(Value::as_str) != Some(handle) {
        return Err("command output artifact provenance differs from its handle".to_owned());
    }
    let prepared = parse_digest(prepared_text)
        .map_err(|_| "command output prepared digest is invalid".to_owned())?;
    if projection
        .get("prepared_digest")
        .and_then(Value::as_str)
        .is_some_and(|value| parse_digest(value).ok() != Some(prepared))
    {
        return Err("command output artifact differs from its prepared result".to_owned());
    }
    let digest = parse_digest(digest_text)
        .map_err(|_| "command output digest is invalid".to_owned())?;
    if offset > size {
        return Err("command output offset is beyond the retained artifact".to_owned());
    }
    let store = ArtifactStore::open(artifact_config.clone())
        .map_err(|error| format!("reopen command artifact store: {error}"))?;
    let mut reader = store
        .open_read(ArtifactDigest::from_sha256(digest))
        .map_err(|error| format!("open command output artifact: {error}"))?;
    if reader.metadata().size() != size {
        return Err("command output artifact length differs from its reference".to_owned());
    }
    let maximum = usize::try_from(size.saturating_sub(offset).min(OUTPUT_PAGE_BYTES as u64))
        .map_err(|_| "command output page is not representable".to_owned())?;
    let bytes = if maximum == 0 {
        Vec::new()
    } else {
        reader
            .read_chunk_at(offset, maximum)
            .map_err(|error| format!("read command output artifact: {error}"))?
            .ok_or_else(|| "command output artifact page is missing".to_owned())?
            .bytes()
            .to_vec()
    };
    let end = offset
        .checked_add(bytes.len() as u64)
        .ok_or_else(|| "command output page offset overflowed".to_owned())?;
    if offset < size && bytes.is_empty() {
        return Err("command output artifact ended before its retained length".to_owned());
    }
    let completeness = descriptor
        .get("completeness")
        .and_then(Value::as_str)
        .ok_or_else(|| "command output artifact completeness is missing".to_owned())?;
    let next = (end < size).then(|| {
        object(vec![
            ("handle", Value::String(handle.to_owned())),
            ("label", Value::String(label.to_owned())),
            ("digest", Value::String(digest_text.to_owned())),
            ("prepared_digest", Value::String(prepared_text.to_owned())),
            ("size", Value::String(size.to_string())),
            ("offset", Value::String(end.to_string())),
        ])
    });
    Ok(object(vec![
        ("handle", Value::String(handle.to_owned())),
        ("label", Value::String(label.to_owned())),
        ("digest", Value::String(digest_text.to_owned())),
        ("prepared_digest", Value::String(prepared_text.to_owned())),
        ("size", Value::String(size.to_string())),
        ("start", Value::String(offset.to_string())),
        ("end", Value::String(end.to_string())),
        ("bytes_hex", Value::String(hex(&bytes))),
        ("capture_completeness", Value::String(completeness.to_owned())),
        (
            "utf8",
            std::str::from_utf8(&bytes)
                .map_or(Value::Null, |text| Value::String(text.to_owned())),
        ),
        ("complete", Value::Bool(next.is_none())),
        ("next", next.unwrap_or(Value::Null)),
    ]))
}

pub(super) fn preview_pending(value: &Value) -> bool {
    value.get("preview_pending").and_then(Value::as_bool) == Some(true)
}

pub(super) fn materialize_deferred(
    mut value: Value,
    artifact_config: &StoreConfig,
    expected_handle: &str,
) -> Result<Value, String> {
    if !preview_pending(&value) {
        return Ok(value);
    }
    let handle = value
        .get("handle")
        .and_then(Value::as_str)
        .ok_or_else(|| "deferred command projection has no handle".to_owned())?;
    if handle != expected_handle {
        return Err("deferred command projection differs from its retained handle".to_owned());
    }
    let expected_prepared = value
        .get("prepared_digest")
        .and_then(Value::as_str)
        .ok_or_else(|| "deferred command projection has no prepared digest".to_owned())?;
    let expected_prepared = parse_digest(expected_prepared)
        .map_err(|_| "deferred command projection prepared digest is invalid".to_owned())?;
    let artifacts = value
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| "deferred command projection has no artifact references".to_owned())?;
    let store = ArtifactStore::open(artifact_config.clone())
        .map_err(|error| format!("reopen command artifact store: {error}"))?;
    let mut stdout = String::new();
    let mut stderr = String::new();
    for artifact in artifacts {
        let action_id = artifact
            .get("action_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "deferred command artifact has no action identity".to_owned())?;
        if action_id != handle {
            return Err("deferred command artifact provenance differs from its handle".to_owned());
        }
        let artifact_prepared = artifact
            .get("prepared_digest")
            .and_then(Value::as_str)
            .ok_or_else(|| "deferred command artifact has no prepared digest".to_owned())?;
        let artifact_prepared = parse_digest(artifact_prepared)
            .map_err(|_| "deferred command artifact prepared digest is invalid".to_owned())?;
        if artifact_prepared != expected_prepared {
            return Err(
                "deferred command artifact provenance differs from its prepared result".to_owned(),
            );
        }
        let digest = artifact
            .get("digest")
            .and_then(Value::as_str)
            .ok_or_else(|| "deferred command artifact has no digest".to_owned())?;
        let digest = parse_digest(digest)
            .map_err(|_| "deferred command artifact digest is invalid".to_owned())?;
        let size = artifact
            .get("size")
            .and_then(Value::as_str)
            .and_then(|size| size.parse::<u64>().ok())
            .ok_or_else(|| "deferred command artifact size is invalid".to_owned())?;
        let label = artifact
            .get("label")
            .and_then(Value::as_str)
            .ok_or_else(|| "deferred command artifact has no label".to_owned())?;
        artifact
            .get("media_type")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "deferred command artifact media type is invalid".to_owned())?;
        let externally_truncated = match artifact.get("completeness").and_then(Value::as_str) {
            Some("complete") => false,
            Some("truncated" | "indeterminate") => true,
            _ => return Err("deferred command artifact completeness is invalid".to_owned()),
        };
        let output =
            read_bounded_artifact(&store, digest, size, externally_truncated)?;
        match label {
            "stdout" | "terminal" => stdout = output,
            "stderr" => stderr = output,
            _ => {}
        }
    }
    let projection = value
        .as_object_mut()
        .ok_or_else(|| "deferred command projection is not an object".to_owned())?;
    projection.remove("output_preview");
    projection.insert("preview_pending".to_owned(), Value::Bool(false));
    projection.insert("stderr".to_owned(), Value::String(stderr));
    projection.insert("stdout".to_owned(), Value::String(stdout));
    Ok(value)
}

fn read_bounded_stream(
    store: &ArtifactStore,
    artifact: &ArtifactReference,
) -> Result<String, String> {
    read_bounded_artifact(
        store,
        artifact.digest(),
        artifact.size(),
        artifact.completeness() != ArtifactCompleteness::Complete,
    )
}

fn read_bounded_artifact(
    store: &ArtifactStore,
    digest: peritus_types::Sha256Digest,
    size: u64,
    externally_truncated: bool,
) -> Result<String, String> {
    if size == 0 && digest != peritus_codec::sha256(b"") {
        return Err("empty command output artifact has a nonempty-content digest".to_owned());
    }
    let mut reader = store
        .open_read(ArtifactDigest::from_sha256(digest))
        .map_err(|error| format!("open command output artifact: {error}"))?;
    if reader.metadata().size() != size {
        return Err("command output artifact length differs from its reference".to_owned());
    }
    if size <= MODEL_STREAM_BYTES as u64 {
        let maximum = usize::try_from(size)
            .map_err(|_| "command output artifact length is unrepresentable".to_owned())?;
        let bytes = reader
            .read_chunk_at(0, maximum.max(1))
            .map_err(|error| format!("read command output artifact: {error}"))?
            .map_or_else(Vec::new, |chunk| chunk.bytes().to_vec());
        return Ok(bounded_stream(&bytes, externally_truncated));
    }
    let head = reader
        .read_chunk_at(0, HALF_STREAM_BYTES)
        .map_err(|error| format!("read command output artifact head: {error}"))?
        .ok_or_else(|| "command output artifact head is missing".to_owned())?;
    let tail_offset = size
        .checked_sub(HALF_STREAM_BYTES as u64)
        .ok_or_else(|| "command output artifact tail offset underflowed".to_owned())?;
    let tail = reader
        .read_chunk_at(tail_offset, HALF_STREAM_BYTES)
        .map_err(|error| format!("read command output artifact tail: {error}"))?
        .ok_or_else(|| "command output artifact tail is missing".to_owned())?;
    let marker = if externally_truncated {
        "\n[output truncated; stream incomplete]\n"
    } else {
        "\n[output truncated]\n"
    };
    Ok(join_stream_preview(
        &String::from_utf8_lossy(head.bytes()),
        &String::from_utf8_lossy(tail.bytes()),
        marker,
    ))
}

const fn completeness_name(value: ArtifactCompleteness) -> &'static str {
    match value {
        ArtifactCompleteness::Complete => "complete",
        ArtifactCompleteness::Truncated => "truncated",
        ArtifactCompleteness::Indeterminate => "indeterminate",
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn parse_digest(value: &str) -> Result<peritus_types::Sha256Digest, ()> {
    if value.len() != peritus_types::Sha256Digest::LENGTH * 2 {
        return Err(());
    }
    let mut bytes = [0_u8; peritus_types::Sha256Digest::LENGTH];
    for (target, pair) in bytes.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        let high = hex_nibble(pair[0]).ok_or(())?;
        let low = hex_nibble(pair[1]).ok_or(())?;
        *target = (high << 4) | low;
    }
    Ok(peritus_types::Sha256Digest::new(bytes))
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

pub(super) fn indeterminate(handle: &str, detail: &str) -> Value {
    object(vec![
        ("error", Value::String(detail.to_owned())),
        ("handle", Value::String(handle.to_owned())),
        ("state", Value::String("indeterminate".to_owned())),
        ("success", Value::Bool(false)),
    ])
}

fn progress_page_value(progress: &ProgressBatch) -> Value {
    let Some(page) = &progress.page else { return Value::Null };
    let replay_identity = hex(page.replay_identity().as_bytes());
    let digest = hex(page.digest().as_bytes());
    object(vec![
        (
            "action_id",
            Value::String(super::identity::action_hex(page.action_id())),
        ),
        ("digest", Value::String(digest.clone())),
        ("end", Value::String(page.end().to_string())),
        (
            "events",
            Value::Array(progress_values(&progress.events)),
        ),
        (
            "next",
            object(vec![
                (
                    "action_id",
                    Value::String(super::identity::action_hex(page.action_id())),
                ),
                ("frontier", Value::String(page.end().to_string())),
                ("previous_page_digest", Value::String(digest)),
                ("replay_identity", Value::String(replay_identity.clone())),
            ]),
        ),
        (
            "prepared_digest",
            Value::String(hex(page.prepared_digest().as_bytes())),
        ),
        (
            "previous_digest",
            page.previous_digest().map_or(Value::Null, |digest| {
                Value::String(hex(digest.as_bytes()))
            }),
        ),
        (
            "replay_identity",
            Value::String(replay_identity),
        ),
        ("start", Value::String(page.start().to_string())),
    ])
}

fn progress_values(progress: &[ToolProgress]) -> Vec<Value> {
    progress
        .iter()
        .map(|event| {
            let sequence = match event.progress_contract() {
                ProgressContract::LifetimeV1 => Value::from(
                    u32::try_from(event.sequence())
                        .expect("V1 progress construction bounds its sequence to u32"),
                ),
                ProgressContract::PagedV2 => Value::String(event.sequence().to_string()),
            };
            object(vec![
                ("kind", Value::String(format!("{:?}", event.kind()).to_ascii_lowercase())),
                ("message", Value::String(event.model_rendering().as_str().to_owned())),
                ("sequence", sequence),
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

fn bounded_stream(bytes: &[u8], externally_truncated: bool) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= MODEL_STREAM_BYTES && !externally_truncated {
        return text.into_owned();
    }
    let incomplete_marker = "\n[output incomplete]\n";
    if externally_truncated && text.len() <= MODEL_STREAM_BYTES - incomplete_marker.len() {
        return format!("{text}{incomplete_marker}");
    }
    let marker = if externally_truncated {
        "\n[output truncated; stream incomplete]\n"
    } else {
        "\n[output truncated]\n"
    };
    join_stream_preview(&text, &text, marker)
}

fn join_stream_preview(head: &str, tail: &str, marker: &str) -> String {
    let available = MODEL_STREAM_BYTES - marker.len();
    let head_end = head.floor_char_boundary(available.div_ceil(2).min(head.len()));
    let tail_start = tail.ceil_char_boundary(tail.len().saturating_sub(available / 2));
    let mut preview = String::with_capacity(head_end + marker.len() + tail.len() - tail_start);
    preview.push_str(&head[..head_end]);
    preview.push_str(marker);
    preview.push_str(&tail[tail_start..]);
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_projection_preserves_head_and_tail() {
        let bytes =
            [vec![b'a'; HALF_STREAM_BYTES + 10], vec![b'z'; HALF_STREAM_BYTES + 10]].concat();
        let value = bounded_stream(&bytes, false);
        assert!(value.starts_with('a'));
        assert!(value.contains("output truncated"));
        assert!(value.ends_with('z'));
    }
}
