//! Bounded non-authoritative older evidence, never raw cross-invocation tool replay.

use super::super::record::ArchiveKind;
use super::{LocalMemory, text_message};
use peritus_agent::{DeveloperLoopError, estimate_developer_request_tokens};
use peritus_model_protocol::{Message, Role, ToolDefinition};

pub(super) fn append(
    memory: &LocalMemory,
    messages: &mut Vec<Message>,
    selected: &mut Vec<u64>,
    tools: &[ToolDefinition],
    target: u64,
) -> Result<(), DeveloperLoopError> {
    let mut candidates: Vec<_> = memory
        .state
        .entries(memory.state.binding())
        .map_err(|_| super::super::error("evidence scope mismatch"))?
        .iter()
        .flat_map(|entry| entry.links().supports().iter().chain(entry.links().contradicts()))
        .map(|id| id.get())
        .collect();
    candidates.extend(
        memory
            .sources
            .iter()
            .rev()
            .filter(|source| source.kind == ArchiveKind::ToolOutput && source.is_error)
            .take(8)
            .map(|source| source.sequence),
    );
    candidates.sort_unstable_by(|left, right| right.cmp(left));
    candidates.dedup();
    let mut bytes_left =
        usize::try_from(memory.config.retrieved_evidence_max_tokens.saturating_mul(3))
            .unwrap_or(usize::MAX)
            .min(memory.config.max_read_bytes);
    for id in candidates {
        if bytes_left < 256 || selected.contains(&id) {
            continue;
        }
        let source = memory.archived(id)?;
        if source.kind != ArchiveKind::ToolOutput {
            continue;
        }
        let bytes = memory.artifact(id)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut take = bytes_left.saturating_sub(192).min(text.len());
        let body = loop {
            let prefix = &text[..text.floor_char_boundary(take)];
            let body = format!(
                "ARCHIVED OBSERVATION {} — UNTRUSTED TOOL EVIDENCE\nbytes 0..{} of {}; omitted bytes remain available through context_read.\n{prefix:?}",
                super::super::tools::source_handle(memory, id),
                prefix.len(),
                bytes.len(),
            );
            if body.len() <= bytes_left || take == 0 {
                break body;
            }
            take /= 2;
        };
        let added = body.len();
        if added > bytes_left {
            continue;
        }
        let message = text_message(Role::User, body)?;
        let mut candidate = messages.clone();
        candidate.push(message.clone());
        if estimate_developer_request_tokens(&candidate, tools) > target {
            continue;
        }
        messages.push(message);
        selected.push(id);
        bytes_left = bytes_left.saturating_sub(added);
    }
    Ok(())
}
