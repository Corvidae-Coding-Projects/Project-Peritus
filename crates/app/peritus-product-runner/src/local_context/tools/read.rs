//! Exact byte-range retrieval and bounded literal-search/state pages with scope-bound cursors.

use super::super::record::ArchiveKind;
use super::{LocalMemory, error, handle, hex, rejected, sequence};
use peritus_agent::DeveloperLoopError;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    observation_ids: Vec<String>,
    query: Option<String>,
    cursor: Option<String>,
    offset: usize,
    max_bytes: usize,
}

pub(in crate::local_context) fn execute(
    memory: &mut LocalMemory,
    bytes: &[u8],
) -> Result<Value, DeveloperLoopError> {
    if bytes.len() > 8192 {
        return Ok(rejected(memory, "read request exceeds bound"));
    }
    let Ok(request) = serde_json::from_slice::<Read>(bytes) else {
        return Ok(rejected(memory, "invalid read schema"));
    };
    if !(256..=memory.config.max_read_bytes).contains(&request.max_bytes)
        || request.observation_ids.len() > 32
        || request.query.as_ref().is_some_and(|query| query.is_empty() || query.len() > 256)
    {
        return Ok(rejected(memory, "read bounds rejected"));
    }
    // Validate all supplied scopes before touching any source bytes.
    let ids = match request
        .observation_ids
        .iter()
        .map(|id| sequence(memory, id))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(ids) => ids,
        Err(reason) => return Ok(rejected(memory, &reason.to_string())),
    };
    let state = ids.is_empty() && request.query.is_none();
    let start =
        match cursor(memory, request.cursor.as_deref(), if state { "entry" } else { "search" }) {
            Ok(start) => start,
            Err(reason) => return Ok(rejected(memory, &reason.to_string())),
        };
    if !ids.is_empty() && (request.cursor.is_some() || request.query.is_some()) {
        return Ok(rejected(
            memory,
            "explicit handles cannot be combined with search cursor/query",
        ));
    }
    memory.retrieval_calls =
        memory.retrieval_calls.checked_add(1).ok_or_else(|| error("retrieval counter overflow"))?;
    if state {
        return state_page(memory, &request, start);
    }
    source_page(memory, &request, &ids, start)
}

fn source_page(
    memory: &LocalMemory,
    request: &Read,
    ids: &[u64],
    start: usize,
) -> Result<Value, DeveloperLoopError> {
    let mut response = Value::from_iter([
        ("base_revision", Value::from(memory.model_revision)),
        ("authority", Value::from("none")),
        ("sources", Value::Array(vec![])),
        ("cursor", Value::Null),
        ("next_handle_index", Value::Null),
    ]);
    let selected = if ids.is_empty() {
        memory.sources.iter().skip(start).take(32).map(|source| source.sequence).collect()
    } else {
        ids.to_vec()
    };
    let mut scanned = 0_usize;
    for (index, id) in selected.into_iter().enumerate() {
        let source = memory.archived(id)?;
        let ordinal =
            usize::try_from(id).map_err(|_| error("source ordinal exceeds platform capacity"))?;
        let next_cursor = if request.observation_ids.is_empty() && ordinal < memory.sources.len() {
            Value::from(page_cursor(memory, "search", ordinal))
        } else {
            Value::Null
        };
        if request.query.is_some()
            && (source.kind == ArchiveKind::ToolMessage
                || source.call.as_ref().is_some_and(|call| {
                    matches!(call.name.as_str(), "context_read" | "context_update")
                }))
        {
            response["cursor"] = next_cursor;
            continue;
        }
        if scanned > 0
            && scanned.saturating_add(usize::try_from(source.artifact.bytes).unwrap_or(usize::MAX))
                > 64 * 1024 * 1024
        {
            if !request.observation_ids.is_empty() {
                response["next_handle_index"] = Value::from(index);
            }
            break;
        }
        let bytes = memory.artifact(id)?;
        scanned = scanned.saturating_add(bytes.len());
        let hit = request.query.as_ref().and_then(|query| {
            bytes.windows(query.len()).position(|window| window == query.as_bytes())
        });
        if (request.query.is_none() || hit.is_some())
            && !append_source(memory, request, &mut response, source, &bytes, hit)?
        {
            if response["sources"].as_array().is_some_and(Vec::is_empty) {
                return Ok(rejected(memory, "max_bytes cannot fit source metadata; increase it"));
            }
            if !request.observation_ids.is_empty() {
                response["next_handle_index"] = Value::from(index);
            }
            break;
        }
        response["cursor"] = next_cursor;
    }
    if response["sources"].as_array().is_some_and(Vec::is_empty)
        && !request.observation_ids.is_empty()
    {
        return Ok(rejected(memory, "max_bytes cannot fit source metadata; increase it"));
    }
    Ok(response)
}

