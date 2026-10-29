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
    pub(in crate::local_context) fn is_mechanical_entry(
        &self,
        entry: &WorkingEntry,
    ) -> Result<bool, DeveloperLoopError> {
        let [id] = entry.links().supports() else { return Ok(false) };
        if entry.kind() != WorkingEntryKind::FailedApproach
            || !entry.links().contradicts().is_empty()
            || !entry.links().depends_on().is_empty()
            || entry.supersedes().is_some()
        {
            return Ok(false);
        }
        let source = self.archived(id.get())?;
        let Some(call) = source.call.as_ref() else { return Ok(false) };
        if source.kind != ArchiveKind::ToolOutput {
            return Ok(false);
        }
        let expected_id = environment::key(
            format!("failed-approach:{}:{:02x?}", call.name, call.arguments_digest).as_bytes(),
        )?;
        let text = format!(
            "Host tool {} reported {} for arguments SHA-256 {:02x?}. Inspect obs:{:06} before repeating the unchanged approach; this is an observation, not independent acceptance.",
            call.name,
            if source.is_error { "failure" } else { "a later non-error result" },
            call.arguments_digest,
            source.sequence
        );
        let text = &text[..text.floor_char_boundary(self.limits.entry_bytes())];
        Ok(entry.id() == expected_id && entry.content().bytes() == text.as_bytes())
    }
    pub(in crate::local_context) fn derived_memory_allowed(&self) -> bool {
        peritus_role::RoleProfile::for_harness_role(self.binding.role())
            .context()
            .memory_visibility()
            != peritus_role::MemoryVisibility::Excluded
    }

    pub(in crate::local_context) fn ingest_facts(&mut self) -> Result<(), DeveloperLoopError> {
        // A version-5 prototype can be read before a profile is available. Defer new
        // host semantics until preparation can commit the version-6 reader floor.
        if !self.derived_memory_allowed()
            || (self.checkpoint_schema
                == crate::local_context::record::LEGACY_DEPENDENCY_CHECKPOINT_SCHEMA_VERSION
                && self.profile.is_none())
        {
            return Ok(());
        }
        if self.checkpoint_schema
            >= crate::local_context::record::DEPENDENCY_CHECKPOINT_SCHEMA_VERSION
        {
            self.ensure_host_dependencies()?;
        }
        let pending = self
            .sources_after(self.transcript.facts_through)
            .filter(|source| source.kind == ArchiveKind::ToolOutput)
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
                let was_host_bound = previous.is_some_and(|entry| entry.host_binding().is_some());
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
                    self.commit_mechanical_delta(delta, was_host_bound, source.sequence)?;
                }
            }
            self.transcript.facts_through = source.sequence;
        }
        self.persist_transcript()
    }
    fn commit_mechanical_delta(
        &mut self,
        delta: WorkingDelta,
        was_host_bound: bool,
        sequence: u64,
    ) -> Result<(), DeveloperLoopError> {
        if was_host_bound {
            self.state_event(&WorkingEvent::HostDelta {
                delta,
                dependencies: WorkingValidity::uncertain(),
                source_binding: peritus_context::working::WorkingHostBinding::new(sequence, None),
            })
        } else {
            self.state_event(&WorkingEvent::Delta(delta))
        }
    }
}
