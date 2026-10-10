//! Exact byte-range retrieval and bounded literal-search/state pages with scope-bound cursors.

use super::super::record::ArchiveKind;
use super::{LocalMemory, error, handle, hex, rejected, sequence};
use peritus_agent::DeveloperLoopError;
use serde::Deserialize;
use serde_json::Value;

mod cursor;
mod state;
use cursor::{cursor, search_cursor};
use state::state_page;

const ARTIFACT_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    observation_ids: Vec<String>,
    query: Option<String>,
    cursor: Option<String>,
    offset: usize,
    max_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CursorPosition {
    index: usize,
    offset: u64,
}

pub(in crate::local_context) fn execute(
    memory: &mut LocalMemory,
    bytes: &[u8],
) -> Result<Value, DeveloperLoopError> {
    let Ok(request) = serde_json::from_slice::<Read>(bytes) else {
        return Ok(rejected(memory, "invalid read schema"));
    };
    if !(256..=memory.config.max_read_bytes).contains(&request.max_bytes) {
        return Ok(rejected(
            memory,
            &format!("max_bytes must be within 256..={}", memory.config.max_read_bytes),
        ));
    }
    if request.query.as_ref().is_some_and(String::is_empty) {
        return Ok(rejected(memory, "query must be null or a nonempty literal search"));
    }
    // Validate every explicit scope before opening any source artifact.
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
    let position = match cursor(
        memory,
        request.cursor.as_deref(),
        if state { "entry" } else { "search" },
        request.query.as_deref(),
    ) {
        Ok(position) => position,
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
        state_page(memory, &request, position)
    } else {
        source_page(memory, &request, &ids, position)
    }
}

fn source_page(
    memory: &LocalMemory,
    request: &Read,
    ids: &[u64],
    start: CursorPosition,
) -> Result<Value, DeveloperLoopError> {
    let search = request.query.as_deref();
    let mut response = Value::from_iter([
        ("base_revision", Value::from(memory.model_revision)),
        ("authority", Value::from("none")),
        ("sources", Value::Array(vec![])),
        ("cursor", Value::Null),
        ("next_handle_index", Value::Null),
    ]);
    let mut index = start.index;
    let mut offset = start.offset;
    let source_count = if ids.is_empty() { memory.sources.len() } else { ids.len() };
    while index < source_count {
        let id = if ids.is_empty() { memory.sources[index].sequence } else { ids[index] };
        let source = memory.archived(id)?;
        if search.is_some()
            && (source.kind == ArchiveKind::ToolMessage
                || source.call.as_ref().is_some_and(|call| {
                    matches!(call.name.as_str(), "context_read" | "context_update")
                }))
        {
            index += 1;
            offset = 0;
            response["cursor"] = search_cursor(memory, search.unwrap_or_default(), index, offset);
            continue;
        }

        if let Some(query) = search {
            let hit = find_match(memory, source, query.as_bytes(), offset)?;
            let Some(hit) = hit else {
                index += 1;
                offset = 0;
                response["cursor"] = search_cursor(memory, query, index, offset);
                continue;
            };
            let continuation = hit.checked_add(1).ok_or_else(|| error("search offset overflow"))?;
            let cursor = search_cursor(memory, query, index, continuation);
            let requested_offset = u64::try_from(request.offset).unwrap_or(u64::MAX);
            let preview_offset = usize::try_from(hit.saturating_sub(128).max(requested_offset))
                .unwrap_or(usize::MAX);
            if !append_source(
                memory,
                request,
                &mut response,
                source,
                preview_offset,
                Some(hit),
                Some(&cursor),
            )? {
                if response["sources"].as_array().is_some_and(Vec::is_empty) {
                    return Ok(rejected(
                        memory,
                        "max_bytes cannot fit source metadata; increase it",
                    ));
                }
                response["cursor"] = search_cursor(memory, query, index, offset);
                return Ok(response);
            }
            response["cursor"] = cursor;
            offset = continuation;
            continue;
        }

        let source_offset = request.offset;
        if !append_source(memory, request, &mut response, source, source_offset, None, None)? {
            if response["sources"].as_array().is_some_and(Vec::is_empty) {
                return Ok(rejected(memory, "max_bytes cannot fit source metadata; increase it"));
            }
            if !ids.is_empty() {
                response["next_handle_index"] = Value::from(index);
            }
            return Ok(response);
        }
        index += 1;
        if !ids.is_empty() && index < ids.len() {
            response["next_handle_index"] = Value::from(index);
        }
    }
    if search.is_some() {
        response["cursor"] = Value::Null;
    }
    if !ids.is_empty() {
        response["next_handle_index"] = Value::Null;
    }
    if !ids.is_empty() && response["sources"].as_array().is_some_and(Vec::is_empty) {
        return Ok(rejected(memory, "max_bytes cannot fit source metadata; increase it"));
    }
    Ok(response)
}

