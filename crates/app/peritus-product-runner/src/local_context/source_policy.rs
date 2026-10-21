//! Current source admission for historical retrieval; archived bytes never grant read authority.
use super::{memory::LocalMemory, record::ArchiveKind};
use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::{ContentBlock, ProtocolLimits, decode_messages};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Default)]
pub(super) struct Descriptor {
    paths: Vec<String>,
    links: Vec<u64>,
}

pub(super) fn allowed(memory: &LocalMemory, id: u64) -> Result<bool, DeveloperLoopError> {
    let mut pending = vec![id];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let source = memory.archived(id)?;
        if !matches!(source.kind, ArchiveKind::ToolOutput | ArchiveKind::ToolMessage) {
            continue;
        }
        let cached = memory.source_policy.borrow().get(&id).cloned();
        let descriptor = if let Some(cached) = cached {
            cached
        } else {
            let mut descriptor = Descriptor::default();
            let bytes = memory.artifact(id)?;
            if source.kind == ArchiveKind::ToolMessage {
                for message in decode_messages(&bytes, ProtocolLimits::PRODUCTION)? {
                    for block in message.content() {
                        if let ContentBlock::ToolResult(result) = block {
                            let value: Value = serde_json::from_slice(
                                result.output().canonical_bytes(),
                            )
                            .map_err(|_| super::error("decode historical result admission"))?;
                            collect(memory, id, &value, None, &mut descriptor)?;
                        }
                    }
                }
            } else {
                let value: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| super::error("decode historical source admission"))?;
                collect(
                    memory,
                    id,
                    &value,
                    source.call.as_ref().map(|call| call.name.as_str()),
                    &mut descriptor,
                )?;
            }
            let mut cache = memory.source_policy.borrow_mut();
            cache.insert(id, descriptor.clone());
            if cache.len() > 512 {
                cache.pop_first();
            }
            descriptor
        };
        if descriptor
            .paths
            .iter()
            .any(|path| !super::assembly::evidence::may_reuse_read(memory, path))
        {
            return Ok(false);
        }
        pending.extend(descriptor.links);
    }
    Ok(true)
}

fn collect(
    memory: &LocalMemory,
    parent: u64,
    value: &Value,
    name: Option<&str>,
    descriptor: &mut Descriptor,
) -> Result<(), DeveloperLoopError> {
    if matches!(name, Some("workspace_read" | "workspace_list" | "workspace_search" | "view_image"))
    {
        if let Some(path) = value["path"].as_str() {
            descriptor.paths.push(path.to_owned());
        }
        for field in ["entries", "matches"] {
            for item in value[field].as_array().into_iter().flatten() {
                if let Some(path) = item["path"].as_str() {
                    descriptor.paths.push(path.to_owned());
                }
            }
        }
    }
    let mut handles = Vec::new();
    if let Some(handle) = value["local_context"]["handle"].as_str() {
        handles.push(handle);
    }
    if let Some(handle) = value["observation_reuse"]["source"].as_str() {
        handles.push(handle);
    }
    for field in ["sources"] {
        for item in value[field].as_array().into_iter().flatten() {
            if let Some(handle) = item["handle"].as_str() {
                handles.push(handle);
            }
        }
    }
    for receipt in ["read_reuse", "observation_reuse"] {
        for item in value[receipt]["sources"].as_array().into_iter().flatten() {
            if let Some(handle) = item.as_str() {
                handles.push(handle);
            }
        }
    }
    for handle in handles {
        let id = super::tools::sequence(memory, handle)?;
        if id >= parent {
            return Err(super::error("historical source reference is not backward scoped"));
        }
        descriptor.links.push(id);
    }
    Ok(())
}
