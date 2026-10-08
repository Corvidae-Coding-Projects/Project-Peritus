//! Exact byte-range retrieval and lossless literal-search/state pages with scope-bound cursors.

use super::super::record::{ArchiveKind, ArchivedObservation};
use super::{LocalMemory, entry_id, error, handle, hex, rejected, sequence};
use peritus_agent::DeveloperLoopError;
use peritus_artifact_store::ArtifactReadHandle;
use peritus_codec::sha256;
use peritus_context::ContextNodeId;
use serde::Deserialize;
use serde_json::Value;

const STREAM_CHUNK_BYTES_U64: u64 = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    observation_ids: Vec<String>,
    #[serde(default)]
    entry_ids: Vec<String>,
    query: Option<String>,
    cursor: Option<String>,
    #[serde(default)]
    offset: u64,
    #[serde(default)]
    end_offset: Option<u64>,
    max_bytes: usize,
}

#[derive(Clone, Copy)]
struct Position {
    index: u64,
    offset: u64,
}

#[derive(Clone, Copy)]
struct MatchRange {
    start: u64,
    end: u64,
}

#[derive(Clone, Copy)]
struct SourceRange {
    start: u64,
    end: u64,
    matched: Option<MatchRange>,
}

#[derive(Clone, Copy)]
struct SourceFragmentSpec<'a> {
    source: &'a ArchivedObservation,
    range: SourceRange,
}

#[derive(Clone, Copy)]
struct EntryFragmentSpec {
    mode: &'static str,
    binding: [u8; 32],
    count: u64,
    index: u64,
    start: u64,
    entry_end: u64,
}

#[derive(Clone)]
struct Continuation {
    cursor: Option<String>,
    next_handle_index: Option<u64>,
}

#[derive(Clone, Copy)]
enum FragmentEncoding {
    Utf8,
    Hex,
}

pub(in crate::local_context) fn execute(
    memory: &mut LocalMemory,
    bytes: &[u8],
) -> Result<Value, DeveloperLoopError> {
    let Ok(request) = serde_json::from_slice::<Read>(bytes) else {
        return Ok(rejected(memory, "invalid read schema"));
    };
    if request.max_bytes == 0 || request.max_bytes > memory.config.max_read_bytes {
        return Ok(rejected(
            memory,
            &format!("max_bytes must be within 1..={}", memory.config.max_read_bytes),
        ));
    }
    if request.query.as_ref().is_some_and(String::is_empty) {
        return Ok(rejected(memory, "query must be null or a nonempty literal search"));
    }
    let ids = match request
        .observation_ids
        .iter()
        .map(|id| sequence(memory, id))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(ids) => ids,
        Err(reason) => return Ok(rejected(memory, &reason.to_string())),
    };
    let entry_ids = match request
        .entry_ids
        .iter()
        .map(|id| entry_id(id))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(ids) => ids,
        Err(reason) => return Ok(rejected(memory, &reason.to_string())),
    };
    if entry_ids.len() > memory.limits.entries() {
        return Ok(rejected(memory, "entry_ids exceeds the physical request bound"));
    }
    if entry_ids.iter().enumerate().any(|(index, id)| entry_ids[..index].contains(id)) {
        return Ok(rejected(memory, "duplicate working-entry handle"));
    }
    let state = ids.is_empty() && entry_ids.is_empty() && request.query.is_none();
    if !ids.is_empty() && !entry_ids.is_empty() {
        return Ok(rejected(
            memory,
            "observation and working-entry handles cannot be combined",
        ));
    }
    if (!ids.is_empty() || !entry_ids.is_empty()) && request.query.is_some() {
        return Ok(rejected(memory, "explicit handles cannot be combined with a search query"));
    }
    if (state || !entry_ids.is_empty()) && (request.offset != 0 || request.end_offset.is_some()) {
        return Ok(rejected(memory, "working-entry pages do not accept byte-range bounds"));
    }
    if request.query.is_some() && request.end_offset.is_some() {
        return Ok(rejected(memory, "end_offset is only valid for explicit handle ranges"));
    }
    if request.end_offset.is_some_and(|end| end < request.offset) {
        return Ok(rejected(memory, "end_offset precedes offset"));
    }

    let mode = if state {
        "e"
    } else if !entry_ids.is_empty() {
        "i"
    } else if ids.is_empty() {
        "s"
    } else {
        "h"
    };
    let binding = request_binding(memory, mode, &ids, &entry_ids, &request)?;
    let initial = Position { index: 0, offset: request.offset };
    let position = match parse_cursor(memory, request.cursor.as_deref(), mode, binding, initial) {
        Ok(position) => position,
        Err(reason) => return Ok(rejected(memory, &reason.to_string())),
    };
    if let Err(reason) = validate_position(memory, &request, &ids, &entry_ids, mode, position) {
        return Ok(rejected(memory, &reason.to_string()));
    }

    memory.retrieval_calls =
        memory.retrieval_calls.checked_add(1).ok_or_else(|| error("retrieval counter overflow"))?;
    if state || !entry_ids.is_empty() {
        entry_page(
            memory,
            &request,
            mode,
            (!entry_ids.is_empty()).then_some(entry_ids.as_slice()),
            binding,
            position,
        )
    } else if ids.is_empty() {
        search_page(memory, &request, binding, position)
    } else {
        explicit_page(memory, &request, &ids, binding, position)
    }
}

