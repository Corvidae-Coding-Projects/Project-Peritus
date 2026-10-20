//! Credits only evidence actually present in the final provider-visible messages.

use super::{LocalMemory, Page, ReadReuse, Span};
use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::{ContentBlock, Message, Role};
use serde_json::Value;

pub(in crate::local_context) fn register_rendered(
    memory: &mut LocalMemory,
    body: &str,
    id: u64,
    value: &Value,
) {
    if let Some(page) = Page::from_value(id, value) {
        memory
            .read_reuse
            .rendered
            .insert(peritus_codec::sha256(body.as_bytes()).into_bytes(), (id, page.span));
        memory.read_reuse.visible.entry(id).or_default().push(page.span);
    }
}

pub(in crate::local_context) fn observe_view(
    memory: &mut LocalMemory,
    messages: &[Message],
) -> Result<(), DeveloperLoopError> {
    let mut index = std::mem::take(&mut memory.read_reuse);
    let result = (|| {
        index.refresh(memory)?;
        index.visible.clear();
        index.pending.clear();
        for message in messages {
            for block in message.content() {
                match block {
                    ContentBlock::ToolResult(result) if !result.is_error() => {
                        if let Ok(value) =
                            serde_json::from_slice::<Value>(result.output().canonical_bytes())
                        {
                            credit_result(memory, &mut index, &value);
                        }
                    }
                    ContentBlock::Text(text) if message.role() == Role::User => {
                        let key =
                            peritus_codec::sha256(text.expose_for_wire().as_bytes()).into_bytes();
                        if let Some((id, span)) = index.rendered.get(&key).copied() {
                            index.visible.entry(id).or_default().push(span);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    memory.read_reuse = index;
    result
}

fn credit_result(memory: &LocalMemory, index: &mut ReadReuse, value: &Value) {
    if let Some(handle) =
        value.get("local_context").and_then(|value| value.get("handle")).and_then(Value::as_str)
    {
        if let Some(page) =
            super::super::tools::sequence(memory, handle).ok().and_then(|id| index.pages.get(&id))
        {
            let mut raw = value.clone();
            raw.as_object_mut().map(|object| object.remove("local_context"));
            if memory.archived(page.id).is_ok_and(|source| {
                source.artifact.digest == peritus_codec::sha256(raw.to_string().as_bytes())
            }) {
                index.visible.entry(page.id).or_default().push(page.span);
            }
        }
    }
    // A complete context_read fragment can restore an exact archived page. Partial raw
    // JSON fragments are conservatively excluded; the read tool can restore the page.
    if let Some(sources) = value.get("sources").and_then(Value::as_array) {
        for source in sources {
            if source["offset"] != 0
                || !source["next_offset"].is_null()
                || source["encoding"] != "utf8"
            {
                continue;
            }
            let Some(handle) = source["handle"].as_str() else { continue };
            let Some(data) = source["data"].as_str() else { continue };
            if let Some(page) = super::super::tools::sequence(memory, handle)
                .ok()
                .and_then(|id| index.pages.get(&id))
                && memory.archived(page.id).is_ok_and(|record| {
                    record.artifact.digest == peritus_codec::sha256(data.as_bytes())
                })
            {
                index.visible.entry(page.id).or_default().push(page.span);
            }
        }
    }
}

pub(in crate::local_context) fn preview(
    memory: &LocalMemory,
    id: u64,
    maximum: usize,
) -> Result<Option<Value>, DeveloperLoopError> {
    let Some(page) = memory.read_reuse.pages.get(&id) else { return Ok(None) };
    // Assembly also reuses coverage: overlapping old pages must not repeatedly fill
    // the optional evidence window with the same bytes after compaction.
    let mut spans = memory
        .read_reuse
        .paths
        .get(&page.path)
        .into_iter()
        .flatten()
        .filter_map(|id| memory.read_reuse.pages.get(id))
        .filter(|other| other.digest == page.digest)
        .flat_map(|other| memory.read_reuse.visible.get(&other.id).into_iter().flatten())
        .filter_map(|span| span.intersect(page.span))
        .collect::<Vec<_>>();
    spans.sort_unstable_by_key(|span| span.start);
    let mut cursor = page.span.start;
    let mut end = page.span.end;
    for span in spans {
        if span.start > cursor {
            end = span.start;
            break;
        }
        cursor = cursor.max(span.end);
    }
    if cursor >= page.span.end {
        return Ok(None);
    }
    super::page::slice(memory, page, Span { start: cursor, end }, maximum)
}
