//! Mechanical failed-approach records, derived only from completed host tool observations.

use super::super::super::{error, record::ArchiveKind};
use super::{LocalMemory, environment};
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_context::{
    ContextLimits, ContextNodeId, bind_context_content,
    working::{
        ObservationId, WorkingDelta, WorkingEntry, WorkingEntryKind, WorkingEntryStatus,
        WorkingEvent, WorkingLinks, WorkingValidity,
    },
};

impl LocalMemory {
    pub(in crate::local_context) fn derived_memory_allowed(&self) -> bool {
        peritus_role::RoleProfile::for_harness_role(self.binding.role())
            .context()
            .memory_visibility()
            != peritus_role::MemoryVisibility::Excluded
    }

    pub(in crate::local_context) fn ingest_facts(&mut self) -> Result<(), DeveloperLoopError> {
        if !self.derived_memory_allowed() {
            return Ok(());
        }
        let pending = self
            .sources
            .iter()
            .filter(|source| {
                source.sequence > self.transcript.facts_through
                    && source.kind == ArchiveKind::ToolOutput
            })
            .cloned()
            .collect::<Vec<_>>();
        for source in pending {
            let call =
                source.call.as_ref().ok_or_else(|| error("tool fact has no call identity"))?;
            if !call.name.starts_with("context_") {
                // Preserve the original logical key so existing unresolved obligations continue
                // on the same lineage after the history/active split was introduced.
                let obligation = environment::key(
                    format!("failed-approach:{}:{:02x?}", call.name, call.arguments_digest)
                        .as_bytes(),
                )?;
                let previous = current_obligation(self, obligation)?;
                let status = if source.is_error {
                    WorkingEntryStatus::Open
                } else {
                    WorkingEntryStatus::Resolved
                };
                let repeated = previous.is_some_and(|(_, previous_status, previous_source)| {
                    previous_status == status
                        && self.sources.iter().any(|candidate| {
                            candidate.sequence == previous_source
                                && candidate.artifact.digest == source.artifact.digest
                                && candidate.is_error == source.is_error
                        })
                });
                if (source.is_error || previous.is_some()) && !repeated {
                    let id = previous.map_or(obligation, |(previous, _, _)| {
                        transition_id(obligation, source.sequence)
                    });
                    let text = fact_summary(call, &source, status);
                    let text = &text[..text.floor_char_boundary(self.limits.entry_bytes())];
                    let content_limits = ContextLimits::new(
                        self.limits.entries(),
                        self.limits.entry_bytes(),
                        self.limits.links(),
                        5,
                    )
                    .map_err(|_| error("fact content limits"))?;
                    let content = bind_context_content(
                        text.as_bytes().to_vec(),
                        sha256(text.as_bytes()),
                        content_limits,
                    )
                    .map_err(|_| error("bind mechanical fact content"))?;
                    let links = WorkingLinks::new(
                        vec![
                            ObservationId::new(source.sequence)
                                .map_err(|_| error("invalid fact source"))?,
                        ],
                        Vec::new(),
                        Vec::new(),
                        self.limits,
                    )
                    .map_err(|_| error("invalid fact links"))?;
                    let validity = WorkingValidity::new(
                        self.workspace_scope.direct.then_some(self.binding.conversation_revision()),
                        (!self.workspace_scope.direct)
                            .then_some(self.state.environment().candidate()),
                        Vec::new(),
                        self.limits,
                    )
                    .map_err(|_| error("invalid fact validity"))?;
                    let entry = WorkingEntry::new(
                        id,
                        WorkingEntryKind::FailedApproach,
                        content,
                        links,
                        validity,
                        self.limits,
                    )
                    .map_err(|_| error("invalid fact entry"))?
                    .with_status(status)
                    .map_err(|_| error("invalid fact status"))?;
                    let entry = if let Some((previous, _, _)) = previous {
                        entry
                            .with_supersedes(previous)
                            .map_err(|_| error("invalid fact supersession"))?
                    } else {
                        entry
                    };
                    let delta = WorkingDelta::new(
                        self.state.binding(),
                        self.state.revision(),
                        vec![entry],
                        self.limits,
                    )
                    .map_err(|_| error("invalid mechanical delta"))?;
                    self.state_event(&WorkingEvent::Delta(delta))?;
                }
            }
            self.transcript.facts_through = source.sequence;
        }
        self.persist_transcript()
    }
}

fn current_obligation(
    memory: &LocalMemory,
    root: ContextNodeId,
) -> Result<Option<(ContextNodeId, WorkingEntryStatus, u64)>, DeveloperLoopError> {
    let entries = memory
        .state
        .entries(memory.state.binding())
        .map_err(|_| error("failed-approach lineage binding mismatch"))?;
    let Ok(mut index) = entries.binary_search_by_key(&root, WorkingEntry::id) else {
        return Ok(None);
    };
    loop {
        let current = &entries[index];
        if current.kind() != WorkingEntryKind::FailedApproach
            || current.links().supports().len() != 1
        {
            return Err(error("failed-approach lineage is invalid"));
        }
        let mut successor = None;
        for (candidate_index, candidate) in entries.iter().enumerate() {
            if candidate.supersedes() == Some(current.id()) {
                if successor.replace(candidate_index).is_some() {
                    return Err(error("failed-approach lineage has competing successors"));
                }
            }
        }
        match successor {
            Some(next) if current.status() == WorkingEntryStatus::Superseded => index = next,
            Some(_) => return Err(error("failed-approach successor did not retire its predecessor")),
            None if current.status() == WorkingEntryStatus::Superseded => {
                return Err(error("failed-approach lineage is truncated"));
            }
            None => {
                return Ok(Some((
                    current.id(),
                    current.status(),
                    current.links().supports()[0].get(),
                )));
            }
        }
    }
}

fn transition_id(root: ContextNodeId, source: u64) -> ContextNodeId {
    let mut material = b"peritus-failed-approach-transition-v1\0".to_vec();
    material.extend_from_slice(root.as_bytes());
    material.extend_from_slice(&source.to_be_bytes());
    let digest = sha256(&material);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    ContextNodeId::new(id).expect("derived failed-approach identity is nonzero")
}

fn fact_summary(
    call: &super::super::super::record::CallIdentity,
    source: &super::super::super::record::ArchivedObservation,
    status: WorkingEntryStatus,
) -> String {
    format!(
        "obs:{:06}; status={}; tool={:?}; tool_name_sha256={:02x?}; arguments_sha256={:02x?}. The exact occurrence and result remain in the observation archive. Inspect that source before repeating the unchanged approach; this is evidence, not independent acceptance.",
        source.sequence,
        if status == WorkingEntryStatus::Open { "failed" } else { "resolved_by_non_error" },
        call.name,
        sha256(call.name.as_bytes()).into_bytes(),
        call.arguments_digest,
    )
}
