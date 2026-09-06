//! Exact checkpoint reconstruction plus an ordered uncovered journal suffix.

use super::super::{
    error,
    record::{
        ArchiveKind, ArchivedObservation, CheckpointManifest, MemoryRecord, TranscriptManifest,
        ViewValidation, decode, encode,
    },
};
use super::LocalMemory;
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{
    ObservationId, WorkingEvent, apply_working_event, decode_working_event, decode_working_state,
    encode_working_state,
};
use peritus_model_protocol::{ProtocolLimits, decode_messages};

impl LocalMemory {
    pub(in crate::local_context) fn recover(&mut self) -> Result<(), DeveloperLoopError> {
        let records = self.store.records()?;
        let through = if let Some(bytes) = self.store.checkpoint_root()? {
            let manifest: CheckpointManifest = decode(&bytes)?;
            self.restore_checkpoint(&manifest)?;
            let through = manifest.through_event;
            self.last_checkpoint = Some(manifest);
            through
        } else {
            0
        };
        if records.is_empty() {
            let state = self.store.store(
                &encode_working_state(&self.state)
                    .map_err(|_| error("encode initial working state"))?,
            )?;
            self.commit(&MemoryRecord::Genesis { state }, &[state.digest])?;
            return Ok(());
        }
        for (index, bytes) in records.iter().enumerate() {
            if (index as u64) < through {
                continue;
            }
            self.replay_record(decode(bytes)?, index == 0)?;
        }
        self.validate_index()?;
        // Derived host projections are completed from committed observations, never from effects.
        self.sync_protocol()?;
        self.ingest_facts()?;
        Ok(())
    }

