//! Rebuildable suffix index. Only proven identical successful inspections may share a card.

use std::collections::BTreeMap;

use crate::local_context::operation_plan::{PlanRecord, decode_message, load_record};
use crate::local_context::{
    error, memory::LocalMemory, record::ArchiveKind, storage::StoredArtifact,
};
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_model_protocol::ContentBlock;

#[derive(Clone)]
pub(super) struct Evidence {
    pub(super) source: u64,
    pub(super) tool: String,
    pub(super) arguments: [u8; 32],
    pub(super) artifact: StoredArtifact,
    pub(super) is_error: bool,
    pub(super) arguments_message: Option<(u64, usize)>,
    pub(super) host_step: bool,
    pub(super) file: Option<String>,
}

#[derive(Default)]
pub(in crate::local_context) struct Index {
    through: usize,
    cards: BTreeMap<u64, Evidence>,
    inspections: BTreeMap<(String, [u8; 32], [u8; 32]), u64>,
    aliases: BTreeMap<u64, u64>,
    calls: BTreeMap<(u64, String), (u64, usize)>,
    root: [u8; 32],
    unknown_dependencies: bool,
}

impl Index {
    pub(super) fn evidence(&self) -> Vec<Evidence> {
        self.cards.values().cloned().collect()
    }

    pub(super) fn representative(&self, id: u64) -> u64 {
        self.aliases.get(&id).copied().unwrap_or(id)
    }

    pub(super) const fn processed(&self) -> usize {
        self.through
    }

    pub(super) const fn root(&self) -> [u8; 32] {
        self.root
    }

    // Immutable source classification is derived once per appended observation. This is
    // not a current-file, permission, candidate or request-validity cache.
    pub(super) const fn dependencies_are_managed(&self) -> bool {
        !self.unknown_dependencies
    }

    pub(super) fn refresh(&mut self, memory: &LocalMemory) -> Result<(), DeveloperLoopError> {
        if self.through > memory.sources.len() {
            return Err(error("decision evidence index moved beyond committed observations"));
        }
        for source in memory.sources.iter().skip(self.through) {
            if source.kind == ArchiveKind::Assistant {
                let message = memory.message(source.sequence)?;
                for (block, value) in message.content().iter().enumerate() {
                    if let ContentBlock::ToolCall(call) = value {
                        self.calls.insert(
                            (source.invocation, call.id().expose_for_wire().to_owned()),
                            (source.sequence, block),
                        );
                    }
                }
            }
            if let Some(mut evidence) = evidence(memory, source.sequence)? {
                // Directory/search results can include ignored files, empty directories,
                // and other metadata outside the managed source snapshot.
                let file_tool = evidence.tool == "workspace_read";
                let memory_tool = matches!(
                    evidence.tool.as_str(),
                    "context_update"
                        | "context_read"
                        | "context_sources"
                        | "inspection_plan"
                        | "inspection_result_read"
                );
                if !memory_tool && !evidence.is_error && file_tool {
                    let value: serde_json::Value =
                        serde_json::from_slice(&memory.store.read(evidence.artifact)?)
                            .map_err(|_| error("decode decision source descriptor"))?;
                    if evidence.tool == "workspace_read" {
                        evidence.file = value["path"].as_str().map(str::to_owned);
                    }
                    self.unknown_dependencies |= value["path"].as_str().is_none_or(|path| {
                        std::path::Path::new(path).is_absolute()
                            || crate::workspace_filter::generated(std::path::Path::new(path))
                    });
                } else if !memory_tool {
                    self.unknown_dependencies = true;
                }
                if let Some(call) = &source.call {
                    evidence.arguments_message =
                        self.calls.get(&(source.invocation, call.id.clone())).copied();
                }
                let reusable = !evidence.is_error
                    && matches!(
                        evidence.tool.as_str(),
                        "workspace_read"
                            | "workspace_list"
                            | "workspace_search"
                            | "skill_read"
                            | "skill_list"
                    );
                let key = (
                    evidence.tool.clone(),
                    evidence.arguments,
                    evidence.artifact.digest.into_bytes(),
                );
                if reusable && let Some(prior) = self.inspections.get(&key) {
                    self.aliases.insert(evidence.source, *prior);
                } else {
                    if reusable {
                        self.inspections.insert(key, evidence.source);
                    }
                    self.cards.insert(evidence.source, evidence);
                }
            }
            let metadata = serde_json::to_vec(source)
                .map_err(|_| error("encode decision source-root metadata"))?;
            let mut binding = b"peritus-decision-source-root-v1\0".to_vec();
            binding.extend_from_slice(&self.root);
            binding.extend_from_slice(&metadata);
            self.root = sha256(&binding).into_bytes();
            self.through += 1;
        }
        Ok(())
    }
}

fn evidence(memory: &LocalMemory, id: u64) -> Result<Option<Evidence>, DeveloperLoopError> {
    let source = memory.archived(id)?;
    if source.kind == ArchiveKind::ToolOutput {
        let call =
            source.call.as_ref().ok_or_else(|| error("decision evidence has no tool binding"))?;
        return Ok(Some(Evidence {
            source: id,
            tool: call.name.clone(),
            arguments: call.arguments_digest,
            artifact: source.artifact,
            is_error: source.is_error,
            arguments_message: None,
            host_step: false,
            file: None,
        }));
    }
    if source.kind != ArchiveKind::HostObservation {
        return Ok(None);
    }
    if decode_message(&memory.artifact(id)?)?.is_none() {
        return Ok(None);
    }
    let PlanRecord::Step { proposal, index, arguments, output, is_error, .. } =
        load_record(memory, id)?
    else {
        return Ok(None);
    };
    let PlanRecord::Proposal { nodes, .. } = load_record(memory, proposal)? else {
        return Err(error("decision plan step has no original proposal"));
    };
    let node =
        nodes.get(index as usize).ok_or_else(|| error("decision plan ordinal unavailable"))?;
    Ok(Some(Evidence {
        source: id,
        tool: node.tool.name().to_owned(),
        arguments: sha256(arguments.to_string().as_bytes()).into_bytes(),
        artifact: output,
        is_error,
        arguments_message: None,
        host_step: true,
        file: None,
    }))
}
