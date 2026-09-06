//! Mechanical failed-approach records, derived only from completed host tool observations.

use super::super::super::{error, record::ArchiveKind};
use super::{LocalMemory, environment};
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_context::{
    ContextLimits, bind_context_content,
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
                let id = environment::key(
                    format!("failed-approach:{}:{:02x?}", call.name, call.arguments_digest)
                        .as_bytes(),
                )?;
                let previous = self.state.entry(self.state.binding(), id).ok();
                if (source.is_error || previous.is_some())
                    && !previous.is_some_and(|entry| {
                        entry.links().supports().iter().any(|id| id.get() == source.sequence)
                            || entry.status() == WorkingEntryStatus::Superseded
                    })
                {
                    let text = format!(
                        "Host tool {} reported {} for arguments SHA-256 {:02x?}. Inspect obs:{:06} before repeating the unchanged approach; this is an observation, not independent acceptance.",
                        call.name,
                        if source.is_error { "failure" } else { "a later non-error result" },
                        call.arguments_digest,
                        source.sequence
                    );
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
                    .with_status(if source.is_error {
                        WorkingEntryStatus::Open
                    } else {
                        WorkingEntryStatus::Resolved
                    })
                    .map_err(|_| error("invalid fact status"))?;
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
