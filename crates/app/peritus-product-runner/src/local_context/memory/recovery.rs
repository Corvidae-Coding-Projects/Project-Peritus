//! Exact checkpoint reconstruction plus an ordered uncovered journal suffix.

use super::super::{
    checkpoint_validation, error,
    record::{
        ArchiveKind, ArchivedObservation, CHECKPOINT_SCHEMA_VERSION,
        CONTEXT_UPDATE_ENTRY_PAGE_SCHEMA_VERSION, CONTEXT_UPDATE_PAGE_SCHEMA_VERSION,
        CONTEXT_UPDATE_REDUCER_SCHEMA_VERSION, CONTEXT_UPDATE_SCHEMA_VERSION,
        CheckpointManifest, ContextUpdateEntryPage, ContextUpdateEntryStatus,
        ContextUpdateRecord, ContextUpdateReducer, ContextUpdateReducerPage,
        ContextUpdateRoot, ContextUpdateTranscriptPage, INDEXED_CHECKPOINT_SCHEMA_VERSION,
        INDEX_PAGE_SCHEMA_VERSION, InlineContextUpdate, LEGACY_CHECKPOINT_SCHEMA_VERSION,
        LEGACY_CONTEXT_UPDATE_SCHEMA_VERSION, MemoryRecord, PAGED_GENESIS_SCHEMA_VERSION,
        LEGACY_SEGMENT_CONTINUATION_SCHEMA_VERSION, SEGMENT_CONTINUATION_SCHEMA_VERSION,
        PAGED_CHECKPOINT_SCHEMA_VERSION, SNAPSHOT_CHECKPOINT_SCHEMA_VERSION, SourceIndexPage,
        TRANSCRIPT_CONTEXT_UPDATE_SCHEMA_VERSION, TranscriptManifest, ViewValidation, decode,
        encode,
    },
    storage::StoredArtifact,
    view_binding,
};
use super::LocalMemory;
use peritus_agent::DeveloperLoopError;
use peritus_context::ContextNodeId;
use peritus_context::working::{
    ObservationId, ObservationSource, ReusableWorkingStateHistory, WorkingDelta,
    WorkingEntryStatus, WorkingEvent, WorkingState, WorkingStateArtifact,
    WorkingStateReadError, apply_working_event, decode_paged_working_state_with_history_from,
    decode_working_event, decode_working_state, decode_working_state_core,
};
use peritus_model_protocol::{ProtocolLimits, decode_messages};
use peritus_types::Sha256Digest;

const CONTEXT_UPDATE_PAGE_ENTRIES: usize = 255;

impl LocalMemory {
    pub(in crate::local_context) fn recover(&mut self) -> Result<(), DeveloperLoopError> {
        let through = if let Some(bytes) = self.store.checkpoint_root()? {
            let manifest: CheckpointManifest = decode(&bytes)?;
            self.restore_checkpoint(&manifest)?;
            let through = manifest.through_event;
            self.last_checkpoint = Some(manifest);
            through
        } else {
            0
        };
        if self.store.sequence() == 0 {
            let source_index = self.store.store(&encode(&SourceIndexPage {
                schema_version: INDEX_PAGE_SCHEMA_VERSION,
                previous: None,
                first_sequence: 1,
                observations: Vec::new(),
            })?)?;
            let state = super::checkpoint::store_working_snapshot(
                &self.store,
                &self.state,
                source_index,
            )?;
            self.commit(
                &MemoryRecord::GenesisRoot {
                    schema_version: PAGED_GENESIS_SCHEMA_VERSION,
                    state,
                    source_index,
                },
                &[state.digest],
            )?;
            return Ok(());
        }
        let mut cursor = through;
        loop {
            let records = self.store.record_page_after(cursor)?;
            if records.is_empty() {
                break;
            }
            for bytes in records {
                cursor = cursor.checked_add(1).ok_or_else(|| error("journal cursor overflow"))?;
                self.replay_record(decode(&bytes)?, cursor == 1)?;
            }
        }
        checkpoint_validation::validate_index(
            &self.state,
            &self.sources,
            &self.transcript,
            self.limits,
        )?;
        self.validate_invocation_index()?;
        // Derived host projections are completed from committed observations, never from effects.
        self.sync_protocol()?;
        self.ingest_facts()?;
        Ok(())
    }

