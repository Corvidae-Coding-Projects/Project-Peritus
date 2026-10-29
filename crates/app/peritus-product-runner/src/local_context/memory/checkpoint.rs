//! Persist exact prepared view and reconstruction artifacts before atomic publication.

use super::super::{
    checkpoint_validation, error,
    record::{CheckpointManifest, MemoryRecord, encode},
    view_binding,
};
use super::LocalMemory;
use peritus_agent::{DeveloperLoopError, estimate_developer_request_tokens};
use peritus_codec::sha256;
use peritus_model_protocol::{Message, ProtocolLimits, encode_messages};
use serde_json::Value;

impl LocalMemory {
    pub(in crate::local_context) fn publish(
        &mut self,
        messages: &[Message],
    ) -> Result<(), DeveloperLoopError> {
        let started = std::time::Instant::now();
        let previous_work = self.store.work();
        let previous_pages = self.page_cache.work();
        let previous_generation = self.store.generation();
        let result = self.publish_inner(messages);
        crate::efficiency::event(
            &self.trace_path,
            serde_json::json!({
                "event": "context-checkpoint-work",
                "generation_before": previous_generation,
                "generation": self.store.generation(),
                "committed": self.store.generation() != previous_generation,
                "completed": result.is_ok(),
                "elapsed_micros": started.elapsed().as_micros(),
                "work": self.store.work().since(previous_work),
                "pages": self.page_cache.work().since(previous_pages),
                "semantics": "per-publication attempt, including failures; completed byte counters exclude partial failed I/O; not physical filesystem or provider counters"
            }),
        );
        result
    }

    fn validate_prepared(&self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        let prepared =
            self.prepared.as_ref().ok_or_else(|| error("no prepared view to publish"))?;
        let profile = self.profile.as_ref().ok_or_else(|| error("prepared view lacks profile"))?;
        let tool_policy = view_binding::tool_policy(&self.tools)?;
        let render_policy = sha256(&encode(&self.config)?).into_bytes();
        if prepared.messages != messages
            || prepared.validation.state_revision != self.state.revision()
            || prepared.through_event != self.store.sequence()
            || prepared.validation.model_revision != self.model_revision
            || prepared.validation.profile != profile.profile_id().into_bytes()
            || prepared.validation.profile_revision != profile.revision()
            || prepared.validation.max_input_tokens != profile.limits().max_input_tokens()
            || prepared.validation.estimated_input_tokens
                != estimate_developer_request_tokens(messages, &self.tools)
            || prepared.validation.local_compactor_failures != self.local_compactor_failures
            || prepared.validation.retrieval_calls != self.retrieval_calls
            || prepared.validation.tool_policy != Some(tool_policy)
            || prepared.policy.into_bytes() != render_policy
        {
            return Err(error("stale or changed prepared view"));
        }
        checkpoint_validation::validate_checkpoint(
            self.checkpoint_schema,
            &self.state,
            &self.sources,
            &self.transcript,
            &prepared.validation,
            self.limits,
        )?;
        Ok(())
    }

    fn publish_inner(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.validate_prepared(messages)?;
        let dependencies = if self.host_dependencies_enabled() && self.reader_guard_target.is_none()
        {
            Some(super::PublishedDependencies {
                validity: super::super::assembly::decision::request_dependencies(self)?,
                through: self.state.through_observation(),
                state_revision: self.state.revision(),
                model_revision: self.model_revision,
                view: sha256(&encode_messages(messages, ProtocolLimits::PRODUCTION)?),
            })
        } else {
            None
        };
        let prepared =
            self.prepared.as_ref().ok_or_else(|| error("no prepared view to publish"))?;
        let render_policy = prepared.policy.into_bytes();
        let (working_state, source_index, mut roots) = super::super::pages::store(
            &self.store,
            &self.state,
            &self.sources,
            &mut self.page_cache,
            self.config.reuse_checkpoint_pages,
        )?;
        let transcript_manifest = self.store.store(&encode(&self.transcript)?)?;
        let view_bytes = encode_messages(messages, ProtocolLimits::PRODUCTION)?;
        let view = self.store.store(&view_bytes)?;
        let validation = self.store.store(&encode(&prepared.validation)?)?;
        let previous = self
            .last_checkpoint
            .as_ref()
            .map(|manifest| encode(manifest).map(|bytes| sha256(&bytes).into_bytes()))
            .transpose()?;
        let generation =
            self.store.generation().checked_add(1).ok_or_else(|| error("generation overflow"))?;
        let scope = self.store.scope_digest().into_bytes();
        let view_binding = view_binding::checkpoint(
            self.checkpoint_schema,
            scope,
            generation,
            prepared.through_event,
            render_policy,
            &view_bytes,
            &prepared.validation,
        )?;
        let manifest = CheckpointManifest {
            schema_version: self.checkpoint_schema,
            trace_cursor: self.trace_cursor.clone(),
            scope,
            generation,
            previous,
            through_event: prepared.through_event,
            working_state,
            transcript_manifest,
            source_index,
            view,
            render_policy,
            validation,
            view_binding: Some(view_binding),
        };
        let bytes = encode(&manifest)?;
        let artifact = self.store.store(&bytes)?;
        let event = encode(&MemoryRecord::Checkpoint { manifest: artifact })?;
        roots.extend([
            working_state.digest,
            transcript_manifest.digest,
            source_index.digest,
            view.digest,
            validation.digest,
            artifact.digest,
        ]);
        self.store.append(&event, &roots, Some((self.store.generation(), bytes)))?;
        let trace_payload = serde_json::to_vec(&Value::from_iter([
            ("schema_version", Value::from(manifest.schema_version)),
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
        self.published_dependencies = dependencies;
        self.last_view = messages.to_vec();
        self.prepared = None;
        crate::trace::local_memory::checkpoint(&self.trace_path, &trace_payload)?;
        Ok(())
    }
}
