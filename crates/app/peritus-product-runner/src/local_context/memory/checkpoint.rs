//! Persist exact prepared view and reconstruction artifacts before atomic publication.

use super::super::{
    error,
    record::{CheckpointManifest, MemoryRecord, encode},
};
use super::LocalMemory;
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_context::working::encode_working_state;
use peritus_model_protocol::{Message, ProtocolLimits, encode_messages};
use serde_json::Value;

impl LocalMemory {
    pub(in crate::local_context) fn publish(
        &mut self,
        messages: &[Message],
    ) -> Result<(), DeveloperLoopError> {
        let prepared =
            self.prepared.as_ref().ok_or_else(|| error("no prepared view to publish"))?;
        if prepared.messages != messages
            || prepared.validation.state_revision != self.state.revision()
            || prepared.through_event != self.store.sequence()
        {
            return Err(error("stale or changed prepared view"));
        }
        let working_state = self.store.store(
            &encode_working_state(&self.state).map_err(|_| error("encode working checkpoint"))?,
        )?;
        let transcript_manifest = self.store.store(&encode(&self.transcript)?)?;
        let source_index = self.store.store(&encode(&self.sources)?)?;
        let view = self.store.store(&encode_messages(messages, ProtocolLimits::PRODUCTION)?)?;
        let validation = self.store.store(&encode(&prepared.validation)?)?;
        let previous = self
            .last_checkpoint
            .as_ref()
            .map(|manifest| encode(manifest).map(|bytes| sha256(&bytes).into_bytes()))
            .transpose()?;
        let manifest = CheckpointManifest {
            schema_version: 1,
            scope: self.store.scope_digest().into_bytes(),
            generation: self
                .store
                .generation()
                .checked_add(1)
                .ok_or_else(|| error("generation overflow"))?,
            previous,
            through_event: prepared.through_event,
            working_state,
            transcript_manifest,
            source_index,
            view,
            render_policy: prepared.policy.into_bytes(),
            validation,
        };
        let bytes = encode(&manifest)?;
        let artifact = self.store.store(&bytes)?;
        let event = encode(&MemoryRecord::Checkpoint { manifest: artifact })?;
        let roots = [
            working_state.digest,
            transcript_manifest.digest,
            source_index.digest,
            view.digest,
            validation.digest,
            artifact.digest,
        ];
        self.store.append(&event, &roots, Some((self.store.generation(), bytes)))?;
        let trace_payload = serde_json::to_vec(&Value::from_iter([
            ("schema_version", Value::from(1)),
            ("scope", Value::from(manifest.scope)),
            ("generation", Value::from(manifest.generation)),
            ("manifest_sha256", Value::from(artifact.digest.into_bytes())),
            ("manifest_bytes", Value::from(artifact.bytes)),
            ("view_sha256", Value::from(view.digest.into_bytes())),
            ("state_revision", Value::from(prepared.validation.state_revision)),
            ("estimated_input_tokens", Value::from(prepared.validation.estimated_input_tokens)),
            (
                "validation",
                serde_json::to_value(&prepared.validation)
                    .map_err(|_| error("encode checkpoint validation"))?,
            ),
        ]))
        .map_err(|_| error("encode checkpoint trace"))?;
        self.last_checkpoint = Some(manifest);
        self.last_view = messages.to_vec();
        self.prepared = None;
        crate::trace::local_memory::checkpoint(&self.trace_path, &trace_payload)?;
        Ok(())
    }
}
