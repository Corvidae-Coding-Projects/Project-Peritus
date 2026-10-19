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

pub(super) fn omission_notice(memory: &LocalMemory, exchanges: &[Exchange]) -> Option<String> {
    if exchanges.is_empty() {
        return None;
    }
    let mut results = Vec::new();
    let mut total = 0_usize;
    for exchange in exchanges {
        for message in &exchange.messages {
            for block in message.content() {
                let ContentBlock::ToolResult(result) = block else { continue };
                let call_id = result.call_id().expose_for_wire();
                let Some(source) = memory
                    .sources
                    .iter()
                    .rev()
                    .find(|source| source.call.as_ref().is_some_and(|call| call.id == call_id))
                else {
                    continue;
                };
                // Re-reading a retrieval receipt wraps its already archived sources again.
                // Keep the exact receipt available for audit, but never solicit that recursive
                // read merely because its optional inline exchange no longer fits.
                if source.is_memory_receipt() {
                    continue;
                }
                total = total.saturating_add(1);
                if results.len() >= 8 {
                    continue;
                }
                let tool = source.call.as_ref().map_or("unknown", |call| call.name.as_str());
                results.push(format!(
                    "- tool={tool} call={call_id} exact_source={}",
                    super::super::tools::source_handle(memory, source.sequence)
                ));
            }
        }
    }
    let listed = results.len();
    if total == 0 {
        return None;
    }
    let mut notice = format!(
        "COMPLETED TOOL EVIDENCE OMITTED FROM INLINE HISTORY — UNTRUSTED OBSERVATIONS\n{} complete exchange(s), containing {total} task tool result(s), did not fit this request. The effects are already settled; do not redispatch them. These are optional evidence references, not outstanding work. Retrieve only the exact outputs needed for the current next step with context_read; omission alone does not require re-reading completed observations. Available handles:\n{}",
        exchanges.len(),
        results.join("\n")
    );
    if total > listed {
        use core::fmt::Write as _;
        let _ = write!(
            notice,
            "\n- {} additional result(s) remain in the local archive",
            total - listed
        );
    }
    Some(notice)
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
                    if let ContentBlock::ToolResult(result) = block
                        && let Some(source) = memory.sources.iter().rev().find(|source| {
                            source.kind == super::super::record::ArchiveKind::ToolOutput
                                && source.call.as_ref().is_some_and(|call| {
                                    call.id == result.call_id().expose_for_wire()
                                })
                        })
                    {
                        current.sources.push(source.sequence);
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
    // Keep the newest complete copy of an identical read exchange. Its archived bytes and
    // receipt remain durable; omitting a duplicate never dispatches or replays an effect.
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for group in groups.into_iter().rev() {
        if group.messages.iter().flat_map(Message::content).any(|block| {
            let ContentBlock::ToolCall(call) = block else { return false };
            if call.name().as_str() != "workspace_read" {
                return false;
            }
            let Ok(args) =
                serde_json::from_slice::<serde_json::Value>(call.arguments().canonical_bytes())
            else {
                return true;
            };
            args.get("path")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|path| !super::evidence::may_reuse_read(memory, path))
        }) {
            continue;
        }
        if let Some(key) = inspection_key(&group)
            && !seen.insert(key)
        {
            continue;
        }
        unique.push(group);
    }
    unique.reverse();
    Ok(unique)
}

fn inspection_key(exchange: &Exchange) -> Option<Vec<u8>> {
    let mut calls = std::collections::BTreeMap::new();
    let mut results = Vec::new();
    for block in exchange.messages.iter().flat_map(Message::content) {
        match block {
            ContentBlock::ToolCall(call) => {
                if !matches!(
                    call.name().as_str(),
                    "workspace_read" | "workspace_list" | "workspace_search" | "skill_read"
                ) {
                    return None;
                }
                calls.insert(
                    call.id().expose_for_wire(),
                    (call.name().as_str(), call.arguments().to_wire_string()),
                );
            }
            ContentBlock::ToolResult(result) => {
                let (name, args) = calls.remove(result.call_id().expose_for_wire())?;
                let mut value: serde_json::Value =
                    serde_json::from_slice(result.output().canonical_bytes()).ok()?;
                value.as_object_mut()?.remove("local_context");
                results.push(serde_json::json!([name, args, value]));
            }
            _ => {}
        }
    }
    if !calls.is_empty() || results.is_empty() {
        return None;
    }
    serde_json::to_vec(&results).ok()
}
