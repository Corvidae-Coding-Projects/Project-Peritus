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

pub(super) fn entry_allowed(
    memory: &LocalMemory,
    entry: &peritus_context::working::WorkingEntry,
) -> Result<bool, DeveloperLoopError> {
    let mut pending = entry
        .links()
        .supports()
        .iter()
        .chain(entry.links().contradicts())
        .map(|id| id.get())
        .collect::<Vec<_>>();
    if let Some(binding) = entry.host_binding() {
        pending.extend(
            memory
                .sources
                .iter()
                .take(usize::try_from(binding.through_observation()).unwrap_or(usize::MAX))
                .map(|source| source.sequence),
        );
    }
    allowed_sources(memory, pending)
}

pub(super) fn allowed(memory: &LocalMemory, id: u64) -> Result<bool, DeveloperLoopError> {
    allowed_sources(memory, vec![id])
}

fn allowed_sources(
    memory: &LocalMemory,
    mut pending: Vec<u64>,
) -> Result<bool, DeveloperLoopError> {
    let mut seen = BTreeSet::new();
    let mut expanded_prefix = 0;
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let source = memory.archived(id)?;
        if source.kind == ArchiveKind::Assistant {
            // Model prose and proposed arguments can quote any earlier supplied input.
            // Expand each prefix at most once; source roles do not certify independence.
            let through = source.sequence.saturating_sub(1);
            if through > expanded_prefix {
                pending.extend(
                    memory
                        .source_prefix_after(expanded_prefix)
                        .take_while(|source| source.sequence <= through)
                        .map(|source| source.sequence),
                );
                expanded_prefix = through;
            }
            continue;
        }
        if !matches!(
            source.kind,
            ArchiveKind::ToolOutput | ArchiveKind::ToolMessage | ArchiveKind::HostObservation
        ) {
            continue;
        }
        let cached = memory.source_policy.borrow().get(&id).cloned();
        let descriptor = if let Some(cached) = cached {
            cached
        } else {
            let mut descriptor = Descriptor::default();
            let bytes = memory.artifact(id)?;
            if source.kind == ArchiveKind::HostObservation {
                descriptor = plan_descriptor(memory, id, &bytes)?;
            } else if source.kind == ArchiveKind::ToolMessage {
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

fn plan_descriptor(
    memory: &LocalMemory,
    id: u64,
    bytes: &[u8],
) -> Result<Descriptor, DeveloperLoopError> {
    use super::operation_plan::{PlanRecord, decode_message, load_record};
    let mut descriptor = Descriptor::default();
    if decode_message(bytes)?.is_none() {
        return Ok(descriptor);
    }
    match load_record(memory, id)? {
        PlanRecord::Proposal { .. } => {}
        PlanRecord::Continuation { proposal, steps, next } => {
            if steps.len() != next as usize || steps.len() > 64 {
                return Err(super::error("invalid plan continuation dependencies"));
            }
            descriptor.links.push(proposal);
            descriptor.links.extend(steps);
        }
        PlanRecord::Step { proposal, index, arguments, output, dependencies, .. } => {
            if proposal >= id {
                return Err(super::error("plan proposal is not an earlier source"));
            }
            let PlanRecord::Proposal { nodes, .. } = load_record(memory, proposal)? else {
                return Err(super::error("plan step has no original proposal"));
            };
            let node = nodes
                .get(index as usize)
                .ok_or_else(|| super::error("plan step ordinal unavailable"))?;
            if let Some(path) = arguments["path"].as_str() {
                descriptor.paths.push(path.to_owned());
            }
            let value: Value = serde_json::from_slice(&memory.store.read(output)?)
                .map_err(|_| super::error("invalid exact plan output"))?;
            collect(memory, id, &value, Some(node.tool.name()), &mut descriptor)?;
            descriptor.links.push(proposal);
            descriptor.links.extend(dependencies);
        }
    }
    if descriptor.links.iter().any(|source| *source >= id) {
        return Err(super::error("plan receipt dependency is not backward scoped"));
    }
    Ok(descriptor)
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
