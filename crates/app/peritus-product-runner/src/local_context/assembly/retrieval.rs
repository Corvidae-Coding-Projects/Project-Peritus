//! Host-observed immutable evidence ranges survive omission of their retrieval receipts.

use super::super::{memory::LocalMemory, record::ArchiveKind};
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
    for source in memory.sources.iter().filter(|source| {
        source.invocation == memory.transcript.invocation
            && source.kind == ArchiveKind::ToolOutput
            && !source.is_error
            && source.call.as_ref().is_some_and(|call| call.name == "developer_evidence_read")
    }) {
        let Ok(page) = serde_json::from_slice::<Page>(&memory.artifact(source.sequence)?) else {
            continue;
        };
        if !matches!(
            page.section.as_str(),
            "transcript" | "diff" | "gates" | "developer_commands" | "prior" | "correction"
        ) || page.sha256.len() != 64
            || !page
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            continue;
        }
        let Some(end) = page.offset.checked_add(page.bytes.len() as u64) else { continue };
        if end > page.total_bytes || page.next_offset != (end < page.total_bytes).then_some(end) {
            continue;
        }
        sections
            .entry((page.section, page.sha256, page.total_bytes))
            .or_default()
            .push((page.offset, end));
    }
    if sections.is_empty() {
        return Ok(String::new());
    }
    let mut output = String::from(
        "\nHistorical review evidence already delivered in this Peritus invocation (host receipt ranges only, not correctness or acceptance):\n",
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
