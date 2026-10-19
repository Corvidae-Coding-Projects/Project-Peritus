//! Bounded non-authoritative older evidence, never raw cross-invocation tool replay.

use super::super::record::ArchiveKind;
use super::{LocalMemory, text_message};
use peritus_agent::{DeveloperLoopError, estimate_developer_request_tokens};
use peritus_model_protocol::{
    ContentBlock, Message, ProtocolLimits, Role, ToolDefinition, decode_messages,
};

pub(super) fn append(
    memory: &mut LocalMemory,
    messages: &mut Vec<Message>,
    selected: &mut Vec<u64>,
    tools: &[ToolDefinition],
    target: u64,
) -> Result<(), DeveloperLoopError> {
    index_inspections(memory)?;
    if memory.conversation.as_ref().is_some_and(|view| {
        !view.effective_permissions().allows(crate::control::PermissionCapability::Read)
    }) {
        return Ok(());
    }
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
    // Preserve useful exact source/skill pages across invocation recovery. Old assistant
    // plans are still retrievable, but are not automatically replayed as new work.
    candidates.extend(
        memory
            .sources
            .iter()
            .rev()
            .filter(|source| {
                source.kind == ArchiveKind::ToolOutput
                    && !source.is_error
                    && source.call.as_ref().is_some_and(|call| {
                        matches!(call.name.as_str(), "workspace_read" | "skill_read")
                    })
            })
            .map(|source| source.sequence),
    );
    candidates.sort_unstable_by_key(|id| {
        let target = memory
            .sources
            .iter()
            .find(|source| source.sequence == *id)
            .and_then(|source| source.call.as_ref())
            .and_then(|call| memory.inspection_targets.get(&call.id));
        let focus = target.is_some_and(|path| memory.task_contract.contains(path));
        let skill = target.is_some_and(|path| path.starts_with("skill:"));
        (std::cmp::Reverse(focus), std::cmp::Reverse(skill), std::cmp::Reverse(*id))
    });
    candidates.dedup();
    let mut bytes_left = memory.config.max_read_bytes;
    let mut seen_pages: std::collections::BTreeSet<_> = messages
        .iter()
        .flat_map(Message::content)
        .filter_map(|block| match block {
            ContentBlock::ToolResult(result) => Some(page_key(result.output().canonical_bytes())),
            _ => None,
        })
        .collect();
    for id in candidates {
        if bytes_left < 256 {
            break;
        }
        if selected.contains(&id) {
            continue;
        }
        let source = memory.archived(id)?;
        if !matches!(source.kind, ArchiveKind::ToolOutput | ArchiveKind::Assistant)
            || source.is_memory_receipt()
        {
            continue;
        }
        let bytes = memory.artifact(id)?;
        if source.call.as_ref().is_some_and(|call| call.name == "workspace_read") {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else { continue };
            if !read_is_current(memory, &value) {
                continue;
            }
        }
        if !seen_pages.insert(page_key(&bytes)) {
            continue;
        }
        let (text, label) = if source.kind == ArchiveKind::Assistant {
            let messages = decode_messages(&bytes, ProtocolLimits::PRODUCTION)?;
            let text = messages
                .iter()
                .filter(|message| message.role() == Role::Assistant)
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
            (text, "UNTRUSTED PRIOR ASSISTANT TEXT; not verified state or current instructions")
        } else {
            (String::from_utf8_lossy(&bytes).into_owned(), "UNTRUSTED TOOL EVIDENCE")
        };
        let mut take = bytes_left.saturating_sub(192).min(text.len());
        let fitted = loop {
            let prefix = &text[..text.floor_char_boundary(take)];
            let body = format!(
                "ARCHIVED OBSERVATION {} — {label}\nvisible preview bytes 0..{} of {}; exact source remains available through context_read.\n{prefix:?}",
                super::super::tools::source_handle(memory, id),
                prefix.len(),
                text.len(),
            );
            if !prefix.is_empty() && body.len() <= bytes_left {
                let message = text_message(Role::User, body.clone())?;
                let mut candidate = messages.clone();
                candidate.push(message.clone());
                if estimate_developer_request_tokens(&candidate, tools) <= target {
                    break Some((body.len(), message));
                }
            }
            if take == 0 {
                break None;
            }
            take /= 2;
        };
        let Some((added, message)) = fitted else { continue };
        messages.push(message);
        selected.push(id);
        bytes_left = bytes_left.saturating_sub(added);
    }
    Ok(())
}

fn page_key(bytes: &[u8]) -> [u8; 32] {
    if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(bytes)
        && let Some(object) = value.as_object_mut()
    {
        object.remove("local_context");
        return peritus_codec::sha256(value.to_string().as_bytes()).into_bytes();
    }
    peritus_codec::sha256(bytes).into_bytes()
}

pub(super) fn may_reuse_read(memory: &LocalMemory, path: &str) -> bool {
    memory.conversation.as_ref().is_some_and(|view| {
        crate::developer_tools::may_reuse_read(
            &memory.workspace,
            path,
            &memory.workspace_scope.protected,
            view.as_ref(),
        )
    })
}

pub(super) fn read_is_current(memory: &LocalMemory, value: &serde_json::Value) -> bool {
    let Some(path) = value.get("path").and_then(serde_json::Value::as_str) else { return false };
    let Some(expected) = value.get("source_sha256").and_then(serde_json::Value::as_str) else {
        return false;
    };
    may_reuse_read(memory, path)
        && crate::developer_tools::checked_protected_file_for_developer(
            &memory.workspace,
            path,
            &memory.task_contract,
            &memory.workspace_scope.protected,
        )
        .is_ok_and(|file| {
            crate::developer_tools::source_digest(&file).ok().as_deref() == Some(expected)
        })
}

fn index_inspections(memory: &mut LocalMemory) -> Result<(), DeveloperLoopError> {
    let sources = memory
        .sources
        .iter()
        .filter(|source| {
            source.kind == ArchiveKind::Assistant
                && source.sequence > memory.inspection_indexed_through
        })
        .map(|source| source.sequence)
        .collect::<Vec<_>>();
    for id in sources {
        for message in decode_messages(&memory.artifact(id)?, ProtocolLimits::PRODUCTION)? {
            for block in message.content() {
                let ContentBlock::ToolCall(call) = block else { continue };
                let Ok(args) =
                    serde_json::from_slice::<serde_json::Value>(call.arguments().canonical_bytes())
                else {
                    continue;
                };
                let target = match call.name().as_str() {
                    "workspace_read" => {
                        args.get("path").and_then(serde_json::Value::as_str).map(str::to_owned)
                    }
                    "skill_read" => args
                        .get("skill")
                        .and_then(serde_json::Value::as_str)
                        .map(|skill| format!("skill:{skill}")),
                    _ => None,
                };
                if let Some(target) = target {
                    memory
                        .inspection_targets
                        .insert(call.id().expose_for_wire().to_owned(), target);
                }
            }
        }
        memory.inspection_indexed_through = id;
    }
    Ok(())
}
