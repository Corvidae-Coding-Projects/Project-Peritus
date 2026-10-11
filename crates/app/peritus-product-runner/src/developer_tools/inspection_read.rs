//! Byte-bounded, resumable line reads shared by workspace and reference tools.

use std::{
    fs,
    io::{BufRead as _, BufReader},
    path::Path,
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    inspection_cancellation::InspectionCancellation,
    path::tool,
    wire::{bounded_usize, object},
};

use super::inspection::MAX_PAGE_BYTES;

#[derive(Clone)]
struct Line {
    number: usize,
    offset: u64,
    text: String,
    complete: bool,
}

pub(super) fn read(
    path: &Path,
    arguments: &Value,
    extra: &[(&'static str, Value)],
    cancellation: &InspectionCancellation,
) -> Result<Value, DeveloperLoopError> {
    cancellation.check()?;
    let start = bounded_usize(arguments, "start_line", 1, 1, usize::MAX);
    let end = bounded_usize(arguments, "end_line", start.saturating_add(499), start, usize::MAX);
    let requested_offset = bounded_usize(arguments, "line_byte_offset", 0, 0, usize::MAX);
    let requested_offset =
        u64::try_from(requested_offset).map_err(|_| tool("line byte offset overflow"))?;
    let max_bytes = bounded_usize(
        arguments,
        "max_bytes",
        super::DEFAULT_INSPECTION_PAGE_BYTES,
        256,
        MAX_PAGE_BYTES,
    );
    let file = fs::File::open(path).map_err(|error| tool(error.to_string()))?;
    let mut reader = BufReader::new(file);
    let mut current = 1_usize;
    // Bound captured source bytes directly, then fit the actual JSON response below.
    // A worst-case escaping allowance underfills ordinary source pages and forces
    // needless continuation reads even when the complete file would fit.
    let mut raw_budget = max_bytes;
    let mut lines = Vec::new();
    let mut next = None;

    while current <= end {
        cancellation.check()?;
        if current >= start && raw_budget == 0 {
            if reader.fill_buf().map_err(|error| tool(error.to_string()))?.is_empty() {
                break;
            }
            next = Some((current, 0));
            break;
        }
        let offset = if current == start { requested_offset } else { 0 };
        let mut discard_budget = 0;
        let budget = if current < start { &mut discard_budget } else { &mut raw_budget };
        let Some(line) = read_line(&mut reader, offset, budget, cancellation)? else { break };
        if current >= start && offset > line.total_bytes {
            return Err(tool("line byte offset is beyond the selected line"));
        }
        let complete = line.complete
            && u64::try_from(line.text.len()).unwrap_or(u64::MAX)
                >= line.total_bytes.saturating_sub(offset);
        let next_line = if current >= start && !complete {
            Some((current, offset.saturating_add(line.text.len() as u64)))
        } else {
            None
        };
        if current >= start {
            lines.push(Line { number: current, offset, text: line.text, complete });
        }
        current = current.saturating_add(1);
        if let Some(next_line) = next_line {
            next = Some(next_line);
            break;
        }
    }

    if next.is_none() && arguments.get("end_line").is_none() && current > end {
        cancellation.check()?;
        if !reader.fill_buf().map_err(|error| tool(error.to_string()))?.is_empty() {
            next = Some((current, 0));
        }
    }

    let mut value = response(extra, &lines, start, next);
    let mut omitted = None;
    while encoded_len(&value)? > max_bytes {
        if let Some(last) = lines.pop() {
            next = Some((last.number, last.offset));
            value = response(extra, &lines, start, next);
            omitted = Some(last);
        } else {
            return Err(tool("response metadata exceeds max_bytes"));
        }
    }
    if let Some(last) = omitted {
        if let Some((prefix, cursor)) = fit_line_prefix(extra, &lines, &last, start, max_bytes)? {
            lines.push(prefix);
            next = Some(cursor);
        } else if lines.is_empty() {
            return Err(tool("max_bytes leaves no room for a nonempty line prefix"));
        }
    }
    if let Some(last) = lines.last()
        && !last.complete
    {
        next = Some((last.number, last.offset.saturating_add(last.text.len() as u64)));
    }
    value = response(extra, &lines, start, next);
    if encoded_len(&value)? > max_bytes {
        return Err(tool("response metadata exceeds max_bytes"));
    }
    Ok(value)
}

fn fit_line_prefix(
    extra: &[(&'static str, Value)],
    preceding: &[Line],
    last: &Line,
    start: usize,
    max_bytes: usize,
) -> Result<Option<(Line, (usize, u64))>, DeveloperLoopError> {
    let metadata = response(extra, preceding, start, Some((last.number, last.offset)));
    if encoded_len(&metadata)? > max_bytes {
        return Err(tool("response metadata exceeds max_bytes"));
    }
    let boundaries = last
        .text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(last.text.len()))
        .collect::<Vec<_>>();
    let mut lower = 0;
    let mut upper = boundaries.len() - 1;
    while lower < upper {
        let middle = lower + (upper - lower).div_ceil(2);
        let length = boundaries[middle];
        let line = Line { text: last.text[..length].to_owned(), complete: false, ..last.clone() };
        let cursor = (last.number, last.offset.saturating_add(length as u64));
        let mut page_lines = preceding.to_vec();
        page_lines.push(line);
        let page = response(extra, &page_lines, start, Some(cursor));
        if encoded_len(&page)? <= max_bytes {
            lower = middle;
        } else {
            upper = middle - 1;
        }
    }
    let length = boundaries[lower];
    if length == 0 {
        return Ok(None);
    }
    Ok(Some((
        Line { text: last.text[..length].to_owned(), complete: false, ..last.clone() },
        (last.number, last.offset.saturating_add(length as u64)),
    )))
}

struct ReadLine {
    text: String,
    total_bytes: u64,
    complete: bool,
}

fn read_line(
    reader: &mut BufReader<fs::File>,
    offset: u64,
    budget: &mut usize,
    cancellation: &InspectionCancellation,
) -> Result<Option<ReadLine>, DeveloperLoopError> {
    let mut captured = Vec::new();
    let mut total = 0_u64;
    let mut saw_bytes = false;
    loop {
        cancellation.check()?;
        let available = reader.fill_buf().map_err(|error| tool(error.to_string()))?;
        if available.is_empty() {
            if !saw_bytes {
                return Ok(None);
            }
            return finish_line(captured, total, true).map(Some);
        }
        let count =
            available.iter().position(|byte| *byte == b'\n').map_or(available.len(), |i| i + 1);
        let segment = &available[..count];
        let content = segment.strip_suffix(b"\n").unwrap_or(segment);
        for byte in content {
            saw_bytes = true;
            if total >= offset && *budget > 0 {
                captured.push(*byte);
                *budget -= 1;
            }
            total = total.checked_add(1).ok_or_else(|| tool("line byte count overflow"))?;
        }
        let complete = segment.last() == Some(&b'\n');
        reader.consume(count);
        if complete {
            return finish_line(captured, total, true).map(Some);
        }
    }
}

fn finish_line(
    mut bytes: Vec<u8>,
    total_bytes: u64,
    complete: bool,
) -> Result<ReadLine, DeveloperLoopError> {
    if let Err(error) = std::str::from_utf8(&bytes) {
        if error.error_len().is_some() {
            return Err(tool("selected line is not UTF-8"));
        }
        bytes.truncate(error.valid_up_to());
    }
    let text = String::from_utf8(bytes).map_err(|_| tool("selected line is not UTF-8"))?;
    Ok(ReadLine { text, total_bytes, complete })
}

fn response(
    extra: &[(&'static str, Value)],
    lines: &[Line],
    start: usize,
    next: Option<(usize, u64)>,
) -> Value {
    let mut fields = extra.to_vec();
    let mut content = String::new();
    for line in lines {
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&line.number.to_string());
        content.push_str(": ");
        content.push_str(&line.text);
    }
    fields.extend([
        ("content", Value::String(content)),
        ("start_line", Value::from(start)),
        ("end_line", Value::from(lines.last().map_or(start, |line| line.number))),
        ("next_line", next.map_or(Value::Null, |(line, _)| Value::from(line))),
        ("next_line_byte_offset", next.map_or(Value::Null, |(_, offset)| Value::from(offset))),
    ]);
    object(fields)
}

fn encoded_len(value: &Value) -> Result<usize, DeveloperLoopError> {
    serde_json::to_vec(value).map(|bytes| bytes.len()).map_err(|error| tool(error.to_string()))
}
