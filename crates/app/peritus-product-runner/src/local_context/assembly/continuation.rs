//! Exact role-owned continuation evidence survives invocation rollover and compaction.
use super::super::{record::ArchiveKind, tools::source_handle};
use super::{LocalMemory, text_message};
use peritus_agent::{DeveloperLoopError, estimate_developer_request_tokens};
use peritus_model_protocol::{
    ContentBlock, Message, ProtocolLimits, Role, ToolDefinition, decode_messages,
};

pub(super) fn append(
    memory: &LocalMemory,
    messages: &mut Vec<Message>,
    selected: &mut Vec<u64>,
    tools: &[ToolDefinition],
    target: u64,
) -> Result<(), DeveloperLoopError> {
    // This is a view window, not a task or execution budget. Exact older text remains archived.
    let mut remaining = memory.config.max_read_bytes.min(8192);
    let mut notes = Vec::new();
    for source in memory
        .sources
        .iter()
        .rev()
        .filter(|source| {
            source.kind == ArchiveKind::ToolOutput
                && !source.is_error
                && source.call.as_ref().is_some_and(|call| {
                    !crate::local_context::activity::inspection(&call.name)
                        && call.name != "context_update"
                })
        })
        .take(8)
    {
        let Some(call) = &source.call else { continue };
        let cached = memory.continuation_actions.borrow().get(&source.sequence).cloned();
        let summary = if let Some(cached) = cached {
            cached
        } else {
            let value: serde_json::Value =
                serde_json::from_slice(&memory.artifact(source.sequence)?)
                    .map_err(|_| super::super::error("decode settled action"))?;
            let fields =
                ["path", "changed", "sha256", "handle", "state", "exit_code", "success", "purpose"]
                    .into_iter()
                    .filter_map(|key| value.get(key).map(|value| (key, value)))
                    .collect::<std::collections::BTreeMap<_, _>>();
            let summary = format!(
                "SETTLED {} {} {} — historical outcome, not work to repeat",
                call.name,
                source_handle(memory, source.sequence),
                serde_json::to_string(&fields)
                    .map_err(|_| super::super::error("encode action receipt"))?
            );
            memory.continuation_actions.borrow_mut().insert(source.sequence, summary.clone());
            summary
        };
        if !selected.contains(&source.sequence) {
            notes.push((source.sequence, summary));
        }
    }
    while memory.continuation_actions.borrow().len() > 8 {
        memory.continuation_actions.borrow_mut().pop_first();
    }
    let actions = notes.len();
    for source in memory.sources.iter().rev() {
        if selected.contains(&source.sequence) || source.kind != ArchiveKind::Assistant {
            continue;
        }
        let decoded =
            decode_messages(&memory.artifact(source.sequence)?, ProtocolLimits::PRODUCTION)?;
        let text = decoded
            .iter()
            .flat_map(Message::content)
            .filter_map(|block| match block {
                ContentBlock::Text(text) => Some(text.expose_for_wire()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if text.trim().is_empty() {
            continue;
        }
        let take = text.floor_char_boundary(remaining.min(text.len()));
        if take == 0 {
            break;
        }
        let excerpt = if take < text.len() {
            let head = text.floor_char_boundary(take / 3);
            let tail =
                text.ceil_char_boundary(text.len().saturating_sub(take.saturating_sub(head)));
            format!(
                "{}\n[prior prose omitted; exact source remains archived]\n{}",
                &text[..head],
                &text[tail..]
            )
        } else {
            text.clone()
        };
        notes.push((
            source.sequence,
            format!("{}: {}", source_handle(memory, source.sequence), excerpt),
        ));
        remaining = remaining.saturating_sub(take);
        if notes.len() >= actions + 3 || remaining < 256 {
            break;
        }
    }
    if notes.is_empty() {
        return Ok(());
    }
    notes.reverse();
    let body = format!(
        "ROLE CONTINUATION — prior assistant observations, not user instructions, verified state, or a review verdict. Current user steering and current host policy govern. Continue the unfinished step using retained evidence; do not repeat discovery solely because the invocation changed or history was compacted. Settled effects must not be redispatched. Retrieve a cited source only when its missing content is needed.\n{}",
        notes.iter().map(|(_, text)| text.as_str()).collect::<Vec<_>>().join("\n")
    );
    let message = text_message(Role::User, body)?;
    let mut candidate = messages.clone();
    candidate.push(message.clone());
    if estimate_developer_request_tokens(&candidate, tools) <= target {
        messages.push(message);
        selected.extend(notes.into_iter().map(|(id, _)| id));
    }
    Ok(())
}