fn search_page(
    memory: &LocalMemory,
    request: &Read,
    binding: [u8; 32],
    mut position: Position,
) -> Result<Value, DeveloperLoopError> {
    let query = request.query.as_deref().ok_or_else(|| error("missing search query"))?.as_bytes();
    let mut response = source_response(memory);
    if !base_fits(memory, request.max_bytes, &response)? {
        return Ok(rejected(memory, "max_bytes cannot fit source response metadata"));
    }
    let source_count =
        u64::try_from(memory.sources.len()).map_err(|_| error("source count overflow"))?;
    while position.index < source_count {
        check_cancelled(memory)?;
        let index = usize::try_from(position.index).map_err(|_| error("source index overflow"))?;
        let source = memory.sources.get(index).ok_or_else(|| error("source index unavailable"))?;
        if excluded_from_search(source)
            || u64::try_from(query.len()).map_err(|_| error("query size overflow"))?
                > source.artifact.bytes
        {
            position = Position {
                index: position.index.checked_add(1).ok_or_else(|| error("source index overflow"))?,
                offset: 0,
            };
            continue;
        }
        let mut searcher = ArtifactSearcher::open(memory, source, query, position.offset)?;
        while let Some(hit) = searcher.next_match(memory)? {
            let query_bytes = u64::try_from(query.len()).map_err(|_| error("query size overflow"))?;
            let match_end = hit.checked_add(query_bytes).ok_or_else(|| error("match offset overflow"))?;
            let next = Position {
                index: position.index,
                offset: hit.checked_add(1).ok_or_else(|| error("search cursor overflow"))?,
            };
            let continuation = Continuation {
                cursor: Some(page_cursor(memory, "s", binding, next)),
                next_handle_index: None,
            };
            let appended = append_source_fragment(
                memory,
                request,
                &mut response,
                searcher.reader_mut(),
                SourceFragmentSpec {
                    source,
                    range: SourceRange {
                        start: hit,
                        end: match_end,
                        matched: Some(MatchRange { start: hit, end: match_end }),
                    },
                },
                |_| continuation.clone(),
            )?;
            if appended.is_none() {
                if response_sources_empty(&response) {
                    return Ok(rejected(
                        memory,
                        "max_bytes cannot fit source metadata and one byte; increase it",
                    ));
                }
                return Ok(response);
            }
            position = next;
        }
        position = Position {
            index: position.index.checked_add(1).ok_or_else(|| error("source index overflow"))?,
            offset: 0,
        };
    }
    response["cursor"] = Value::Null;
    Ok(response)
}

