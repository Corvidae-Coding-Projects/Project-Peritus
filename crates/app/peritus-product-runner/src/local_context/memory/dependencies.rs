//! Durable host dependency initialization, originating-view capture and reader-floor publication.

use super::LocalMemory;
use crate::local_context::{
    error,
    record::{DEPENDENCY_CHECKPOINT_SCHEMA_VERSION, LEGACY_DEPENDENCY_CHECKPOINT_SCHEMA_VERSION},
};
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{
    WorkingEntryStatus, WorkingEvent, WorkingHostBinding, WorkingValidity,
};
use peritus_model_protocol::{ModelRequest, ProtocolLimits, encode_messages};

pub(in crate::local_context) struct HostRequestBinding {
    pub(in crate::local_context) validity: WorkingValidity,
    pub(in crate::local_context) sources: WorkingHostBinding,
    pub(in crate::local_context) model_revision: u64,
}

pub(in crate::local_context) struct PublishedDependencies {
    pub(in crate::local_context) validity: WorkingValidity,
    pub(in crate::local_context) through: u64,
    pub(in crate::local_context) state_revision: u64,
    pub(in crate::local_context) model_revision: u64,
    pub(in crate::local_context) view: peritus_types::Sha256Digest,
}

impl LocalMemory {
    pub(in crate::local_context) fn prepare_host_dependencies(
        &mut self,
    ) -> Result<(), DeveloperLoopError> {
        if self.host_dependencies_enabled()
            && self.reader_guard_target.is_none()
            && (self.checkpoint_schema >= LEGACY_DEPENDENCY_CHECKPOINT_SCHEMA_VERSION
                || self.has_unproven_model_entries()?)
        {
            let upgrade = self.checkpoint_schema == LEGACY_DEPENDENCY_CHECKPOINT_SCHEMA_VERSION;
            self.ensure_host_dependencies()?;
            if upgrade {
                self.ingest_facts()?;
            }
        }
        Ok(())
    }
    pub(in crate::local_context) fn host_dependencies_enabled(&self) -> bool {
        self.config.decision_view == crate::DecisionViewPolicy::Conservative
            || self.checkpoint_schema >= LEGACY_DEPENDENCY_CHECKPOINT_SCHEMA_VERSION
    }

    pub(in crate::local_context) fn has_unproven_model_entries(
        &self,
    ) -> Result<bool, DeveloperLoopError> {
        for entry in self
            .state
            .entries(self.state.binding())
            .map_err(|_| error("host dependency entry scope"))?
        {
            if !matches!(entry.status(), WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded)
                && !self.is_mechanical_entry(entry)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(in crate::local_context) fn ensure_reader_guard(
        &mut self,
        minimum: u16,
    ) -> Result<(), DeveloperLoopError> {
        if self
            .last_checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.schema_version >= minimum)
        {
            return Ok(());
        }
        if self.reader_guard_target.is_some() {
            return Err(error("recursive context reader guard publication"));
        }
        let profile = self
            .profile
            .clone()
            .ok_or_else(|| error("reader guard requires a prepared request"))?;
        let tools = self.tools.clone();
        self.checkpoint_schema = self.checkpoint_schema.max(minimum);
        self.reader_guard_target = Some(minimum);
        let result = (|| {
            let messages = self.prepare_view(&profile, &tools)?;
            self.publish(&messages)
        })();
        self.reader_guard_target = None;
        result
    }

    pub(in crate::local_context) fn ensure_host_dependencies(
        &mut self,
    ) -> Result<(), DeveloperLoopError> {
        if !self.host_dependencies_enabled() || self.reader_guard_target.is_some() {
            return Ok(());
        }
        self.ensure_reader_guard(DEPENDENCY_CHECKPOINT_SCHEMA_VERSION)?;
        if let Some(version) = self.transcript.host_dependency_version {
            match version {
                2 => return Ok(()),
                1 => {} // Unqualified v5 bindings must be conservatively migrated.
                _ => return Err(error("unsupported host dependency version")),
            }
        }
        // The current floor is committed before any new host event. An interrupted initialization
        // is completed on reopen even when the caller has selected history compilation.
        loop {
            let mut ids = Vec::new();
            for entry in self
                .state
                .entries(self.state.binding())
                .map_err(|_| error("host dependency entry scope"))?
            {
                if !matches!(
                    entry.status(),
                    WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded
                ) && !self.is_mechanical_entry(entry)?
                {
                    ids.push(entry.id());
                }
                if ids.len() == self.limits.operations() {
                    break;
                }
            }
            if ids.is_empty() {
                break;
            }
            self.state_event(&WorkingEvent::HostInvalidate {
                binding: self.state.binding(),
                base_revision: self.state.revision(),
                entries: ids,
            })?;
        }
        self.transcript.host_dependency_version = Some(2);
        self.persist_transcript()
    }

    pub(in crate::local_context) fn validate_host_event(
        &self,
        event: &WorkingEvent,
        replay: bool,
    ) -> Result<(), DeveloperLoopError> {
        if !matches!(event, WorkingEvent::HostDelta { .. } | WorkingEvent::HostInvalidate { .. }) {
            return Ok(());
        }
        let minimum = if replay {
            LEGACY_DEPENDENCY_CHECKPOINT_SCHEMA_VERSION
        } else {
            DEPENDENCY_CHECKPOINT_SCHEMA_VERSION
        };
        if self
            .last_checkpoint
            .as_ref()
            .is_none_or(|checkpoint| checkpoint.schema_version < minimum)
        {
            return Err(error("host dependency event requires its committed reader guard"));
        }
        let required = if replay && self.checkpoint_schema < DEPENDENCY_CHECKPOINT_SCHEMA_VERSION {
            1
        } else {
            2
        };
        if matches!(event, WorkingEvent::HostDelta { .. })
            && self.transcript.host_dependency_version != Some(required)
        {
            return Err(error("host dependency initialization is incomplete"));
        }
        Ok(())
    }

    pub(in crate::local_context) fn bind_host_dependencies(
        &mut self,
        request: &ModelRequest,
    ) -> Result<(), DeveloperLoopError> {
        let encoded = encode_messages(request.messages(), ProtocolLimits::PRODUCTION)?;
        let snapshot = self.published_dependencies.as_ref().filter(|snapshot| {
            snapshot.view == peritus_codec::sha256(&encoded)
                && snapshot.state_revision == self.state.revision()
                && snapshot.model_revision == self.model_revision
        });
        let exact = snapshot.is_some();
        let validity =
            snapshot.map_or_else(WorkingValidity::uncertain, |snapshot| snapshot.validity.clone());
        let through =
            snapshot.map_or_else(|| self.state.through_observation(), |snapshot| snapshot.through);
        let model_revision =
            snapshot.map_or(self.model_revision, |snapshot| snapshot.model_revision);
        let request = if exact { Some(request.fingerprint()?.digest()) } else { None };
        self.decision_dependencies = Some(HostRequestBinding {
            validity,
            sources: WorkingHostBinding::new(through, request),
            model_revision,
        });
        Ok(())
    }
}
