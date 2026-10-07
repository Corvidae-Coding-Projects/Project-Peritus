//! Product-facing projection of C4 progress and terminal results.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, StoreConfig};
use peritus_tool_protocol::{ArtifactCompleteness, ResultStatus, ToolProgress, ToolResult};
use serde_json::Value;

use crate::developer_tools::wire::object;

const MODEL_STREAM_BYTES: usize = 512 * 1_024;
const HALF_STREAM_BYTES: usize = MODEL_STREAM_BYTES / 2;

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
        let output =
            bounded_stream(&bytes, artifact.completeness() != ArtifactCompleteness::Complete);
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