fn explicit_page(
    memory: &LocalMemory,
    request: &Read,
    ids: &[u64],
    binding: [u8; 32],
    mut position: Position,
) -> Result<Value, DeveloperLoopError> {
    let mut response = source_response(memory);
    if !base_fits(memory, request.max_bytes, &response)? {
        return Ok(rejected(memory, "max_bytes cannot fit source response metadata"));
    }
    let count = u64::try_from(ids.len()).map_err(|_| error("handle count overflow"))?;
    while position.index < count {
        check_cancelled(memory)?;
        let index = usize::try_from(position.index).map_err(|_| error("handle index overflow"))?;
        let source = memory.archived(ids[index])?;
        let range_end = request.end_offset.unwrap_or(source.artifact.bytes);
        let mut reader = memory.store.open_artifact(source.artifact)?;
        let handle_index = position.index;
        let next_handle_offset = request.offset;
        let appended = append_source_fragment(
            memory,
            request,
            &mut response,
            &mut reader,
            SourceFragmentSpec {
                source,
                range: SourceRange { start: position.offset, end: range_end, matched: None },
            },
            |end| {
                let next = if end < range_end {
                    Position { index: handle_index, offset: end }
                } else {
                    Position {
                        index: handle_index.saturating_add(1),
                        offset: next_handle_offset,
                    }
                };
                if next.index < count {
                    Continuation {
                        cursor: Some(page_cursor(memory, "h", binding, next)),
                        next_handle_index: Some(next.index),
                    }
                } else {
                    Continuation { cursor: None, next_handle_index: None }
                }
            },
        )?;
        let Some(end) = appended else {
            if response_sources_empty(&response) {
                return Ok(rejected(
                    memory,
                    "max_bytes cannot fit source metadata and one byte; increase it",
                ));
            }
            return Ok(response);
        };
        position = if end < range_end {
            Position { index: handle_index, offset: end }
        } else {
            Position {
                index: handle_index.checked_add(1).ok_or_else(|| error("handle index overflow"))?,
                offset: request.offset,
            }
        };
    }
    response["cursor"] = Value::Null;
    response["next_handle_index"] = Value::Null;
    Ok(response)
}

fn append_source_fragment(
    memory: &LocalMemory,
    request: &Read,
    response: &mut Value,
    reader: &mut ArtifactReadHandle,
    spec: SourceFragmentSpec<'_>,
    continuation: impl Fn(u64) -> Continuation,
) -> Result<Option<u64>, DeveloperLoopError> {
    check_cancelled(memory)?;
    let remaining = spec
        .range
        .end
        .checked_sub(spec.range.start)
        .ok_or_else(|| error("invalid source range"))?;
    let response_limit =
        u64::try_from(request.max_bytes).map_err(|_| error("read response size overflow"))?;
    let maximum = remaining.min(response_limit);
    let maximum = usize::try_from(maximum).map_err(|_| error("read fragment size overflow"))?;
    let bytes = if maximum == 0 {
        Vec::new()
    } else {
        let chunk = reader
            .read_chunk_at(spec.range.start, maximum)
            .map_err(|_| error("read verified artifact range"))?
            .ok_or_else(|| error("verified artifact ended before requested range"))?;
        if chunk.offset() != spec.range.start {
            return Err(error("artifact reader returned a discontinuous range"));
        }
        chunk.bytes().to_vec()
    };
    check_cancelled(memory)?;

    let fit = |length: usize, encoding: FragmentEncoding| -> Result<bool, DeveloperLoopError> {
        let end = spec
            .range
            .start
            .checked_add(u64::try_from(length).map_err(|_| error("fragment length overflow"))?)
            .ok_or_else(|| error("fragment end overflow"))?;
        let candidate = source_candidate(
            memory,
            response,
            spec,
            end,
            encoding,
            &bytes[..length],
            continuation(end),
        )?;
        Ok(encoded_len(&candidate)? <= request.max_bytes)
    };
    let Some((length, encoding)) = largest_fragment(&bytes, fit)? else {
        return Ok(None);
    };
    if remaining != 0 && length == 0 {
        return Ok(None);
    }
    let end = spec
        .range
        .start
        .checked_add(u64::try_from(length).map_err(|_| error("fragment length overflow"))?)
        .ok_or_else(|| error("fragment end overflow"))?;
    *response = source_candidate(
        memory,
        response,
        spec,
        end,
        encoding,
        &bytes[..length],
        continuation(end),
    )?;
    Ok(Some(end))
}

