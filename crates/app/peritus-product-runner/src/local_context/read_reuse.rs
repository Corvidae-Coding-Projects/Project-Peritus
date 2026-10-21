//! Delivery coverage derived from the existing role-scoped durable observation archive.

mod page;
mod view;

use super::{memory::LocalMemory, record::ArchiveKind, tools::source_handle};
use peritus_agent::{DeveloperLoopError, DeveloperToolObservation};
use peritus_model_protocol::{CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
struct Position {
    line: u64,
    byte: u64,
}

impl Position {
    const END: Self = Self { line: u64::MAX, byte: u64::MAX };
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct Span {
    start: Position,
    end: Position,
}

impl Span {
    fn intersect(self, other: Self) -> Option<Self> {
        let span = Self { start: self.start.max(other.start), end: self.end.min(other.end) };
        (span.start < span.end).then_some(span)
    }
}

#[derive(Clone)]
struct Page {
    id: u64,
    path: String,
    digest: String,
    span: Span,
    eof: bool,
}

impl Page {
    fn from_value(id: u64, value: &Value) -> Option<Self> {
        value.get("content")?.as_str()?;
        let span: Span = serde_json::from_value(value.get("source_range")?.clone()).ok()?;
        if span.start.line == 0 || span.start > span.end {
            return None;
        }
        Some(Self {
            id,
            path: path_key(value.get("path")?.as_str()?)?,
            digest: value.get("source_sha256")?.as_str()?.to_owned(),
            span,
            eof: value.get("eof").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

/// Only metadata stays resident; exact page bodies already live in the durable archive.
#[derive(Default)]
pub(super) struct ReadReuse {
    indexed: usize,
    pages: BTreeMap<u64, Page>,
    paths: BTreeMap<String, Vec<u64>>,
    visible: BTreeMap<u64, Vec<Span>>,
    pending: BTreeSet<[u8; 32]>,
    rendered: BTreeMap<[u8; 32], (u64, Span)>,
    last_action: Option<(u64, String)>,
}

impl ReadReuse {
    pub(super) fn has_page(&self, id: u64) -> bool {
        self.pages.contains_key(&id)
    }
    pub(super) fn clear_rendered(&mut self) {
        self.rendered.clear();
    }
    pub(super) fn record_output(&mut self, output: &DeveloperToolObservation) {
        if serde_json::from_slice::<Value>(output.output.canonical_bytes())
            .ok()
            .and_then(|value| Page::from_value(0, &value))
            .is_some()
        {
            self.pending
                .insert(peritus_codec::sha256(output.output.canonical_bytes()).into_bytes());
        }
    }

    fn refresh(&mut self, memory: &LocalMemory) -> Result<(), DeveloperLoopError> {
        for source in &memory.sources[self.indexed..] {
            if source.kind != ArchiveKind::ToolOutput || source.is_error {
                continue;
            }
            let Some(call) = &source.call else { continue };
            if !matches!(
                call.name.as_str(),
                "workspace_read" | "workspace_write" | "workspace_patch" | "workspace_remove"
            ) {
                continue;
            }
            // This is a rebuildable optimization. An unavailable old page must not
            // create a new blocker before an ordinary authorized source read.
            let Ok(bytes) = memory.artifact(source.sequence) else { continue };
            let Ok(value) = serde_json::from_slice::<Value>(&bytes) else { continue };
            if call.name != "workspace_read" {
                if value.get("changed").and_then(Value::as_bool) == Some(true) {
                    self.last_action = Some((source.sequence, call.name.clone()));
                }
                continue;
            }
            // Pre-migration reads without exact byte coverage are conservatively refreshed.
            let Some(page) = Page::from_value(source.sequence, &value) else { continue };
            if self.pending.contains(source.artifact.digest.as_bytes()) {
                self.visible.entry(page.id).or_default().push(page.span);
            }
            self.paths.entry(page.path.clone()).or_default().push(page.id);
            self.pages.insert(page.id, page);
        }
        self.indexed = memory.sources.len();
        Ok(())
    }

    fn plan(
        &mut self,
        memory: &LocalMemory,
        arguments: &Value,
    ) -> Result<Option<Value>, DeveloperLoopError> {
        self.refresh(memory)?;
        let Some(path) = arguments.get("path").and_then(Value::as_str) else { return Ok(None) };
        let Some(key) = path_key(path) else { return Ok(None) };
        if !super::assembly::evidence::may_reuse_read(memory, path) {
            return Ok(None);
        }
        let Some(ids) = self.paths.get(&key) else { return Ok(None) };
        let Some(latest) = ids.last().and_then(|id| self.pages.get(id)) else { return Ok(None) };
        let pages = ids
            .iter()
            .filter_map(|id| self.pages.get(id))
            .filter(|page| page.digest == latest.digest)
            .collect::<Vec<_>>();
        let start = Position {
            line: arguments.get("start_line").and_then(Value::as_u64).unwrap_or(1),
            byte: arguments.get("line_byte_offset").and_then(Value::as_u64).unwrap_or(0),
        };
        if start.line == 0 {
            return Ok(None);
        }
        let mut end = arguments
            .get("end_line")
            .and_then(Value::as_u64)
            .and_then(|line| line.checked_add(1))
            .map_or(Position::END, |line| Position { line, byte: 0 });
        if end < start {
            return Ok(None);
        }
        let eof = pages.iter().filter(|page| page.eof).map(|page| page.span.end).min();
        if let Some(eof) = eof {
            end = end.min(eof).max(start);
        }
        let requested = Span { start, end };
        let mut visible =
            pages
                .iter()
                .flat_map(|page| {
                    self.visible.get(&page.id).into_iter().flatten().filter_map(move |span| {
                        span.intersect(requested).map(|span| (span, page.id))
                    })
                })
                .collect::<Vec<_>>();
        visible.sort_unstable_by_key(|(span, _)| span.start);
        let mut gaps = Vec::new();
        let mut cursor = start;
        for (span, _) in &visible {
            if span.start > cursor {
                gaps.push(Span { start: cursor, end: span.start });
            }
            cursor = cursor.max(span.end);
        }
        if cursor < end {
            gaps.push(Span { start: cursor, end });
        }
        // A nonzero offset must be validated against an actual line body, even if its
        // logical line interval was seen. An invalid UTF-8/oversized offset falls back.
        if start.byte > 0
            && !pages.iter().any(|page| {
                page.span.start <= start
                    && start < page.span.end
                    && page::valid_offset(memory, page, start).unwrap_or(false)
            })
        {
            return Ok(None);
        }
        let maximum = arguments
            .get("max_bytes")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(crate::developer_tools::DEFAULT_DOCUMENT_READ_BYTES);
        let mut references = visible
            .iter()
            .map(|(_, id)| *id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|id| source_handle(memory, id))
            .collect::<Vec<_>>();
        let Some(gap) = gaps.first().copied() else {
            // No bytes are resent. The original pages are in this exact model view.
            let mut value = json!({"path":path,"source_sha256":latest.digest,"read_reuse":{"state":"already_visible","requested":requested,"sources":references,"source_count":references.len()}});
            while value.to_string().len() > maximum && references.pop().is_some() {
                value["read_reuse"]["sources"] = json!(references);
            }
            return Ok((value.to_string().len() <= maximum).then_some(value));
        };
        let resume = gaps.get(1).map(|span| span.start);
        // Prefer the page that reaches furthest into the missing interval, independent
        // of which overlapping read happened most recently.
        let stored = pages
            .iter()
            .filter(|page| page.span.start <= gap.start && gap.start < page.span.end)
            .max_by_key(|page| page.span.end);
        let mut value = if let Some(page) = stored {
            let Some(mut value) = page::slice(memory, page, gap, maximum)? else { return Ok(None) };
            value["path"] = json!(path);
            value["read_reuse"] = json!({"state":"restored","sources":[source_handle(memory,page.id)],"gap_end":gap.end,"resume":resume});
            value
        } else if !visible.is_empty() {
            // Read only the next missing span. The normal executor admits and executes it.
            let end_line =
                if gap.end.byte == 0 { gap.end.line.saturating_sub(1) } else { gap.end.line };
            let mut narrowed = json!({"start_line":gap.start.line,"line_byte_offset":gap.start.byte,"end_line":end_line});
            if gap.end.byte > 0 {
                narrowed["end_line_byte_offset"] = json!(gap.end.byte);
            }
            json!({"path":path,"source_sha256":latest.digest,"read_reuse":{"state":"read_missing","reused_pages":references.len(),"arguments":narrowed,"gap_end":gap.end,"resume":resume}})
        } else {
            return Ok(None);
        };
        resume_page(&mut value);
        Ok(Some(value))
    }
}

pub(super) fn refresh(memory: &mut LocalMemory) -> Result<(), DeveloperLoopError> {
    let mut index = std::mem::take(&mut memory.read_reuse);
    let result = index.refresh(memory);
    memory.read_reuse = index;
    result
}

pub(super) fn plan(
    memory: &mut LocalMemory,
    call: &CompletedToolCall,
) -> Result<Option<DeveloperToolObservation>, DeveloperLoopError> {
    let Ok(arguments) = serde_json::from_slice::<Value>(call.arguments().canonical_bytes()) else {
        return Ok(None);
    };
    let mut index = std::mem::take(&mut memory.read_reuse);
    let result = index.plan(memory, &arguments);
    memory.read_reuse = index;
    let Some(value) = result.unwrap_or_else(|_| {
        crate::efficiency::event(
            &memory.trace_path,
            json!({"event":"read-reuse-unavailable","action":"ordinary-read"}),
        );
        None
    }) else {
        return Ok(None);
    };
    crate::efficiency::event(
        &memory.trace_path,
        json!({"event":"read-reuse","state":value["read_reuse"]["state"],"invocation":memory.transcript.request_prefix,"semantics":"candidate; workspace executor independently admits permissions and freshness"}),
    );
    Ok(CanonicalJson::parse(&value.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION))
        .ok()
        .map(|output| DeveloperToolObservation { output, is_error: false }))
}

/// Fit the actual reused response, including its receipt, before returning it to the model.
pub(crate) fn fit_response(
    mut value: Value,
    arguments: &Value,
) -> Result<Option<Value>, DeveloperLoopError> {
    let maximum = arguments
        .get("max_bytes")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(crate::developer_tools::DEFAULT_DOCUMENT_READ_BYTES);
    let mut receipt = value["read_reuse"].clone();
    if let Some(fields) = receipt.as_object_mut() {
        fields.remove("arguments");
        fields.remove("gap_end");
        fields.remove("resume");
    }
    if let Some(page) = Page::from_value(0, &value) {
        let reserve = receipt.to_string().len().saturating_add(16);
        let mut sized = value.clone();
        sized["read_reuse"] = receipt.clone();
        if sized.to_string().len() > maximum {
            let Some(mut fitted) =
                page::slice_value(&value, &page, page.span, maximum.saturating_sub(reserve))?
            else {
                return Ok(None);
            };
            fitted["read_reuse"] = value["read_reuse"].clone();
            value = fitted;
        }
        resume_page(&mut value);
    }
    value["read_reuse"] = receipt;
    Ok((value.to_string().len() <= maximum).then_some(value))
}

fn resume_page(value: &mut Value) {
    let Some(end) = value
        .get("source_range")
        .and_then(|range| range.get("end"))
        .and_then(|end| serde_json::from_value::<Position>(end.clone()).ok())
    else {
        return;
    };
    let Some(gap_end) = value["read_reuse"]
        .get("gap_end")
        .and_then(|end| serde_json::from_value::<Position>(end.clone()).ok())
    else {
        return;
    };
    if end >= gap_end {
        let next = value["read_reuse"]
            .get("resume")
            .and_then(|next| serde_json::from_value::<Position>(next.clone()).ok());
        value["next_line"] = next.map_or(Value::Null, |next| json!(next.line));
        value["next_line_byte_offset"] = next.map_or(Value::Null, |next| json!(next.byte));
    }
}

fn path_key(path: &str) -> Option<String> {
    let mut normalized = std::path::PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
            _ => return None,
        }
    }
    normalized.to_str().filter(|path| !path.is_empty()).map(str::to_owned)
}

pub(super) fn progress(memory: &LocalMemory) -> String {
    memory.read_reuse.last_action.as_ref().map_or(String::new(), |(id,name)| format!("\nLast completed changed workspace operation: {name}, source {}. This records an operation, not task completion. Continue the accepted design/current task from retained progress; a source refresh does not reopen startup discovery.",source_handle(memory,*id)))
}

pub(super) use view::{observe_view, preview, register_rendered};

/// A complete typed file page fitted to a provider envelope, preserving source coordinates.
pub(in crate::local_context) fn provider_page(
    memory: &LocalMemory,
    id: u64,
    value: &Value,
    maximum: usize,
) -> Result<Option<Value>, DeveloperLoopError> {
    let Some(page) = Page::from_value(id, value) else { return Ok(None) };
    if let Some(mut value) = page::slice_value(value, &page, page.span, maximum)? {
        value["local_context"] = json!({"handle":source_handle(memory,id)});
        return Ok(Some(value));
    }
    let mut empty = value.clone();
    empty["content"] = json!("");
    empty["source_range"] = json!(Span { start: page.span.start, end: page.span.start });
    empty["end_line"] = json!(page.span.start.line);
    empty["next_line"] = json!(page.span.start.line);
    empty["next_line_byte_offset"] = json!(page.span.start.byte);
    empty["eof"] = json!(false);
    empty["local_context"] = json!({"handle":source_handle(memory,id)});
    Ok(Some(empty))
}
