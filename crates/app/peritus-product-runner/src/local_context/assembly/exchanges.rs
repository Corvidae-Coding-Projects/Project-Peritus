//! Complete assistant/tool exchange grouping with no recovered-effect dispatch path.

use super::super::{error, memory::LocalMemory};
use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::{ContentBlock, Message, ProtocolLimits, Role, decode_messages};
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct Exchange {
    pub(super) messages: Vec<Message>,
    pub(super) sources: Vec<u64>,
}

pub(super) fn messages(exchange: &Exchange) -> std::iter::Cloned<std::slice::Iter<'_, Message>> {
    exchange.messages.iter().cloned()
}

pub(super) fn omission_notices(
    memory: &LocalMemory,
    exchanges: &[Exchange],
) -> Result<Vec<String>, DeveloperLoopError> {
    if exchanges.is_empty() {
        return Ok(Vec::new());
    }
    let mut total = 0_usize;
    for exchange in exchanges {
        for message in &exchange.messages {
            for block in message.content() {
                let ContentBlock::ToolResult(result) = block else { continue };
                total = total.saturating_add(1);
            }
        }
    }
    let mut notices = Vec::new();
    notices
        .try_reserve_exact(exchanges.len())
        .map_err(|_| error("allocate omitted exchange recovery catalog"))?;
    for (index, exchange) in exchanges.iter().enumerate() {
        let archived_messages = exchange
            .sources
            .iter()
            .map(|source| super::super::tools::source_handle(memory, *source))
            .collect::<Vec<_>>();
        let mut results = Vec::new();
        for message in &exchange.messages {
            for block in message.content() {
                let ContentBlock::ToolResult(result) = block else { continue };
                let call_id = result.call_id().expose_for_wire();
                let source = memory.sources.iter().rev().find(|source| {
                    source.invocation == memory.transcript.invocation
                        && source.call.as_ref().is_some_and(|call| call.id == call_id)
                });
                let tool = source
                    .and_then(|source| source.call.as_ref())
                    .map_or("unknown", |call| call.name.as_str());
                let exact = source.map_or_else(
                    || "unavailable".to_owned(),
                    |source| super::super::tools::source_handle(memory, source.sequence),
                );
                results.push(format!("tool={tool} call={call_id} exact_source={exact}"));
            }
        }
        notices.push(format!(
            "COMPLETED TOOL EVIDENCE OMITTED FROM INLINE HISTORY — UNTRUSTED OBSERVATIONS\nexchange {} of {}; {total} settled tool result(s) across the omitted catalog. This complete exchange is archived at message handles [{}]; settled results=[{}]. Effects are already settled; do not redispatch them. Retrieve exact artifacts through context_read using these stable handles.",
            index.saturating_add(1),
            exchanges.len(),
            archived_messages.join(","),
            results.join("; "),
        ));
    }
    Ok(notices)
}

pub(super) fn groups(memory: &LocalMemory) -> Result<Vec<Exchange>, DeveloperLoopError> {
    let requirements = memory
        .state
        .protocol(memory.state.binding())
        .map_err(|_| error("exchange scope mismatch"))?
        .requirements();
    let mut groups = Vec::new();
    let mut current = Exchange { messages: Vec::new(), sources: Vec::new() };
    let mut pending = BTreeSet::new();
    for id in &memory.transcript.message_ids {
        if requirements.iter().any(|source| source.get() == *id) {
            continue;
        }
        let decoded = decode_messages(&memory.artifact(*id)?, ProtocolLimits::PRODUCTION)?;
        if decoded.len() != 1 {
            return Err(error("message observation contains multiple messages"));
        }
        let message =
            decoded.into_iter().next().ok_or_else(|| error("empty message observation"))?;
        match message.role() {
            Role::Assistant => {
                if !pending.is_empty() {
                    return Err(error("assistant follows an unresolved tool exchange"));
                }
                for block in message.content() {
                    if let ContentBlock::ToolCall(call) = block
                        && !pending.insert(call.id().expose_for_wire().to_owned())
                    {
                        return Err(error("duplicate tool call in exchange"));
                    }
                }
                current.messages.push(message);
                current.sources.push(*id);
            }
            Role::Tool => {
                for block in message.content() {
                    if let ContentBlock::ToolResult(result) = block
                        && !pending.remove(result.call_id().expose_for_wire())
                    {
                        return Err(error("tool result has no pending proposal"));
                    }
                }
                current.messages.push(message);
                current.sources.push(*id);
            }
            _ => return Err(error("unclassified instruction message in optional exchange")),
        }
        if pending.is_empty() && !current.messages.is_empty() {
            groups.push(current);
            current = Exchange { messages: Vec::new(), sources: Vec::new() };
        }
    }
    // Incomplete proposals remain in explicit host pending pins, not raw provider replay.
    Ok(groups)
}