fn source_candidate(
    memory: &LocalMemory,
    response: &Value,
    spec: SourceFragmentSpec<'_>,
    end: u64,
    encoding: FragmentEncoding,
    bytes: &[u8],
    continuation: Continuation,
) -> Result<Value, DeveloperLoopError> {
    let mut item = Value::from_iter([
        ("handle", Value::from(handle(memory, spec.source.sequence))),
        ("sha256", Value::from(hex(spec.source.artifact.digest.as_bytes()))),
        ("total_bytes", Value::from(spec.source.artifact.bytes)),
        ("offset", Value::from(spec.range.start)),
        ("end_offset", Value::from(end)),
        (
            "next_offset",
            if end < spec.range.end { Value::from(end) } else { Value::Null },
        ),
        (
            "encoding",
            Value::from(match encoding {
                FragmentEncoding::Utf8 => "utf8",
                FragmentEncoding::Hex => "hex",
            }),
        ),
        ("data", fragment_value(bytes, encoding)?),
        ("is_error", Value::from(spec.source.is_error)),
        (
            "match_offset",
            spec.range.matched.map_or(Value::Null, |range| Value::from(range.start)),
        ),
        (
            "match_end",
            spec.range.matched.map_or(Value::Null, |range| Value::from(range.end)),
        ),
    ]);
    if spec.range.matched.is_none() {
        item.as_object_mut()
            .ok_or_else(|| error("invalid source item"))?
            .remove("match_offset");
        item.as_object_mut()
            .ok_or_else(|| error("invalid source item"))?
            .remove("match_end");
    }
    let mut candidate = response.clone();
    candidate["sources"]
        .as_array_mut()
        .ok_or_else(|| error("invalid read response"))?
        .push(item);
    candidate["cursor"] = continuation.cursor.map_or(Value::Null, Value::from);
    candidate["next_handle_index"] =
        continuation.next_handle_index.map_or(Value::Null, Value::from);
    Ok(candidate)
}

fn entry_page(
    memory: &LocalMemory,
    request: &Read,
    mode: &'static str,
    selected: Option<&[ContextNodeId]>,
    binding: [u8; 32],
    mut position: Position,
) -> Result<Value, DeveloperLoopError> {
    if !memory.derived_memory_allowed() {
        return Ok(rejected(
            memory,
            "role policy excludes derived memory; read exact source handles",
        ));
    }
    let entries = memory
        .state
        .active_entries(memory.state.binding())
        .map_err(|_| error("state read scope mismatch"))?;
    let count = u64::try_from(selected.map_or(entries.len(), <[ContextNodeId]>::len))
        .map_err(|_| error("entry count overflow"))?;
    let mut response = Value::from_iter([
        ("base_revision", Value::from(memory.model_revision)),
        ("generation", Value::from(memory.store.generation())),
        ("observations", Value::from(memory.sources.len())),
        ("pending_operations", Value::from(memory.transcript.pending.len())),
        ("authority", Value::from("none")),
        ("entries", Value::Array(vec![])),
        ("entry_fragment", Value::Null),
        ("cursor", Value::Null),
    ]);
    if !base_fits(memory, request.max_bytes, &response)? {
        return Ok(rejected(memory, "max_bytes cannot fit state response metadata"));
    }
    while position.index < count {
        check_cancelled(memory)?;
        let index = usize::try_from(position.index).map_err(|_| error("entry index overflow"))?;
        let entry = match selected {
            Some(ids) => memory.state
                .entry(memory.state.binding(), ids[index])
                .map_err(|_| error("selected working entry is unavailable"))?,
            None => entries
                .get(index)
                .copied()
                .ok_or_else(|| error("entry index unavailable"))?,
        };
        let item = super::entry_view(memory, entry);
        let bytes = serde_json::to_vec(&item).map_err(|_| error("encode state entry"))?;
        let length = u64::try_from(bytes.len()).map_err(|_| error("entry size overflow"))?;
        if position.offset > length {
            return Ok(rejected(memory, "entry fragment cursor is out of range"));
        }
        if position.offset == 0 {
            let next = Position {
                index: position.index.checked_add(1).ok_or_else(|| error("entry index overflow"))?,
                offset: 0,
            };
            let mut candidate = response.clone();
            candidate["entries"]
                .as_array_mut()
                .ok_or_else(|| error("invalid state response"))?
                .push(item);
            candidate["cursor"] = if next.index < count {
                Value::from(page_cursor(memory, mode, binding, next))
            } else {
                Value::Null
            };
            if encoded_len(&candidate)? <= request.max_bytes {
                response = candidate;
                position = next;
                continue;
            }
        }

        let start = usize::try_from(position.offset).map_err(|_| error("entry offset overflow"))?;
        let available = bytes.len().saturating_sub(start).min(request.max_bytes);
        let fragment = &bytes[start..start + available];
        let current_index = position.index;
        let fit = |fragment_length: usize,
                   encoding: FragmentEncoding|
         -> Result<bool, DeveloperLoopError> {
            let end = position
                .offset
                .checked_add(
                    u64::try_from(fragment_length).map_err(|_| error("entry fragment overflow"))?,
                )
                .ok_or_else(|| error("entry fragment overflow"))?;
            let candidate = entry_fragment_candidate(
                memory,
                &response,
                EntryFragmentSpec {
                    mode,
                    binding,
                    count,
                    index: current_index,
                    start: position.offset,
                    entry_end: length,
                },
                end,
                encoding,
                &fragment[..fragment_length],
            )?;
            Ok(encoded_len(&candidate)? <= request.max_bytes)
        };
        let Some((fragment_length, encoding)) = largest_fragment(fragment, fit)? else {
            if response["entries"].as_array().is_some_and(Vec::is_empty) {
                return Ok(rejected(
                    memory,
                    "max_bytes cannot fit entry-fragment metadata and one byte; increase it",
                ));
            }
            response["cursor"] = Value::from(page_cursor(memory, mode, binding, position));
            return Ok(response);
        };
        if position.offset < length && fragment_length == 0 {
            if response["entries"].as_array().is_some_and(Vec::is_empty) {
                return Ok(rejected(
                    memory,
                    "max_bytes cannot fit entry-fragment metadata and one byte; increase it",
                ));
            }
            response["cursor"] = Value::from(page_cursor(memory, mode, binding, position));
            return Ok(response);
        }
        let end = position
            .offset
            .checked_add(
                u64::try_from(fragment_length).map_err(|_| error("entry fragment overflow"))?,
            )
            .ok_or_else(|| error("entry fragment overflow"))?;
        return entry_fragment_candidate(
            memory,
            &response,
            EntryFragmentSpec {
                mode,
                binding,
                count,
                index: current_index,
                start: position.offset,
                entry_end: length,
            },
            end,
            encoding,
            &fragment[..fragment_length],
        );
    }
    response["cursor"] = Value::Null;
    Ok(response)
}

