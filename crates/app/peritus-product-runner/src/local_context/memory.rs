//! Durable state owner shared by the loop port and its bounded memory-tool decorator.

mod checkpoint;
pub(super) mod environment;
mod ingestion;
mod recovery;

use super::{
    LocalContextConfig, error,
    record::{
        ArchivedObservation, CheckpointManifest, MemoryRecord, TranscriptManifest, ViewValidation,
        encode,
    },
    storage::LocalStore,
};
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{
    WorkingBinding, WorkingEvent, WorkingLimits, WorkingState, apply_working_event,
    encode_working_event,
};
use peritus_model_protocol::{Message, ProviderProfile, ToolDefinition};
use peritus_types::Sha256Digest;
use std::path::{Path, PathBuf};

pub(super) struct LocalMemory {
    pub(super) store: LocalStore,
    pub(super) state: WorkingState,
    pub(super) sources: Vec<ArchivedObservation>,
    pub(super) transcript: TranscriptManifest,
    pub(super) config: LocalContextConfig,
    pub(super) workspace: PathBuf,
    pub(super) trace_path: PathBuf,
    pub(super) binding: WorkingBinding,
    pub(super) limits: WorkingLimits,
    pub(super) last_checkpoint: Option<CheckpointManifest>,
    pub(super) last_view: Vec<Message>,
    pub(super) prepared: Option<PreparedView>,
    pub(super) profile: Option<ProviderProfile>,
    pub(super) tools: Vec<ToolDefinition>,
    pub(super) local_compactor_failures: u64,
    pub(super) retrieval_calls: u64,
    pub(super) model_revision: u64,
    pub(super) compactor_runtime: Option<crate::CommandRuntime>,
    pub(super) task_contract: String,
    pub(super) workspace_scope: environment::WorkspaceScope,
}

pub(super) struct PreparedView {
    pub(super) messages: Vec<Message>,
    pub(super) validation: ViewValidation,
    pub(super) through_event: u64,
    pub(super) policy: Sha256Digest,
}

impl LocalMemory {
    pub(super) fn load(
        root: &Path,
        workspace: &Path,
        trace_path: &Path,
        binding: WorkingBinding,
        config: LocalContextConfig,
    ) -> Result<Self, DeveloperLoopError> {
        Self::load_scoped(
            root,
            workspace,
            trace_path,
            binding,
            config,
            environment::WorkspaceScope::default(),
        )
    }

    pub(super) fn load_scoped(
        root: &Path,
        workspace: &Path,
        trace_path: &Path,
        binding: WorkingBinding,
        config: LocalContextConfig,
        workspace_scope: environment::WorkspaceScope,
    ) -> Result<Self, DeveloperLoopError> {
        config.validate().map_err(|_| error("invalid local context configuration"))?;
        let limits = config.working_limits().map_err(|_| error("invalid local context limits"))?;
        let store = if workspace_scope.direct {
            LocalStore::open_folder(root, workspace, binding, &workspace_scope.protected)?
        } else {
            LocalStore::open(root, workspace, binding)?
        };
        let environment =
            environment::capture(workspace, binding, &[], "", limits, &workspace_scope)?;
        let state =
            WorkingState::new(environment, limits).map_err(|_| error("create working state"))?;
        let mut memory = Self {
            store,
            state,
            sources: Vec::new(),
            transcript: TranscriptManifest::default(),
            config,
            workspace: workspace.to_path_buf(),
            trace_path: trace_path.to_path_buf(),
            binding,
            limits,
            last_checkpoint: None,
            last_view: Vec::new(),
            prepared: None,
            profile: None,
            tools: Vec::new(),
            local_compactor_failures: 0,
            retrieval_calls: 0,
            model_revision: 0,
            compactor_runtime: None,
            task_contract: String::new(),
            workspace_scope,
        };
        memory.recover()?;
        let trace_path = memory.trace_path.clone();
        let torn = crate::trace::local_memory::observations(
            &trace_path,
            memory.store.scope_digest(),
            |invocation, tool_sequence, call, observation| {
                memory.observe_tool_in(invocation, tool_sequence, &call, &observation).map(|_| ())
            },
        )?;
        if torn {
            return Err(error(
                "torn trace tail: valid observations recovered; trace repair required before new work",
            ));
        }
        Ok(memory)
    }

    pub(super) fn state_event(&mut self, event: &WorkingEvent) -> Result<(), DeveloperLoopError> {
        let next = apply_working_event(&self.state, event)
            .map_err(|_| error("working-state event rejected"))?;
        let model_revision = self.next_model_revision(event)?;
        let reducer = self
            .store
            .store(&encode_working_event(event).map_err(|_| error("encode working event"))?)?;
        self.commit(&MemoryRecord::StateEvent { reducer }, &[reducer.digest])?;
        self.state = next;
        self.model_revision = model_revision;
        self.prepared = None;
        Ok(())
    }

    pub(super) fn next_model_revision(
        &self,
        event: &WorkingEvent,
    ) -> Result<u64, DeveloperLoopError> {
        if matches!(event, WorkingEvent::Delta(_) | WorkingEvent::Refresh { .. }) {
            self.model_revision.checked_add(1).ok_or_else(|| error("model revision overflow"))
        } else {
            Ok(self.model_revision)
        }
    }

    pub(super) fn commit(
        &mut self,
        record: &MemoryRecord,
        roots: &[Sha256Digest],
    ) -> Result<(), DeveloperLoopError> {
        self.store.append(&encode(record)?, roots, None)
    }

    pub(super) fn archived(
        &self,
        sequence: u64,
    ) -> Result<&ArchivedObservation, DeveloperLoopError> {
        let index = usize::try_from(
            sequence.checked_sub(1).ok_or_else(|| error("invalid observation handle"))?,
        )
        .map_err(|_| error("observation handle overflow"))?;
        self.sources
            .get(index)
            .filter(|source| source.sequence == sequence)
            .ok_or_else(|| error("observation is unavailable"))
    }

    pub(super) fn artifact(&self, sequence: u64) -> Result<Vec<u8>, DeveloperLoopError> {
        let record = self.archived(sequence)?;
        self.store.read(record.artifact)
    }
}
