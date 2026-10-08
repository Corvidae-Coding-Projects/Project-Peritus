use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::super::{LocalMemory, entry_view, error, rejected};
use super::{CursorPosition, Read, cursor::entry_cursor};

pub(super) fn state_page(
    memory: &LocalMemory,
    request: &Read,
    start: CursorPosition,
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
    if start.index > entries.len() {
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
    for (index, entry) in entries.iter().enumerate().skip(start.index) {
        let item = entry_view(memory, entry);
        let text = item["text"].as_str().unwrap_or_default().to_owned();
        let text_offset = if index == start.index {
            usize::try_from(start.offset).unwrap_or(usize::MAX)
        } else {
            0
        };
        if text_offset > text.len() || !text.is_char_boundary(text_offset) {
            return Ok(rejected(memory, "entry text cursor out of range"));
        }
        let next_entry = CursorPosition { index: index + 1, offset: 0 };
        let mut complete = response.clone();
        complete["entries"]
            .as_array_mut()
            .ok_or_else(|| error("invalid state response"))?
            .push(item.clone());
        complete["cursor"] = if index + 1 < entries.len() {
            Value::from(entry_cursor(memory, index + 1, 0))
        } else {
            Value::Null
        };
        if text_offset == 0 && complete.to_string().len() <= request.max_bytes {
            response = complete;
            continue;
        }
        if response["entries"].as_array().is_some_and(|items| !items.is_empty()) {
            response["cursor"] = Value::from(entry_cursor(memory, index, text_offset as u64));
            return Ok(response);
        }
        let Some((partial, end)) = partial_entry(
            &response,
            item,
            index,
            text_offset,
            request.max_bytes,
            memory,
            index + 1 == entries.len(),
        )?
        else {
            return Ok(rejected(memory, "max_bytes cannot fit entry metadata; increase it"));
        };
        response = partial;
        if end < text.len() {
            response["cursor"] = Value::from(entry_cursor(memory, index, end as u64));
        } else {
            response["cursor"] = if index + 1 < entries.len() {
                Value::from(entry_cursor(memory, next_entry.index, 0))
            } else {
                Value::Null
            };
        }
        return Ok(response);
    }
    Ok(response)
}

fn partial_entry(
    response: &Value,
    mut item: Value,
    index: usize,
    start: usize,
    maximum: usize,
    memory: &LocalMemory,
    last: bool,
) -> Result<Option<(Value, usize)>, DeveloperLoopError> {
    let text = item["text"].as_str().unwrap_or_default().to_owned();
    let boundaries = text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .filter(|offset| *offset >= start)
        .collect::<Vec<_>>();
    let mut low = 0_usize;
    let mut high = boundaries.len().saturating_sub(1);
    let mut best = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        let end = boundaries[middle];
        item["text"] = Value::from(&text[start..end]);
        item["text_offset"] = Value::from(start);
        item["text_total_bytes"] = Value::from(text.len());
        item["next_text_offset"] = if end < text.len() { Value::from(end) } else { Value::Null };
        let mut candidate = response.clone();
        candidate["entries"]
            .as_array_mut()
            .ok_or_else(|| error("invalid state response"))?
            .push(item.clone());
        candidate["cursor"] = if end < text.len() {
            Value::from(entry_cursor(memory, index, end as u64))
        } else if last {
            Value::Null
        } else {
            Value::from(entry_cursor(memory, index + 1, 0))
        };
        if candidate.to_string().len() <= maximum {
            best = Some((candidate, end));
            low = middle.saturating_add(1);
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }
    Ok(best.filter(|(_, end)| *end > start || start == text.len()))
}