fn entry_fragment_candidate(
    memory: &LocalMemory,
    response: &Value,
    spec: EntryFragmentSpec,
    end: u64,
    encoding: FragmentEncoding,
    bytes: &[u8],
) -> Result<Value, DeveloperLoopError> {
    let next = if end < spec.entry_end {
        Some(Position { index: spec.index, offset: end })
    } else if spec.index.checked_add(1).is_some_and(|next| next < spec.count) {
        Some(Position { index: spec.index + 1, offset: 0 })
    } else {
        None
    };
    let fragment = Value::from_iter([
        ("index", Value::from(spec.index)),
        ("offset", Value::from(spec.start)),
        ("end_offset", Value::from(end)),
        (
            "next_offset",
            if end < spec.entry_end { Value::from(end) } else { Value::Null },
        ),
        (
            "encoding",
            Value::from(match encoding {
                FragmentEncoding::Utf8 => "utf8",
                FragmentEncoding::Hex => "hex",
            }),
        ),
        ("data", fragment_value(bytes, encoding)?),
    ]);
    let mut candidate = response.clone();
    candidate["entry_fragment"] = fragment;
    candidate["cursor"] = next
        .map(|position| Value::from(page_cursor(memory, spec.mode, spec.binding, position)))
        .unwrap_or(Value::Null);
    Ok(candidate)
}

struct ArtifactSearcher<'a> {
    reader: ArtifactReadHandle,
    query: &'a [u8],
    total: u64,
    buffer: Vec<u8>,
    buffer_start: u64,
    search_offset: u64,
    next_read: u64,
}

impl<'a> ArtifactSearcher<'a> {
    fn open(
        memory: &LocalMemory,
        source: &ArchivedObservation,
        query: &'a [u8],
        offset: u64,
    ) -> Result<Self, DeveloperLoopError> {
        check_cancelled(memory)?;
        let reader = memory.store.open_artifact(source.artifact)?;
        check_cancelled(memory)?;
        Ok(Self {
            reader,
            query,
            total: source.artifact.bytes,
            buffer: Vec::new(),
            buffer_start: offset,
            search_offset: offset,
            next_read: offset,
        })
    }