    fn restore_checkpoint(
        &mut self,
        manifest: &CheckpointManifest,
    ) -> Result<(), DeveloperLoopError> {
        if !matches!(
            manifest.schema_version,
            LEGACY_CHECKPOINT_SCHEMA_VERSION
                | SNAPSHOT_CHECKPOINT_SCHEMA_VERSION
                | INDEXED_CHECKPOINT_SCHEMA_VERSION
                | PAGED_CHECKPOINT_SCHEMA_VERSION
                | CHECKPOINT_SCHEMA_VERSION
        ) || manifest.scope != self.store.scope_digest().into_bytes()
            || manifest.generation != self.store.generation()
            || manifest.through_event >= self.store.sequence()
        {
            return Err(error("checkpoint binding or publication mismatch"));
        }
        checkpoint_validation::validate_schema_lineage(manifest, |digest| {
            self.store.read_digest(digest)
        })?;
        if matches!(
            manifest.schema_version,
            INDEXED_CHECKPOINT_SCHEMA_VERSION
                | PAGED_CHECKPOINT_SCHEMA_VERSION
                | CHECKPOINT_SCHEMA_VERSION
        ) {
            self.restore_checkpoint_indexes(
                manifest.schema_version,
                manifest.source_index,
                manifest.transcript_manifest,
            )?;
            let observations = self
                .sources
                .iter()
                .map(|source| {
                    ObservationSource::new(
                        ObservationId::new(source.sequence)
                            .map_err(|_| error("invalid checkpoint observation sequence"))?,
                        source.artifact.digest,
                        source.artifact.bytes,
                        0,
                        source.artifact.bytes,
                        source.kind.source_kind(),
                    )
                    .map_err(|_| error("invalid checkpoint observation source"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let working = self.store.read(manifest.working_state)?;
            self.state = if matches!(
                manifest.schema_version,
                PAGED_CHECKPOINT_SCHEMA_VERSION | CHECKPOINT_SCHEMA_VERSION
            ) {
                let source_index = WorkingStateArtifact::new(
                    manifest.source_index.digest.into_bytes(),
                    manifest.source_index.bytes,
                )
                .map_err(|_| error("invalid paged source-index reference"))?;
                let (state, history) = decode_paged_working_state_with_history_from(
                    &working,
                    source_index,
                    &observations,
                    self.binding,
                    self.limits,
                    |artifact| self.store.read(super::super::storage::StoredArtifact {
                        digest: Sha256Digest::new(artifact.digest()),
                        bytes: artifact.bytes(),
                    }),
                )
                .map_err(|failure| match failure {
                    WorkingStateReadError::Artifact(failure) => failure,
                    WorkingStateReadError::Codec(_) => error("invalid paged working checkpoint"),
                })?;
                self.working_history = Some(history);
                state
            } else {
                self.working_history = None;
                decode_working_state_core(
                    &working,
                    &observations,
                    self.binding,
                    self.limits,
                )
                .map_err(|_| error("invalid incremental working checkpoint"))?
            };
        } else {
            self.working_history = None;
            self.state = decode_working_state(
                &self.store.read(manifest.working_state)?,
                self.binding,
                self.limits,
            )
            .map_err(|_| error("invalid working checkpoint"))?;
            self.sources = decode(&self.store.read(manifest.source_index)?)?;
            self.transcript = decode(&self.store.read(manifest.transcript_manifest)?)?;
            self.restore_legacy_invocation_index(manifest.through_event)?;
        }
        let view_bytes = self.store.read(manifest.view)?;
        self.last_view = decode_messages(&view_bytes, ProtocolLimits::PRODUCTION)?;
        let validation_bytes = self.store.read(manifest.validation)?;
        let validation: ViewValidation = decode(&validation_bytes)?;
        checkpoint_validation::validate_checkpoint(
            manifest.schema_version,
            &self.state,
            &self.sources,
            &self.transcript,
            &validation,
            self.limits,
        )?;
        self.segment_continuation.clone_from(&validation.segment_continuation);
        view_binding::verify(manifest, &view_bytes, &validation)?;
        self.local_compactor_failures = validation.local_compactor_failures;
        self.retrieval_calls = validation.retrieval_calls;
        self.model_revision = validation.model_revision;
        let manifest_bytes = encode(manifest)?;
        let manifest_artifact = super::super::storage::StoredArtifact {
            digest: peritus_codec::sha256(&manifest_bytes),
            bytes: u64::try_from(manifest_bytes.len())
                .map_err(|_| error("checkpoint manifest size overflow"))?,
        };
        let event = encode(&MemoryRecord::Checkpoint { manifest: manifest_artifact })?;
        self.last_checkpoint_owner = Some(self.store.checkpoint_owner(
            manifest
                .through_event
                .checked_add(1)
                .ok_or_else(|| error("checkpoint event sequence overflow"))?,
            &event,
        )?);
        if matches!(
            manifest.schema_version,
            INDEXED_CHECKPOINT_SCHEMA_VERSION
                | PAGED_CHECKPOINT_SCHEMA_VERSION
                | CHECKPOINT_SCHEMA_VERSION
        ) {
            self.retry_previous_checkpoint_retirement(manifest);
        }
        Ok(())
    }

    fn restore_legacy_invocation_index(
        &mut self,
        through_event: u64,
    ) -> Result<(), DeveloperLoopError> {
        self.invocations.clear();
        self.invocation_ranges.clear();
        self.indexed_invocation_count = 0;
        let mut cursor = 0_u64;
        while cursor < through_event {
            let records = self.store.record_page_after(cursor)?;
            if records.is_empty() {
                return Err(error("legacy invocation index does not reach checkpoint frontier"));
            }
            for bytes in records {
                cursor = cursor
                    .checked_add(1)
                    .ok_or_else(|| error("legacy invocation cursor overflow"))?;
                if cursor > through_event {
                    break;
                }
                if let MemoryRecord::Invocation { sequence, request_prefix } = decode(&bytes)? {
                    self.adopt_invocation(sequence, request_prefix)?;
                }
            }
        }
        self.validate_invocation_index()
    }

    fn retry_previous_checkpoint_retirement(&self, manifest: &CheckpointManifest) {
        let Some(previous_digest) = manifest.previous else {
            return;
        };
        let Ok(bytes) = self.store.read_digest(previous_digest) else {
            // Successful prior retirement can make the obsolete manifest unreachable.
            return;
        };
        let retirement = (|| -> Result<(), DeveloperLoopError> {
            if peritus_codec::sha256(&bytes).into_bytes() != previous_digest {
                return Err(error("obsolete checkpoint predecessor digest mismatch"));
            }
            let previous: CheckpointManifest = decode(&bytes)?;
            if previous.scope != manifest.scope
                || previous.generation.checked_add(1) != Some(manifest.generation)
            {
                return Err(error("obsolete checkpoint predecessor lineage mismatch"));
            }
            let artifact = super::super::storage::StoredArtifact {
                digest: peritus_codec::sha256(&bytes),
                bytes: u64::try_from(bytes.len())
                    .map_err(|_| error("obsolete checkpoint manifest size overflow"))?,
            };
            let event = encode(&MemoryRecord::Checkpoint { manifest: artifact })?;
            let owner = self.store.checkpoint_owner(
                previous
                    .through_event
                    .checked_add(1)
                    .ok_or_else(|| error("obsolete checkpoint event sequence overflow"))?,
                &event,
            )?;
            self.store.retire_checkpoint_owner(owner)
        })();
        if let Err(retire) = retirement {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr().lock(),
                "obsolete local checkpoint reference retirement remains deferred: {retire}"
            );
        }
    }

    fn replay_record(
        &mut self,
        record: MemoryRecord,
        first: bool,
    ) -> Result<(), DeveloperLoopError> {
        match record {
            MemoryRecord::Genesis { state } => {
                if !first {
                    return Err(error("duplicate genesis record"));
                }
                let state =
                    decode_working_state(&self.store.read(state)?, self.binding, self.limits)
                        .map_err(|_| error("invalid genesis state"))?;
                if state.revision() != 0 || state.through_observation() != 0 {
                    return Err(error("genesis is not empty"));
                }
                self.state = state;
            }
            MemoryRecord::GenesisRoot {
                schema_version,
                state,
                source_index,
            } => {
                if !first || schema_version != PAGED_GENESIS_SCHEMA_VERSION {
                    return Err(error("invalid paged genesis record"));
                }
                let page: SourceIndexPage = decode(&self.store.read(source_index)?)?;
                if page.schema_version != INDEX_PAGE_SCHEMA_VERSION
                    || page.previous.is_some()
                    || page.first_sequence != 1
                    || !page.observations.is_empty()
                    || !self.sources.is_empty()
                {
                    return Err(error("invalid paged genesis source index"));
                }
                let (state, history) = self.read_linked_working_snapshot(state, source_index)?;
                if state.revision() != 0 || state.through_observation() != 0 {
                    return Err(error("genesis is not empty"));
                }
                self.state = state;
                self.working_history = Some(history);
            }
            MemoryRecord::Invocation { sequence, request_prefix } => {
                if self.transcript.invocation.checked_add(1) != Some(sequence) {
                    return Err(error("invalid invocation prefix"));
                }
                self.adopt_invocation(sequence, request_prefix.clone())?;
                self.transcript.invocation = sequence;
                self.transcript.request_prefix = request_prefix;
                self.transcript.current_inputs.clear();
                self.transcript.message_ids.clear();
                self.segment_continuation = None;
            }
            MemoryRecord::InvocationCompleted {
                schema_version,
                invocation,
                request_prefix,
                segment_sequence,
            } => {
                let active = self
                    .segment_continuation
                    .as_ref()
                    .ok_or_else(|| error("segment completion has no active continuation"))?;
                if !matches!(
                    schema_version,
                    LEGACY_SEGMENT_CONTINUATION_SCHEMA_VERSION
                        | SEGMENT_CONTINUATION_SCHEMA_VERSION
                )
                    || schema_version != active.schema_version
                    || active.invocation != invocation
                    || active.request_prefix != request_prefix
                    || active.segment_sequence != segment_sequence
                    || self.transcript.invocation != invocation
                    || self.transcript.request_prefix != request_prefix
                {
                    return Err(error("segment completion identity mismatch"));
                }
                self.segment_continuation = None;
            }
            MemoryRecord::ToolEffectUncertain {
                invocation,
                request_prefix,
                tool_sequence,
                call,
            } => {
                self.apply_tool_effect_uncertain(
                    invocation,
                    &request_prefix,
                    tool_sequence,
                    &call,
                )?;
            }
            MemoryRecord::Observation { observation, reducer } => {
                self.validate_observation(&observation)?;
                let event =
                    decode_working_event(&self.store.read(reducer)?, self.binding, self.limits)
                        .map_err(|_| error("invalid observation event"))?;
                let WorkingEvent::Observation { source, .. } = &event else {
                    return Err(error("source record is not an observation"));
                };
                observation.validate_locator(*source)?;
                self.store.read(observation.artifact)?;
                self.state = apply_working_event(&self.state, &event)
                    .map_err(|_| error("observation replay rejected"))?;
                self.sources.push(observation.clone());
                self.project_observation(&observation)?;
            }
            MemoryRecord::StateEvent { reducer } => {
                let event =
                    decode_working_event(&self.store.read(reducer)?, self.binding, self.limits)
                        .map_err(|_| error("invalid state event"))?;
                if matches!(event, WorkingEvent::Observation { .. }) {
                    return Err(error("unindexed observation event"));
                }
                self.model_revision = self.next_model_revision(&event)?;
                self.state = apply_working_event(&self.state, &event)
                    .map_err(|_| error("working-state replay rejected"))?;
            }
            MemoryRecord::ContextUpdate(record) => self.replay_context_update(record)?,
            MemoryRecord::Transcript { manifest } => {
                checkpoint_validation::validate_transcript(
                    &self.state,
                    &self.sources,
                    &manifest,
                    self.transcript.invocation,
                    self.limits,
                )?;
                self.transcript = manifest;
            }
            MemoryRecord::TranscriptIndex { transcript } => {
                self.replay_transcript_index(transcript)?;
            }
            MemoryRecord::CheckpointIndex { source, transcript } => {
                self.replay_checkpoint_index(source, transcript)?;
            }
            MemoryRecord::HostIndex { root } => {
                self.replay_host_index(root)?;
            }
            MemoryRecord::CheckpointHostIndex { source, root } => {
                self.replay_checkpoint_host_index(source, root)?;
            }
            MemoryRecord::Checkpoint { manifest } => {
                let bytes = self.store.read(manifest)?;
                if self.last_checkpoint.as_ref().map(encode).transpose()?.as_deref()
                    != Some(bytes.as_slice())
                {
                    return Err(error("checkpoint event has no matching published root"));
                }
            }
            MemoryRecord::Compactor { input, output, failed } => {
                for artifact in [input, output].into_iter().flatten() {
                    self.store.read(artifact)?;
                }
                if failed {
                    self.local_compactor_failures = self
                        .local_compactor_failures
                        .checked_add(1)
                        .ok_or_else(|| error("local compactor failure counter overflow"))?;
                }
            }
        }
        Ok(())
    }

    fn replay_context_update(
        &mut self,
        record: ContextUpdateRecord,
    ) -> Result<(), DeveloperLoopError> {
        match record {
            ContextUpdateRecord::Inline(update) => self.replay_inline_context_update(update),
            ContextUpdateRecord::Root(update) => {
                if !matches!(
                    update.schema_version,
                    TRANSCRIPT_CONTEXT_UPDATE_SCHEMA_VERSION | CONTEXT_UPDATE_SCHEMA_VERSION
                ) {
                    return Err(error("unsupported context update root record"));
                }
                let root: ContextUpdateRoot = decode(&self.store.read(update.root)?)?;
                self.replay_context_update_root(root)
            }
        }
    }

    fn replay_inline_context_update(
        &mut self,
        update: InlineContextUpdate,
    ) -> Result<(), DeveloperLoopError> {
        if update.schema_version != LEGACY_CONTEXT_UPDATE_SCHEMA_VERSION
            || update.base_model_revision != self.model_revision
            || update.reducers.is_empty()
        {
            return Err(error("context update model revision mismatch"));
        }
        let mut expected_transcript = self.transcript.clone();
        expected_transcript.files.clone_from(&update.transcript.files);
        if update.transcript != expected_transcript {
            return Err(error("context update changed a host-owned transcript field"));
        }
        let mut successor = self.state.clone();
        let mut saw_refresh = false;
        let mut saw_delta = false;
        for reducer in update.reducers {
            let event = decode_working_event(
                &self.store.read(reducer)?,
                self.binding,
                self.limits,
            )
            .map_err(|_| error("invalid context update reducer"))?;
            validate_context_reducer_sequence(&event, &mut saw_refresh, &mut saw_delta)?;
            successor = apply_working_event(&successor, &event)
                .map_err(|_| error("context update replay rejected"))?;
        }
        self.adopt_context_update(
            update.base_model_revision,
            successor,
            update.transcript,
            saw_delta,
        )
    }

    fn replay_context_update_root(
        &mut self,
        root: ContextUpdateRoot,
    ) -> Result<(), DeveloperLoopError> {
        let source_count = u64::try_from(self.sources.len())
            .map_err(|_| error("context update source count overflow"))?;
        if !matches!(
            root.schema_version,
            TRANSCRIPT_CONTEXT_UPDATE_SCHEMA_VERSION | CONTEXT_UPDATE_SCHEMA_VERSION
        )
            || root.base_model_revision != self.model_revision
            || root.reducer_count == 0
            || self.source_index_tail != Some(root.source_index)
            || self.indexed_source_count != source_count
        {
            return Err(error("context update root binding mismatch"));
        }

        let mut successor = self.state.clone();
        let mut saw_refresh = false;
        let mut saw_delta = false;
        let mut expected_reducer = 0_u64;
        let mut next = Some(root.reducer_head);
        while let Some(artifact) = next {
            let page: ContextUpdateReducerPage = decode(&self.store.read(artifact)?)?;
            let count = u64::try_from(page.reducers.len())
                .map_err(|_| error("context update reducer page count overflow"))?;
            let end = expected_reducer
                .checked_add(count)
                .ok_or_else(|| error("context update reducer count overflow"))?;
            if page.schema_version != CONTEXT_UPDATE_PAGE_SCHEMA_VERSION
                || page.first_reducer != expected_reducer
                || count == 0
                || page.reducers.len() > CONTEXT_UPDATE_PAGE_ENTRIES
                || end > root.reducer_count
            {
                return Err(error("invalid context update reducer page"));
            }
            for reducer in page.reducers {
                let (event, expected_successor) = self.read_context_reducer(
                    reducer,
                    root.source_index,
                    &successor,
                )?;
                validate_context_reducer_sequence(
                    &event,
                    &mut saw_refresh,
                    &mut saw_delta,
                )?;
                let applied = apply_working_event(&successor, &event)
                    .map_err(|_| error("context update replay rejected"))?;
                if let Some(expected_successor) = expected_successor
                    && applied != expected_successor
                {
                    return Err(error("context update reducer snapshot mismatch"));
                }
                successor = applied;
                expected_reducer = expected_reducer
                    .checked_add(1)
                    .ok_or_else(|| error("context update reducer count overflow"))?;
            }
            if expected_reducer != end {
                return Err(error("context update reducer page is incomplete"));
            }
            next = page.next;
        }
        if expected_reducer != root.reducer_count {
            return Err(error("context update reducer chain is incomplete"));
        }

        let transcript = self.read_context_update_transcript(&root)?;
        self.adopt_context_update(
            root.base_model_revision,
            successor,
            transcript,
            saw_delta,
        )
    }

    fn read_context_reducer(
        &self,
        artifact: StoredArtifact,
        source_index: StoredArtifact,
        current: &WorkingState,
    ) -> Result<(WorkingEvent, Option<WorkingState>), DeveloperLoopError> {
        let reducer: ContextUpdateReducer = decode(&self.store.read(artifact)?)?;
        match reducer {
            ContextUpdateReducer::Event {
                schema_version,
                event,
            } => {
                if schema_version != CONTEXT_UPDATE_REDUCER_SCHEMA_VERSION {
                    return Err(error("unsupported context update event reducer"));
                }
                let event = decode_working_event(
                    &self.store.read(event)?,
                    self.binding,
                    self.limits,
                )
                .map_err(|_| error("invalid linked context update event"))?;
                if !matches!(event, WorkingEvent::Delta(_)) {
                    return Err(error("linked context update event is not a delta"));
                }
                Ok((event, None))
            }
            ContextUpdateReducer::Refresh {
                schema_version,
                base_revision,
                state,
            } => {
                if schema_version != CONTEXT_UPDATE_REDUCER_SCHEMA_VERSION {
                    return Err(error("unsupported context update refresh reducer"));
                }
                let state = self.read_linked_working_state(state, source_index)?;
                let event = WorkingEvent::Refresh {
                    base_revision,
                    environment: state.environment().clone(),
                };
                Ok((event, Some(state)))
            }
            ContextUpdateReducer::DeltaSnapshot {
                schema_version,
                base_revision,
                state,
                entry_head,
                entry_count,
            } => {
                if schema_version != CONTEXT_UPDATE_REDUCER_SCHEMA_VERSION
                    || entry_count == 0
                {
                    return Err(error("unsupported context update delta reducer"));
                }
                let state = self.read_linked_working_state(state, source_index)?;
                let entries = self.read_context_delta_entries(entry_head, entry_count, &state)?;
                let delta = WorkingDelta::new(
                    current.binding(),
                    base_revision,
                    entries,
                    self.limits,
                )
                .map_err(|_| error("invalid linked context update delta"))?;
                Ok((WorkingEvent::Delta(delta), Some(state)))
            }
        }
    }

    fn read_context_delta_entries(
        &self,
        head: StoredArtifact,
        entry_count: u64,
        target: &WorkingState,
    ) -> Result<Vec<peritus_context::working::WorkingEntry>, DeveloperLoopError> {
        let mut entries = Vec::new();
        let mut expected_entry = 0_u64;
        let mut previous: Option<ContextNodeId> = None;
        let mut next = Some(head);
        while let Some(artifact) = next {
            let page: ContextUpdateEntryPage = decode(&self.store.read(artifact)?)?;
            let count = u64::try_from(page.entries.len())
                .map_err(|_| error("context update entry page count overflow"))?;
            let end = expected_entry
                .checked_add(count)
                .ok_or_else(|| error("context update entry count overflow"))?;
            if page.schema_version != CONTEXT_UPDATE_ENTRY_PAGE_SCHEMA_VERSION
                || page.first_entry != expected_entry
                || count == 0
                || page.entries.len() > CONTEXT_UPDATE_PAGE_ENTRIES
                || end > entry_count
            {
                return Err(error("invalid context update entry page"));
            }
            for identity in page.entries {
                let id = ContextNodeId::new(identity.id)
                    .map_err(|_| error("invalid context update entry identity"))?;
                if previous.is_some_and(|previous| previous >= id) {
                    return Err(error("noncanonical context update entry identity order"));
                }
                let entry = target
                    .entry(target.binding(), id)
                    .map_err(|_| error("context update entry is absent from its snapshot"))?
                    .clone()
                    .with_status(match identity.status {
                        ContextUpdateEntryStatus::Open => WorkingEntryStatus::Open,
                        ContextUpdateEntryStatus::Contradicted => {
                            WorkingEntryStatus::Contradicted
                        }
                        ContextUpdateEntryStatus::Resolved => WorkingEntryStatus::Resolved,
                    })
                    .map_err(|_| error("invalid context update entry status"))?;
                entries.push(entry);
                previous = Some(id);
                expected_entry = expected_entry
                    .checked_add(1)
                    .ok_or_else(|| error("context update entry count overflow"))?;
            }
            if expected_entry != end {
                return Err(error("context update entry page is incomplete"));
            }
            next = page.next;
        }
        if expected_entry != entry_count {
            return Err(error("context update entry chain is incomplete"));
        }
        Ok(entries)
    }

    fn read_linked_working_state(
        &self,
        state: StoredArtifact,
        source_index: StoredArtifact,
    ) -> Result<WorkingState, DeveloperLoopError> {
        self.read_linked_working_snapshot(state, source_index)
            .map(|(state, _)| state)
    }

    fn read_linked_working_snapshot(
        &self,
        state: StoredArtifact,
        source_index: StoredArtifact,
    ) -> Result<(WorkingState, ReusableWorkingStateHistory), DeveloperLoopError> {
        let observations = self
            .sources
            .iter()
            .map(|source| {
                ObservationSource::new(
                    ObservationId::new(source.sequence)
                        .map_err(|_| error("invalid linked observation sequence"))?,
                    source.artifact.digest,
                    source.artifact.bytes,
                    0,
                    source.artifact.bytes,
                    source.kind.source_kind(),
                )
                .map_err(|_| error("invalid linked observation source"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let source = WorkingStateArtifact::new(
            source_index.digest.into_bytes(),
            source_index.bytes,
        )
        .map_err(|_| error("invalid linked source-index reference"))?;
        decode_paged_working_state_with_history_from(
            &self.store.read(state)?,
            source,
            &observations,
            self.binding,
            self.limits,
            |artifact| {
                self.store.read(StoredArtifact {
                    digest: Sha256Digest::new(artifact.digest()),
                    bytes: artifact.bytes(),
                })
            },
        )
        .map_err(|failure| match failure {
            WorkingStateReadError::Artifact(failure) => failure,
            WorkingStateReadError::Codec(_) => error("invalid linked working state"),
        })
    }

    fn read_context_update_transcript(
        &self,
        root: &ContextUpdateRoot,
    ) -> Result<TranscriptManifest, DeveloperLoopError> {
        let mut transcript = self.transcript.clone();
        let before = if root.schema_version == TRANSCRIPT_CONTEXT_UPDATE_SCHEMA_VERSION {
            super::checkpoint::transcript_digest(&transcript)?
        } else {
            super::checkpoint::observed_file_digest(&transcript.files)?
        };
        if before != root.transcript_before {
            return Err(error("context update transcript predecessor mismatch"));
        }
        let mut expected_change = 0_u64;
        let mut next = root.transcript_head;
        while let Some(artifact) = next {
            let page: ContextUpdateTranscriptPage = decode(&self.store.read(artifact)?)?;
            expected_change = super::checkpoint::apply_context_transcript_page(
                &mut transcript,
                &page,
                expected_change,
                root.schema_version,
            )?;
            if expected_change > root.transcript_change_count {
                return Err(error("context update transcript exceeds declared changes"));
            }
            next = page.next;
        }
        let after = if root.schema_version == TRANSCRIPT_CONTEXT_UPDATE_SCHEMA_VERSION {
            super::checkpoint::transcript_digest(&transcript)?
        } else {
            super::checkpoint::observed_file_digest(&transcript.files)?
        };
        if expected_change != root.transcript_change_count || after != root.transcript_after {
            return Err(error("context update transcript chain is incomplete"));
        }
        Ok(transcript)
    }

    fn adopt_context_update(
        &mut self,
        base_model_revision: u64,
        successor: WorkingState,
        transcript: TranscriptManifest,
        saw_delta: bool,
    ) -> Result<(), DeveloperLoopError> {
        if !saw_delta {
            return Err(error("context update has no working delta"));
        }
        checkpoint_validation::validate_transcript(
            &successor,
            &self.sources,
            &transcript,
            self.transcript.invocation,
            self.limits,
        )?;
        let model_revision = base_model_revision
            .checked_add(1)
            .ok_or_else(|| error("model revision overflow"))?;
        self.state = successor;
        self.transcript = transcript;
        self.model_revision = model_revision;
        Ok(())
    }

    fn validate_observation(&self, source: &ArchivedObservation) -> Result<(), DeveloperLoopError> {
        let expected = u64::try_from(self.sources.len())
            .ok()
            .and_then(|count| count.checked_add(1))
            .ok_or_else(|| error("source index sequence overflow"))?;
        if source.sequence != expected
            || source.invocation != self.transcript.invocation
            || source.invocation == 0
        {
            return Err(error("source index is not contiguous or invocation-bound"));
        }
        if let Some(call) = &source.call {
            if call.id.is_empty()
                || call.name.is_empty()
                || source.kind != ArchiveKind::ToolOutput
            {
                return Err(error("invalid archived call identity"));
            }
        } else if source.kind == ArchiveKind::ToolOutput {
            return Err(error("tool output lacks call identity"));
        }
        Ok(())
    }
}

fn validate_context_reducer_sequence(
    event: &WorkingEvent,
    saw_refresh: &mut bool,
    saw_delta: &mut bool,
) -> Result<(), DeveloperLoopError> {
    match event {
        WorkingEvent::Refresh { .. } if !*saw_refresh && !*saw_delta => {
            *saw_refresh = true;
        }
        WorkingEvent::Delta(_) => *saw_delta = true,
        WorkingEvent::Refresh { .. }
        | WorkingEvent::Observation { .. }
        | WorkingEvent::Protocol(_) => {
            return Err(error("invalid context update reducer sequence"));
        }
    }
    Ok(())
}