    fn restore_checkpoint(
        &mut self,
        manifest: &CheckpointManifest,
    ) -> Result<(), DeveloperLoopError> {
        if manifest.schema_version != 1
            || manifest.scope != self.store.scope_digest().into_bytes()
            || manifest.generation != self.store.generation()
            || manifest.through_event >= self.store.sequence()
        {
            return Err(error("checkpoint binding or publication mismatch"));
        }
        self.state = decode_working_state(
            &self.store.read(manifest.working_state)?,
            self.binding,
            self.limits,
        )
        .map_err(|_| error("invalid working checkpoint"))?;
        self.sources = decode(&self.store.read(manifest.source_index)?)?;
        self.transcript = decode(&self.store.read(manifest.transcript_manifest)?)?;
        self.last_view =
            decode_messages(&self.store.read(manifest.view)?, ProtocolLimits::PRODUCTION)?;
        let validation: ViewValidation = decode(&self.store.read(manifest.validation)?)?;
        if validation.state_revision != self.state.revision()
            || validation.through_observation != self.state.through_observation()
            || validation.estimated_input_tokens > validation.max_input_tokens
            || validation.max_input_tokens == 0
        {
            return Err(error("checkpoint validation does not bind its state"));
        }
        self.local_compactor_failures = validation.local_compactor_failures;
        self.retrieval_calls = validation.retrieval_calls;
        self.model_revision = validation.model_revision;
        self.validate_index()
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
            MemoryRecord::Invocation { sequence, request_prefix } => {
                if self.transcript.invocation.checked_add(1) != Some(sequence)
                    || request_prefix.len() > 256
                {
                    return Err(error("invalid invocation prefix"));
                }
                self.transcript.invocation = sequence;
                self.transcript.request_prefix = request_prefix;
                self.transcript.current_inputs.clear();
                self.transcript.message_ids.clear();
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
            MemoryRecord::Transcript { manifest } => {
                self.validate_transcript(&manifest)?;
                self.transcript = manifest;
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

    fn validate_observation(&self, source: &ArchivedObservation) -> Result<(), DeveloperLoopError> {
        if source.sequence != self.sources.len() as u64 + 1
            || source.invocation != self.transcript.invocation
            || self.sources.len() >= self.limits.observations()
            || source.invocation == 0
        {
            return Err(error("source index is not contiguous or invocation-bound"));
        }
        if let Some(call) = &source.call {
            if call.id.is_empty()
                || call.id.len() > 256
                || call.name.is_empty()
                || call.name.len() > 128
                || source.kind != ArchiveKind::ToolOutput
            {
                return Err(error("invalid archived call identity"));
            }
        } else if source.kind == ArchiveKind::ToolOutput {
            return Err(error("tool output lacks call identity"));
        }
        Ok(())
    }

    fn validate_index(&self) -> Result<(), DeveloperLoopError> {
        if self.sources.len() as u64 != self.state.through_observation()
            || self.sources.len() > self.limits.observations()
        {
            return Err(error("source index size mismatch"));
        }
        for (index, source) in self.sources.iter().enumerate() {
            if source.sequence != index as u64 + 1
                || source.invocation == 0
                || source.invocation > self.transcript.invocation
            {
                return Err(error("invalid source index order"));
            }
            let locator = self
                .state
                .observation(
                    self.state.binding(),
                    ObservationId::new(source.sequence).map_err(|_| error("invalid source id"))?,
                )
                .map_err(|_| error("unresolved indexed observation"))?;
            source.validate_locator(locator)?;
        }
        let mut invocation = 0;
        let mut tool_sequence = 0_u64;
        for source in &self.sources {
            if source.invocation < invocation {
                return Err(error("source invocation order regressed"));
            }
            if source.invocation != invocation {
                invocation = source.invocation;
                tool_sequence = 0;
            }
            if let Some(sequence) = source.tool_sequence {
                tool_sequence =
                    tool_sequence.checked_add(1).ok_or_else(|| error("tool sequence overflow"))?;
                if sequence != tool_sequence {
                    return Err(error("noncontiguous archived tool sequence"));
                }
            }
        }
        self.validate_transcript(&self.transcript)
    }

    fn validate_transcript(
        &self,
        transcript: &TranscriptManifest,
    ) -> Result<(), DeveloperLoopError> {
        if transcript.invocation != self.transcript.invocation
            || transcript.request_prefix.len() > 256
            || transcript.message_ids.len() > ProtocolLimits::PRODUCTION.max_messages()
            || transcript.current_inputs.len() > self.limits.entries()
            || transcript.pending.len() > self.limits.entries()
            || transcript.files.len() > self.limits.entries()
            || transcript.facts_through > self.state.through_observation()
        {
            return Err(error("transcript manifest bounds or identity mismatch"));
        }
        for ids in [&transcript.message_ids, &transcript.current_inputs] {
            if ids.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(error("noncanonical transcript source order"));
            }
            for id in ids {
                let source = self.archived(*id)?;
                if source.invocation != transcript.invocation
                    || source.kind == ArchiveKind::ToolOutput
                {
                    return Err(error("invalid transcript message source"));
                }
            }
        }
        for id in &transcript.current_inputs {
            if !transcript.message_ids.contains(id)
                || !matches!(self.archived(*id)?.kind, ArchiveKind::Policy | ArchiveKind::User)
            {
                return Err(error("invocation input is not a pinned instruction source"));
            }
        }
        if transcript.files.windows(2).any(|pair| pair[0] >= pair[1])
            || transcript.files.iter().any(|path| path.is_empty() || path.len() > 4096)
            || transcript.pending.windows(2).any(|pair| pair[0].key >= pair[1].key)
        {
            return Err(error("noncanonical pending or file projection"));
        }
        for pending in &transcript.pending {
            let source = self.archived(pending.source)?;
            if pending.invocation != source.invocation {
                return Err(error("pending projection invocation mismatch"));
            }
            let expected_key = if let Some(handle) = &pending.handle {
                if source.kind != ArchiveKind::ToolOutput
                    || source.call.as_ref() != Some(&pending.call)
                    || handle.is_empty()
                    || handle.len() > 256
                {
                    return Err(error("pending operation source mismatch"));
                }
                super::environment::key(format!("operation:{handle}").as_bytes())?
            } else {
                if source.kind != ArchiveKind::Assistant {
                    return Err(error("pending proposal source mismatch"));
                }
                super::environment::key(
                    format!("proposal:{}/{}", pending.invocation, pending.call.id).as_bytes(),
                )?
            };
            if expected_key.into_bytes() != pending.key {
                return Err(error("pending identity does not bind its source"));
            }
        }
        Ok(())
    }
}