    fn reader_mut(&mut self) -> &mut ArtifactReadHandle {
        &mut self.reader
    }

    fn next_match(&mut self, memory: &LocalMemory) -> Result<Option<u64>, DeveloperLoopError> {
        let query_length =
            u64::try_from(self.query.len()).map_err(|_| error("query size overflow"))?;
        loop {
            check_cancelled(memory)?;
            let buffer_end = self
                .buffer_start
                .checked_add(
                    u64::try_from(self.buffer.len())
                        .map_err(|_| error("search buffer overflow"))?,
                )
                .ok_or_else(|| error("search buffer overflow"))?;
            if self.search_offset >= self.buffer_start
                && self
                    .search_offset
                    .checked_add(query_length)
                    .is_some_and(|end| end <= buffer_end)
            {
                let relative = usize::try_from(self.search_offset - self.buffer_start)
                    .map_err(|_| error("search offset overflow"))?;
                if let Some(found) = self.buffer[relative..]
                    .windows(self.query.len())
                    .position(|window| window == self.query)
                {
                    let hit = self
                        .search_offset
                        .checked_add(
                            u64::try_from(found).map_err(|_| error("match offset overflow"))?,
                        )
                        .ok_or_else(|| error("match offset overflow"))?;
                    if hit.checked_add(query_length).is_some_and(|end| end <= buffer_end) {
                        self.search_offset = hit
                            .checked_add(1)
                            .ok_or_else(|| error("search cursor overflow"))?;
                        return Ok(Some(hit));
                    }
                }
            }
            if self.next_read >= self.total {
                return Ok(None);
            }
            let overlap = query_length.saturating_sub(1);
            let retain_from = buffer_end.saturating_sub(overlap).max(self.search_offset);
            if retain_from > self.buffer_start {
                let drain = usize::try_from(retain_from - self.buffer_start)
                    .map_err(|_| error("search buffer offset overflow"))?;
                if drain > self.buffer.len() {
                    return Err(error("search buffer cursor exceeded retained bytes"));
                }
                self.buffer.drain(..drain);
                self.buffer_start = retain_from;
                self.search_offset = retain_from;
            }
            let remaining = self.total - self.next_read;
            let count = usize::try_from(remaining.min(STREAM_CHUNK_BYTES_U64))
                .map_err(|_| error("search chunk size overflow"))?;
            let chunk = self
                .reader
                .read_chunk_at(self.next_read, count)
                .map_err(|_| error("scan verified artifact bytes"))?
                .ok_or_else(|| error("verified artifact ended during search"))?;
            if chunk.offset() != self.next_read {
                return Err(error("artifact search returned a discontinuous range"));
            }
            self.buffer.extend_from_slice(chunk.bytes());
            self.next_read = self
                .next_read
                .checked_add(
                    u64::try_from(chunk.bytes().len())
                        .map_err(|_| error("search offset overflow"))?,
                )
                .ok_or_else(|| error("search offset overflow"))?;
        }
    }
}

fn largest_fragment(
    bytes: &[u8],
    mut fits: impl FnMut(usize, FragmentEncoding) -> Result<bool, DeveloperLoopError>,
) -> Result<Option<(usize, FragmentEncoding)>, DeveloperLoopError> {
    let hex_length = largest_fitting_index(bytes.len(), |length| {
        fits(length, FragmentEncoding::Hex)
    })?;
    let valid_utf8 = match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(reason) => reason.valid_up_to(),
    };
    let text = std::str::from_utf8(&bytes[..valid_utf8])
        .map_err(|_| error("validate UTF-8 fragment boundary"))?;
    let mut boundaries = text.char_indices().map(|(index, _)| index).collect::<Vec<_>>();
    if boundaries.last().copied() != Some(valid_utf8) {
        boundaries.push(valid_utf8);
    }
    let utf8_boundary = largest_fitting_boundary(&boundaries, |length| {
        fits(length, FragmentEncoding::Utf8)
    })?;
    Ok(match (hex_length, utf8_boundary) {
        (None, None) => None,
        (Some(length), None) => Some((length, FragmentEncoding::Hex)),
        (None, Some(length)) => Some((length, FragmentEncoding::Utf8)),
        (Some(hex_length), Some(utf8_length)) if hex_length > utf8_length => {
            Some((hex_length, FragmentEncoding::Hex))
        }
        (Some(_), Some(utf8_length)) => Some((utf8_length, FragmentEncoding::Utf8)),
    })
}

