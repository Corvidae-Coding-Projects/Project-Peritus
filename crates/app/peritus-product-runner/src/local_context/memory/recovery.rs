//! Exact checkpoint reconstruction plus an ordered uncovered journal suffix.

use super::super::{
    checkpoint_validation, error,
    record::{
        ArchiveKind, ArchivedObservation, CHECKPOINT_SCHEMA_VERSION, CheckpointManifest,
        LEGACY_CHECKPOINT_SCHEMA_VERSION, MemoryRecord, ViewValidation, decode, encode,
    },
    view_binding,
};
use super::LocalMemory;
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{
    WorkingEvent, apply_working_event, decode_working_event, decode_working_state,
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
        checkpoint_validation::validate_index(
            &self.state,
            &self.sources,
            &self.transcript,
            self.limits,
        )?;
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
            LEGACY_CHECKPOINT_SCHEMA_VERSION | CHECKPOINT_SCHEMA_VERSION
        ) || manifest.scope != self.store.scope_digest().into_bytes()
            || manifest.generation != self.store.generation()
            || manifest.through_event >= self.store.sequence()
        {
            return Err(error("checkpoint binding or publication mismatch"));
        }
        checkpoint_validation::validate_schema_lineage(manifest, |digest| {
            self.store.read_digest(digest)
        })?;
        self.state = decode_working_state(
            &self.store.read(manifest.working_state)?,
            self.binding,
            self.limits,
        )
        .map_err(|_| error("invalid working checkpoint"))?;
        self.sources = decode(&self.store.read(manifest.source_index)?)?;
        self.transcript = decode(&self.store.read(manifest.transcript_manifest)?)?;
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
        view_binding::verify(manifest, &view_bytes, &validation)?;
        self.local_compactor_failures = validation.local_compactor_failures;
        self.retrieval_calls = validation.retrieval_calls;
        self.model_revision = validation.model_revision;
        Ok(())
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
                checkpoint_validation::validate_transcript(
                    &self.state,
                    &self.sources,
                    &manifest,
                    self.transcript.invocation,
                    self.limits,
                )?;
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
}
