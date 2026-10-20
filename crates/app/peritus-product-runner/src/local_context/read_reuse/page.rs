//! Exact source fragments, sliced on UTF-8 byte boundaries and fitted to the caller's page.

use super::{LocalMemory, Page, Position, Span};
use peritus_agent::DeveloperLoopError;
use serde_json::{Value, json};

pub(super) fn valid_offset(
    memory: &LocalMemory,
    page: &Page,
    at: Position,
) -> Result<bool, DeveloperLoopError> {
    let value: Value = serde_json::from_slice(&memory.artifact(page.id)?)
        .map_err(|_| super::super::error("invalid archived read"))?;
    Ok(parts(&value, page).is_some_and(|parts| {
        parts.iter().any(|part| {
            part.line == at.line
                && at.byte >= part.offset
                && at.byte <= part.offset.saturating_add(part.text.len() as u64)
                && part.text.is_char_boundary((at.byte - part.offset) as usize)
        })
    }))
}

struct Part<'a> {
    line: u64,
    offset: u64,
    text: &'a str,
    complete: bool,
}

fn parts<'a>(value: &'a Value, page: &Page) -> Option<Vec<Part<'a>>> {
    let content = value.get("content")?.as_str()?;
    if content.is_empty() {
        return Some(Vec::new());
    }
    content
        .split('\n')
        .map(|line| {
            let (number, text) = line.split_once(": ")?;
            let line = number.parse::<u64>().ok()?;
            let offset = if line == page.span.start.line { page.span.start.byte } else { 0 };
            let complete = line < page.span.end.line;
            Some(Part { line, offset, text, complete })
        })
        .collect()
}

pub(super) fn slice(
    memory: &LocalMemory,
    page: &Page,
    requested: Span,
    maximum: usize,
) -> Result<Option<Value>, DeveloperLoopError> {
    let value: Value = serde_json::from_slice(&memory.artifact(page.id)?)
        .map_err(|_| super::super::error("invalid archived read"))?;
    slice_value(&value, page, requested, maximum)
}

pub(super) fn slice_value(
    value: &Value,
    page: &Page,
    requested: Span,
    maximum: usize,
) -> Result<Option<Value>, DeveloperLoopError> {
    let Some(span) = page.span.intersect(requested) else { return Ok(None) };
    let Some(parts) = parts(value, page) else { return Ok(None) };
    let mut output = value.clone();
    if let Some(object) = output.as_object_mut() {
        object.remove("local_context");
        object.remove("read_reuse");
    }
    output["start_line"] = json!(span.start.line);
    output["start_line_byte_offset"] = json!(span.start.byte);
    struct Segment {
        line: u64,
        offset: u64,
        begin: usize,
        end: usize,
        complete: bool,
    }
    let mut content = String::new();
    let mut segments = Vec::new();
    for part in parts {
        if part.line < span.start.line
            || part.line > span.end.line
            || (part.line == span.end.line && span.end.byte == 0)
        {
            continue;
        }
        let begin = if part.line == span.start.line {
            span.start.byte.saturating_sub(part.offset)
        } else {
            0
        };
        let end = if part.line == span.end.line {
            span.end.byte.saturating_sub(part.offset)
        } else {
            part.text.len() as u64
        };
        let (Ok(begin), Ok(end)) = (usize::try_from(begin), usize::try_from(end)) else {
            return Ok(None);
        };
        let Some(text) = part.text.get(begin..end) else { return Ok(None) };
        if !content.is_empty() {
            content.push('\n');
        }
        use std::fmt::Write as _;
        let _ = write!(content, "{}: ", part.line);
        let text_begin = content.len();
        content.push_str(text);
        segments.push(Segment {
            line: part.line,
            offset: part.offset + begin as u64,
            begin: text_begin,
            end: content.len(),
            complete: part.complete && end == part.text.len(),
        });
    }
    if segments.is_empty() {
        return Ok(None);
    }
    // One source pass and logarithmic serialization fits avoid quadratic large-page work.
    let render = |output: &mut Value, limit: usize| -> bool {
        let count = segments.partition_point(|segment| segment.begin <= limit);
        let Some(segment) = count.checked_sub(1).and_then(|index| segments.get(index)) else {
            return false;
        };
        let cut = content.floor_char_boundary(limit.min(segment.end)).max(segment.begin);
        let end = if cut == segment.end && segment.complete {
            Position { line: segment.line.saturating_add(1), byte: 0 }
        } else {
            Position {
                line: segment.line,
                byte: segment.offset.saturating_add((cut - segment.begin) as u64),
            }
        };
        output["content"] = json!(&content[..cut]);
        output["end_line"] = json!(segment.line);
        output["source_range"] = json!(Span { start: span.start, end });
        let done = end >= requested.end || (page.eof && end >= page.span.end);
        output["next_line"] = if done { Value::Null } else { json!(end.line) };
        output["next_line_byte_offset"] = if done { Value::Null } else { json!(end.byte) };
        output["eof"] = json!(page.eof && end >= page.span.end);
        end > span.start
    };
    if render(&mut output, content.len()) && output.to_string().len() <= maximum {
        return Ok(Some(output));
    }
    let mut lower = 0;
    let mut upper = content.len();
    let mut best = None;
    while lower <= upper {
        let middle = lower + (upper - lower) / 2;
        let progress = render(&mut output, middle);
        if !progress {
            lower = middle.saturating_add(1);
            continue;
        }
        if output.to_string().len() <= maximum {
            best = Some(middle);
            lower = middle.saturating_add(1);
        } else if middle == 0 {
            break;
        } else {
            upper = middle - 1;
        }
    }
    if let Some(best) = best {
        render(&mut output, best);
        Ok(Some(output))
    } else {
        Ok(None)
    }
}
