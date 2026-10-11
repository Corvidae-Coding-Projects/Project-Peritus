//! Atomic `HarnessBench` usage-proxy publication.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use super::projection::Round;
use crate::BenchmarkError;

pub(super) fn publish(
    proxy_dir: &Path,
    task_id: &str,
    session_id: &str,
    model_id: &str,
    rounds: &[Round],
) -> Result<usize, BenchmarkError> {
    let responses = proxy_dir.join("responses");
    fs::create_dir_all(&responses).map_err(|error| {
        BenchmarkError::filesystem("create usage-proxy responses", &responses, error)
    })?;
    let mut request_rows = Vec::with_capacity(rounds.len());
    for (index, round) in rounds.iter().enumerate() {
        let path = responses.join(format!("peritus-{:04}.json", index + 1));
        let tool_calls = round
            .tool_calls
            .iter()
            .map(|call| {
                json!({
                    "id": call.id,
                    "type": "function",
                    "function": {"name": call.name, "arguments": call.arguments},
                })
            })
            .collect::<Vec<_>>();
        let record = json!({
            "task_id": task_id,
            "session_id": session_id,
            "model_id": model_id,
            "framework": "peritus",
            "provider": "peritus-normalized-trace",
            "request_body": serde_json::to_string(&json!({"messages": round.request_messages}))?,
            "response_json": {
                "model": round.model,
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": round.assistant_text,
                        "tool_calls": tool_calls,
                    }
                }]
            }
        });
        atomic_json(&path, &record)?;
        let counters = round.usage;
        let input = counters.input_tokens().unwrap_or(0);
        let output = counters.output_tokens().unwrap_or(0);
        let explicit_total = counters.total_tokens();
        let total = explicit_total.unwrap_or_else(|| input.saturating_add(output));
        let cached = counters.cached_input_tokens().or(round.observed_cache_tokens).unwrap_or(0);
        request_rows.push(json!({
            "task_id": task_id,
            "session_id": session_id,
            "model_id": model_id,
            "framework": "peritus",
            "provider": "peritus-normalized-trace",
            "raw_response_file": path,
            "input_tokens": input,
            "output_tokens": output,
            "cache_read_tokens": cached,
            "cache_write_tokens": counters.cache_creation_input_tokens().unwrap_or(0),
            "total_tokens": total,
            "total_tokens_source": if explicit_total.is_some() { "provider" } else { "derived_input_plus_output" },
            "provider_cost_microunits": counters.provider_cost_microunits(),
            "response_model": round.model,
        }));
    }
    atomic_json_lines(&proxy_dir.join("requests.jsonl"), &request_rows)?;
    Ok(rounds.len())
}

fn atomic_json(path: &Path, value: &Value) -> Result<(), BenchmarkError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    atomic(path, &bytes)
}

fn atomic_json_lines(path: &Path, values: &[Value]) -> Result<(), BenchmarkError> {
    let mut bytes = Vec::new();
    for value in values {
        serde_json::to_writer(&mut bytes, value)?;
        bytes.push(b'\n');
    }
    atomic(path, &bytes)
}

fn atomic(path: &Path, bytes: &[u8]) -> Result<(), BenchmarkError> {
    let temporary = temporary_path(path);
    let mut file = fs::File::create(&temporary).map_err(|error| {
        BenchmarkError::filesystem("create usage-proxy file", &temporary, error)
    })?;
    file.write_all(bytes)
        .map_err(|error| BenchmarkError::filesystem("write usage-proxy file", &temporary, error))?;
    file.sync_all()
        .map_err(|error| BenchmarkError::filesystem("sync usage-proxy file", &temporary, error))?;
    fs::rename(&temporary, path)
        .map_err(|error| BenchmarkError::filesystem("publish usage-proxy file", path, error))
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".new");
    PathBuf::from(value)
}