fn largest_fitting_index(
    maximum: usize,
    mut fits: impl FnMut(usize) -> Result<bool, DeveloperLoopError>,
) -> Result<Option<usize>, DeveloperLoopError> {
    if maximum != 0 && fits(maximum)? {
        return Ok(Some(maximum));
    }
    if !fits(0)? {
        return Ok(None);
    }
    let mut low = 0;
    let mut high = maximum.saturating_sub(1);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if fits(middle)? {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    Ok(Some(low))
}

fn largest_fitting_boundary(
    boundaries: &[usize],
    mut fits: impl FnMut(usize) -> Result<bool, DeveloperLoopError>,
) -> Result<Option<usize>, DeveloperLoopError> {
    let Some(&last) = boundaries.last() else {
        return Ok(None);
    };
    if boundaries.len() > 1 && fits(last)? {
        return Ok(Some(last));
    }
    if !fits(boundaries[0])? {
        return Ok(None);
    }
    let mut low = 0;
    let mut high = boundaries.len().saturating_sub(2);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if fits(boundaries[middle])? {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    Ok(Some(boundaries[low]))
}

fn fragment_value(bytes: &[u8], encoding: FragmentEncoding) -> Result<Value, DeveloperLoopError> {
    match encoding {
        FragmentEncoding::Utf8 => std::str::from_utf8(bytes)
            .map(Value::from)
            .map_err(|_| error("invalid UTF-8 fragment")),
        FragmentEncoding::Hex => Ok(Value::from(hex(bytes))),
    }
}

fn source_response(memory: &LocalMemory) -> Value {
    Value::from_iter([
        ("base_revision", Value::from(memory.model_revision)),
        ("authority", Value::from("none")),
        ("sources", Value::Array(vec![])),
        ("cursor", Value::Null),
        ("next_handle_index", Value::Null),
    ])
}

fn base_fits(
    memory: &LocalMemory,
    maximum: usize,
    response: &Value,
) -> Result<bool, DeveloperLoopError> {
    check_cancelled(memory)?;
    Ok(encoded_len(response)? <= maximum)
}

fn encoded_len(value: &Value) -> Result<usize, DeveloperLoopError> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|_| error("encode context read response"))
}

fn response_sources_empty(response: &Value) -> bool {
    response["sources"].as_array().is_some_and(Vec::is_empty)
}

fn excluded_from_search(source: &ArchivedObservation) -> bool {
    source.kind == ArchiveKind::ToolMessage
        || source.call.as_ref().is_some_and(|call| {
            matches!(call.name.as_str(), "context_read" | "context_update")
        })
}

fn validate_position(
    memory: &LocalMemory,
    request: &Read,
    ids: &[u64],
    entry_ids: &[ContextNodeId],
    mode: &str,
    position: Position,
) -> Result<(), DeveloperLoopError> {
    match mode {
        "e" | "i" => {
            let entries = memory
                .state
                .active_entries(memory.state.binding())
                .map_err(|_| error("entry cursor scope"))?;
            let count = u64::try_from(if mode == "i" { entry_ids.len() } else { entries.len() })
                .map_err(|_| error("entry count overflow"))?;
            if mode == "i" {
                for id in entry_ids {
                    memory.state
                        .entry(memory.state.binding(), *id)
                        .map_err(|_| error("selected working entry is unavailable"))?;
                }
            }
            if position.index > count || (position.index == count && position.offset != 0) {
                return Err(error("entry cursor out of range"));
            }
        }
        "s" => {
            let count =
                u64::try_from(memory.sources.len()).map_err(|_| error("source count overflow"))?;
            if position.index > count || (position.index == count && position.offset != 0) {
                return Err(error("search cursor out of range"));
            }
            if position.index < count {
                let index = usize::try_from(position.index)
                    .map_err(|_| error("source index overflow"))?;
                if position.offset > memory.sources[index].artifact.bytes {
                    return Err(error("search byte cursor out of range"));
                }
            }
        }
        "h" => {
            let count = u64::try_from(ids.len()).map_err(|_| error("handle count overflow"))?;
            for &id in ids {
                let source = memory.archived(id)?;
                let end = request.end_offset.unwrap_or(source.artifact.bytes);
                if request.offset > end || end > source.artifact.bytes {
                    return Err(error("requested byte range is outside an artifact"));
                }
            }
            if position.index > count || (position.index == count && position.offset != 0) {
                return Err(error("handle cursor out of range"));
            }
            if position.index < count {
                let index = usize::try_from(position.index)
                    .map_err(|_| error("handle index overflow"))?;
                let source = memory.archived(ids[index])?;
                let end = request.end_offset.unwrap_or(source.artifact.bytes);
                if position.offset < request.offset || position.offset > end {
                    return Err(error("handle byte cursor out of range"));
                }
            }
        }
        _ => return Err(error("invalid context read mode")),
    }
    Ok(())
}