fn append_source(
    memory: &LocalMemory,
    request: &Read,
    response: &mut Value,
    source: &super::super::record::ArchivedObservation,
    offset: usize,
    match_offset: Option<u64>,
    cursor: Option<&Value>,
) -> Result<bool, DeveloperLoopError> {
    let mut candidate = response.clone();
    let source_offset = u64::try_from(offset)
        .map_err(|_| error("source offset exceeds durable size representation"))?
        .min(source.artifact.bytes);
    let mut item = Value::from_iter([
        ("handle", Value::from(handle(memory, source.sequence))),
        ("sha256", Value::from(hex(source.artifact.digest.as_bytes()))),
        ("total_bytes", Value::from(source.artifact.bytes)),
        ("offset", Value::from(source_offset)),
        ("next_offset", Value::Null),
        ("encoding", Value::from("utf8")),
        ("data", Value::from("")),
        ("is_error", Value::from(source.is_error)),
    ]);
    if let Some(match_offset) = match_offset {
        item["match_offset"] = Value::from(match_offset);
    }
    candidate["sources"].as_array_mut().ok_or_else(|| error("invalid read response"))?.push(item);
    if let Some(cursor) = cursor {
        candidate["cursor"] = (*cursor).clone();
    }
    if candidate.to_string().len() > request.max_bytes {
        return Ok(false);
    }

    let bytes =
        read_fragment(memory, source, source_offset, request.max_bytes.min(ARTIFACT_CHUNK_BYTES))?;
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => Some(text),
        Err(error) if error.error_len().is_none() && error.valid_up_to() > 0 => {
            std::str::from_utf8(&bytes[..error.valid_up_to()]).ok()
        }
        Err(_) => None,
    };
    let encoding = if text.is_some() { "utf8" } else { "hex" };
    let raw_length = text.map_or(bytes.len(), str::len);
    let boundaries = text.map(|text| {
        text.char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(text.len()))
            .collect::<Vec<_>>()
    });
    let mut low = 0_usize;
    let mut high = boundaries.as_ref().map_or(raw_length, |items| items.len().saturating_sub(1));
    let mut best = 0_usize;
    while low <= high {
        let middle = low + (high - low) / 2;
        let byte_length = boundaries.as_ref().map_or(middle, |items| items[middle]);
        let data =
            text.map_or_else(|| hex(&bytes[..byte_length]), |text| text[..byte_length].to_owned());
        let end = source_offset
            .checked_add(u64::try_from(byte_length).map_err(|_| error("source offset overflow"))?)
            .ok_or_else(|| error("source offset overflow"))?;
        let item = candidate["sources"]
            .as_array_mut()
            .and_then(|items| items.last_mut())
            .ok_or_else(|| error("invalid read page"))?;
        item["data"] = Value::from(data);
        item["encoding"] = Value::from(encoding);
        item["next_offset"] =
            if end < source.artifact.bytes { Value::from(end) } else { Value::Null };
        if candidate.to_string().len() <= request.max_bytes {
            best = byte_length;
            low = middle.saturating_add(1);
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }
    let data = text.map_or_else(|| hex(&bytes[..best]), |text| text[..best].to_owned());
    if best == 0 && source_offset < source.artifact.bytes {
        return Ok(false);
    }
    let end = source_offset
        .checked_add(u64::try_from(best).map_err(|_| error("source offset overflow"))?)
        .ok_or_else(|| error("source offset overflow"))?;
    let item = candidate["sources"]
        .as_array_mut()
        .and_then(|items| items.last_mut())
        .ok_or_else(|| error("invalid read page"))?;
    item["data"] = Value::from(data);
    item["encoding"] = Value::from(encoding);
    item["next_offset"] = if end < source.artifact.bytes { Value::from(end) } else { Value::Null };
    if candidate.to_string().len() > request.max_bytes {
        return Ok(false);
    }
    *response = candidate;
    Ok(true)
}

fn read_fragment(
    memory: &LocalMemory,
    source: &super::super::record::ArchivedObservation,
    offset: u64,
    maximum: usize,
) -> Result<Vec<u8>, DeveloperLoopError> {
    let mut reader = memory.store.open_read(source.artifact)?;
    let mut output = Vec::new();
    while let Some(chunk) =
        reader.read_chunk(ARTIFACT_CHUNK_BYTES).map_err(|_| error("read source artifact"))?
    {
        let chunk_end = chunk
            .offset()
            .checked_add(
                u64::try_from(chunk.bytes().len())
                    .map_err(|_| error("source chunk size overflow"))?,
            )
            .ok_or_else(|| error("source byte offset overflow"))?;
        if chunk_end <= offset {
            continue;
        }
        let within = usize::try_from(offset.saturating_sub(chunk.offset()))
            .map_err(|_| error("source offset exceeds platform capacity"))?;
        output.extend_from_slice(&chunk.bytes()[within..]);
        if output.len() >= maximum {
            output.truncate(maximum);
            break;
        }
    }
    Ok(output)
}

fn find_match(
    memory: &LocalMemory,
    source: &super::super::record::ArchivedObservation,
    query: &[u8],
    start: u64,
) -> Result<Option<u64>, DeveloperLoopError> {
    if query.is_empty() || u64::try_from(query.len()).unwrap_or(u64::MAX) > source.artifact.bytes {
        return Ok(None);
    }
    let mut reader = memory.store.open_read(source.artifact)?;
    let mut carry = Vec::new();
    while let Some(chunk) =
        reader.read_chunk(ARTIFACT_CHUNK_BYTES).map_err(|_| error("search source artifact"))?
    {
        let carry_len = carry.len();
        let base = chunk.offset().saturating_sub(u64::try_from(carry_len).unwrap_or(u64::MAX));
        carry.extend_from_slice(chunk.bytes());
        let scan_from =
            usize::try_from(start.saturating_sub(base)).unwrap_or(usize::MAX).min(carry.len());
        if let Some(found) =
            carry[scan_from..].windows(query.len()).position(|window| window == query)
        {
            let absolute = base
                .checked_add(
                    u64::try_from(scan_from + found)
                        .map_err(|_| error("search offset overflow"))?,
                )
                .ok_or_else(|| error("search offset overflow"))?;
            if absolute >= start {
                return Ok(Some(absolute));
            }
        }
        let keep = query.len().saturating_sub(1).min(carry.len());
        carry.drain(..carry.len() - keep);
    }
    Ok(None)
}
