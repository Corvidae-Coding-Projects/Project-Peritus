//! Host-observed immutable evidence ranges survive omission of their retrieval receipts.

use super::super::{memory::LocalMemory, record::DeliveredEvidence};
use peritus_agent::DeveloperLoopError;
use serde::Deserialize;
use std::{collections::BTreeMap, fmt::Write as _};

#[derive(Deserialize)]
struct Page {
    section: String,
    sha256: String,
    total_bytes: u64,
    offset: u64,
    next_offset: Option<u64>,
    bytes: String,
}

pub(super) fn coverage(memory: &LocalMemory) -> Result<String, DeveloperLoopError> {
    let mut sections = BTreeMap::<(String, String, u64), Vec<(u64, u64)>>::new();
    for page in &memory.transcript.delivered_evidence {
        sections
            .entry((page.section.clone(), page.sha256.clone(), page.total_bytes))
            .or_default()
            .push((page.start, page.end));
    }
    if sections.is_empty() {
        return Ok(String::new());
    }
    let mut output = String::from(
        "\nHistorical review evidence already delivered in this logical role task (host receipt ranges only, not correctness or acceptance):\n",
    );
    for ((section, digest, total), mut ranges) in sections {
        ranges.sort_unstable();
        let mut merged = Vec::<(u64, u64)>::new();
        for (start, end) in ranges {
            if let Some(last) = merged.last_mut()
                && start <= last.1
            {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        let complete = merged.as_slice() == [(0, total)];
        let _ = writeln!(
            output,
            "- section={section} total_bytes={total} sha256={digest} delivered_ranges={merged:?} complete={complete}"
        );
    }
    output.push_str("These facts record delivery of the immutable source bytes, not a review verdict. Matching initial source markers remain in the pinned prompt after retrieval; do not reread delivered ranges solely because those markers remain or their receipt exchanges were omitted. Retrieve a relevant range again when you need its content, and independently inspect current workspace state.\n");
    Ok(output)
}

/// Record actual submitted pages, not oversized raw results or undelivered archive entries.
pub(in crate::local_context) fn observe_view(
    memory: &mut LocalMemory,
    messages: &[peritus_model_protocol::Message],
) -> Result<(), DeveloperLoopError> {
    use peritus_model_protocol::ContentBlock;
    let mut calls = BTreeMap::new();
    let mut ranges = memory.transcript.delivered_evidence.clone();
    for block in messages.iter().flat_map(peritus_model_protocol::Message::content) {
        match block {
            ContentBlock::ToolCall(call) => {
                calls.insert(call.id().expose_for_wire(), call.name().as_str());
            }
            ContentBlock::ToolResult(result) if !result.is_error() => {
                if calls.remove(result.call_id().expose_for_wire())
                    != Some("developer_evidence_read")
                {
                    continue;
                }
                let Ok(page) = serde_json::from_slice::<Page>(result.output().canonical_bytes())
                else {
                    continue;
                };
                let Some(end) = page.offset.checked_add(page.bytes.len() as u64) else {
                    continue;
                };
                if end > page.total_bytes
                    || page.next_offset != (end < page.total_bytes).then_some(end)
                    || page.sha256.len() != 64
                {
                    continue;
                }
                ranges.push(DeliveredEvidence {
                    section: page.section,
                    sha256: page.sha256,
                    total_bytes: page.total_bytes,
                    start: page.offset,
                    end,
                });
            }
            _ => {}
        }
    }
    ranges.sort();
    let mut merged: Vec<DeliveredEvidence> = Vec::new();
    for page in ranges {
        if let Some(last) = merged.last_mut()
            && last.section == page.section
            && last.sha256 == page.sha256
            && last.total_bytes == page.total_bytes
            && page.start <= last.end
        {
            last.end = last.end.max(page.end);
        } else {
            merged.push(page);
        }
    }
    if merged != memory.transcript.delivered_evidence {
        memory.transcript.delivered_evidence = merged;
        memory.persist_transcript()?;
    }
    Ok(())
}