fn request_binding(
    memory: &LocalMemory,
    mode: &str,
    ids: &[u64],
    entry_ids: &[ContextNodeId],
    request: &Read,
) -> Result<[u8; 32], DeveloperLoopError> {
    let mut value = Value::from_iter([
        ("mode", Value::from(mode)),
        (
            "revision",
            Value::from(matches!(mode, "e" | "i").then_some(memory.model_revision)),
        ),
        (
            "handles",
            Value::Array(ids.iter().copied().map(Value::from).collect()),
        ),
        (
            "query",
            request.query.as_ref().map_or(Value::Null, |query| Value::from(query.as_str())),
        ),
        ("offset", Value::from(request.offset)),
        ("end_offset", request.end_offset.map_or(Value::Null, Value::from)),
    ]);
    if mode == "i" {
        value["entries"] = Value::Array(
            entry_ids.iter()
                .map(|id| Value::from(format!("entry:{}", hex(id.as_bytes()))))
                .collect(),
        );
    }
    let bytes = serde_json::to_vec(&value).map_err(|_| error("encode cursor binding"))?;
    Ok(*sha256(&bytes).as_bytes())
}

fn page_cursor(memory: &LocalMemory, mode: &str, binding: [u8; 32], position: Position) -> String {
    let token = cursor_token(memory, mode, binding, position);
    format!("v2:{mode}:{}:{}:{token}", position.index, position.offset)
}

fn cursor_token(
    memory: &LocalMemory,
    mode: &str,
    binding: [u8; 32],
    position: Position,
) -> String {
    let material = format!(
        "context-read/v2|{}|{mode}|{}|{}|{}",
        hex(memory.store.scope_digest().as_bytes()),
        position.index,
        position.offset,
        hex(&binding),
    );
    hex(sha256(material.as_bytes()).as_bytes())
}

fn parse_cursor(
    memory: &LocalMemory,
    value: Option<&str>,
    mode: &str,
    binding: [u8; 32],
    initial: Position,
) -> Result<Position, DeveloperLoopError> {
    let Some(value) = value else {
        return Ok(initial);
    };
    let mut parts = value.split(':');
    if parts.next() != Some("v2") || parts.next() != Some(mode) {
        return Err(error("cursor scope or mode mismatch"));
    }
    let index = parts
        .next()
        .ok_or_else(|| error("invalid cursor"))?
        .parse::<u64>()
        .map_err(|_| error("invalid cursor index"))?;
    let offset = parts
        .next()
        .ok_or_else(|| error("invalid cursor"))?
        .parse::<u64>()
        .map_err(|_| error("invalid cursor offset"))?;
    let token = parts.next().ok_or_else(|| error("invalid cursor"))?;
    if parts.next().is_some() {
        return Err(error("invalid cursor"));
    }
    let position = Position { index, offset };
    if token != cursor_token(memory, mode, binding, position) {
        return Err(error("cursor scope or request mismatch"));
    }
    Ok(position)
}

fn check_cancelled(memory: &LocalMemory) -> Result<(), DeveloperLoopError> {
    if memory.cancellation.is_cancelled() {
        Err(error("context read cancelled"))
    } else {
        Ok(())
    }
}