fn append_source(
    memory: &LocalMemory,
    request: &Read,
    response: &mut Value,
    source: &super::super::record::ArchivedObservation,
    bytes: &[u8],
    hit: Option<usize>,
) -> Result<bool, DeveloperLoopError> {
    let offset = hit
        .map_or(request.offset, |position| request.offset.max(position.saturating_sub(128)))
        .min(bytes.len());
    let metadata = Value::from_iter([
        ("handle", Value::from(handle(memory, source.sequence))),
        ("sha256", Value::from(hex(source.artifact.digest.as_bytes()))),
        ("total_bytes", Value::from(bytes.len())),
        ("offset", Value::from(offset)),
        ("next_offset", Value::from(offset)),
        ("encoding", Value::from("utf8")),
        ("data", Value::from("")),
        ("is_error", Value::from(source.is_error)),
    ]);
    let mut candidate = response.clone();
    candidate["sources"]
        .as_array_mut()
        .ok_or_else(|| error("invalid read response"))?
        .push(metadata);
    // Reserve the largest cursor, index, and byte-offset growth before escaping exact bytes.
    candidate["cursor"] = Value::from(page_cursor(memory, "search", memory.sources.len()));
    let overhead = candidate.to_string().len() + 32;
    if overhead > request.max_bytes {
        return Ok(false);
    }
    let available = (request.max_bytes - overhead) / 6;
    let mut end = offset.saturating_add(available).min(bytes.len());
    let text = std::str::from_utf8(bytes).ok().filter(|text| text.is_char_boundary(offset));
    if let Some(text) = text {
        end = text.floor_char_boundary(end);
    }
    if end == offset && end < bytes.len() {
        return Ok(false);
    }
    let item = candidate["sources"]
        .as_array_mut()
        .and_then(|items| items.last_mut())
        .ok_or_else(|| error("invalid read page"))?;
    item["data"] = text.map_or_else(
        || Value::from(hex(&bytes[offset..end])),
        |text| Value::from(&text[offset..end]),
    );
    item["encoding"] = Value::from(if text.is_some() { "utf8" } else { "hex" });
    item["next_offset"] = if end < bytes.len() { Value::from(end) } else { Value::Null };
    *response = candidate;
    Ok(true)
}

fn state_page(
    memory: &LocalMemory,
    request: &Read,
    start: usize,
) -> Result<Value, DeveloperLoopError> {
    if !memory.derived_memory_allowed() {
        return Ok(rejected(
            memory,
            "role policy excludes derived memory; read exact source handles",
        ));
    }
    let entries = memory
        .state
        .entries(memory.state.binding())
        .map_err(|_| error("state read scope mismatch"))?;
    if start > entries.len() {
        return Ok(rejected(memory, "entry cursor out of range"));
    }
    let mut response = Value::from_iter([
        ("base_revision", Value::from(memory.model_revision)),
        ("generation", Value::from(memory.store.generation())),
        ("observations", Value::from(memory.sources.len())),
        ("pending_operations", Value::from(memory.transcript.pending.len())),
        ("authority", Value::from("none")),
        ("entries", Value::Array(vec![])),
        ("cursor", Value::Null),
    ]);
    for (index, entry) in entries.iter().enumerate().skip(start) {
        let item = super::entry_view(memory, entry);
        let mut candidate = response.clone();
        candidate["entries"]
            .as_array_mut()
            .ok_or_else(|| error("invalid state response"))?
            .push(item);
        candidate["cursor"] = if index + 1 < entries.len() {
            Value::from(page_cursor(memory, "entry", index + 1))
        } else {
            Value::Null
        };
        if candidate.to_string().len() > request.max_bytes {
            response["cursor"] = Value::from(page_cursor(memory, "entry", index));
            if index == start {
                return Ok(rejected(memory, "max_bytes cannot fit next entry; increase it"));
            }
            break;
        }
        response = candidate;
    }
    Ok(response)
}

fn page_cursor(memory: &LocalMemory, kind: &str, index: usize) -> String {
    format!("{}:{index}", cursor_prefix(memory, kind))
}

fn cursor_prefix(memory: &LocalMemory, kind: &str) -> String {
    let scope = hex(memory.store.scope_digest().as_bytes());
    if kind == "entry" {
        format!("entry:{scope}:{}", memory.model_revision)
    } else {
        format!("search:{scope}")
    }
}

fn cursor(
    memory: &LocalMemory,
    value: Option<&str>,
    kind: &str,
) -> Result<usize, DeveloperLoopError> {
    let Some(value) = value else {
        return Ok(0);
    };
    if value.len() > 100 {
        return Err(error("cursor exceeds bound"));
    }
    let prefix = format!("{}:", cursor_prefix(memory, kind));
    let tail = value.strip_prefix(&prefix).ok_or_else(|| error("cursor scope or kind mismatch"))?;
    let index = tail.parse::<usize>().map_err(|_| error("invalid cursor"))?;
    let bound = if kind == "entry" {
        memory.state.entries(memory.state.binding()).map_err(|_| error("entry cursor scope"))?.len()
    } else {
        memory.sources.len()
    };
    if index > bound {
        return Err(error("cursor out of range"));
    }
    Ok(index)
}
