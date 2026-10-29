//! Durable state owner shared by the loop port and its bounded memory-tool decorator.

mod checkpoint;
mod decoded;
mod dependencies;
pub(super) mod environment;
mod ingestion;
pub(in crate::local_context) use dependencies::{HostRequestBinding, PublishedDependencies};
pub(in crate::local_context) use ingestion::call_identity;
mod recovery;
mod source_index;
pub(in crate::local_context) use source_index::SourceProjectionWork;

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
    pub(super) archive_bytes: Option<u64>,
    pub(super) indexed_sources: bool,
    pub(super) source_work: std::cell::Cell<SourceProjectionWork>,
    tool_sources: std::collections::BTreeMap<(u64, u64), usize>,
    latest_tool_sequence: std::collections::BTreeMap<u64, u64>,
    pub(super) page_cache: super::pages::PageCache,
    pub(super) checkpoint_schema: u16,
    pub(super) transcript: TranscriptManifest,
    pub(super) config: LocalContextConfig,
    pub(super) workspace: PathBuf,
    pub(super) trace_path: PathBuf,
    pub(super) inspection_targets: std::collections::BTreeMap<String, String>,
    pub(super) inspection_indexed_through: u64,
    pub(super) trace_cursor: Option<crate::trace::local_memory::TraceCursor>,
    pub(super) binding: WorkingBinding,
    pub(super) limits: WorkingLimits,
    pub(super) last_checkpoint: Option<CheckpointManifest>,
    pub(super) last_view: Vec<Message>,
    pub(super) inline_epoch_start: Option<u64>,
    pub(super) exchange_index: super::assembly::exchanges::ExchangeIndex,
    pub(super) decoded_hits: std::cell::Cell<u64>,
    pub(super) decoded_misses: std::cell::Cell<u64>,
    pub(super) archive_lookups: std::cell::Cell<u64>,
    pub(super) measure_work: bool,
    pub(super) prepared: Option<PreparedView>,
    pub(super) profile: Option<ProviderProfile>,
    pub(super) tools: Vec<ToolDefinition>,
    pub(super) local_compactor_failures: u64,
    pub(super) semantic_through_observation: u64,
    pub(super) retrieval_calls: u64,
    pub(super) model_revision: u64,
    pub(super) compactor_runtime: Option<crate::CommandRuntime>,
    pub(super) task_contract: String,
    pub(super) conversation: Option<std::sync::Arc<dyn crate::ConversationView>>,
    pub(super) cancellation: Option<(
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        peritus_provider_core::CancellationToken,
    )>,
    pub(super) workspace_scope: environment::WorkspaceScope,
    pub(super) activity: super::activity::Activity,
    pub(super) source_policy:
        std::cell::RefCell<std::collections::BTreeMap<u64, super::source_policy::Descriptor>>,
    pub(super) continuation_actions: std::cell::RefCell<std::collections::BTreeMap<u64, String>>,
    pub(super) read_reuse: super::read_reuse::ReadReuse,
    pub(super) decision_index: super::assembly::decision::Index,
    pub(super) decision_files: [u8; 32],
    pub(super) decision_dependencies: Option<HostRequestBinding>,
    pub(super) published_dependencies: Option<PublishedDependencies>,
    pub(super) reader_guard_target: Option<u16>,
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
        let limits = LocalContextConfig::working_limits();
        let mut store = if workspace_scope.direct {
            LocalStore::open_folder(root, workspace, binding, &workspace_scope.protected)?
        } else {
            LocalStore::open(root, workspace, binding)?
        };
        store.set_artifact_reuse(config.reuse_checkpoint_pages);
        let environment =
            environment::capture(workspace, binding, &[], "", limits, &workspace_scope)?;
        let state =
            WorkingState::new(environment, limits).map_err(|_| error("create working state"))?;
        let mut memory = Self {
            store,
            state,
            sources: Vec::new(),
            archive_bytes: Some(0),
            indexed_sources: std::env::var_os("PERITUS_CONTEXT_SOURCE_INDEX")
                .is_none_or(|value| value != "reference"),
            source_work: std::cell::Cell::default(),
            tool_sources: std::collections::BTreeMap::new(),
            latest_tool_sequence: std::collections::BTreeMap::new(),
            page_cache: super::pages::PageCache::default(),
            checkpoint_schema: super::record::CHECKPOINT_SCHEMA_VERSION,
            transcript: TranscriptManifest::default(),
            config,
            workspace: workspace.to_path_buf(),
            trace_path: trace_path.to_path_buf(),
            inspection_targets: std::collections::BTreeMap::new(),
            inspection_indexed_through: 0,
            trace_cursor: None,
            binding,
            limits,
            last_checkpoint: None,
            last_view: Vec::new(),
            inline_epoch_start: None,
            exchange_index: super::assembly::exchanges::ExchangeIndex::default(),
            decoded_hits: std::cell::Cell::default(),
            decoded_misses: std::cell::Cell::default(),
            archive_lookups: std::cell::Cell::default(),
            measure_work: peritus_provider_core::efficiency_diagnostics_enabled(),
            prepared: None,
            profile: None,
            tools: Vec::new(),
            local_compactor_failures: 0,
            semantic_through_observation: 0,
            retrieval_calls: 0,
            model_revision: 0,
            compactor_runtime: None,
            task_contract: String::new(),
            conversation: None,
            cancellation: None,
            workspace_scope,
            read_reuse: super::read_reuse::ReadReuse::default(),
            decision_index: super::assembly::decision::Index::default(),
            decision_files: [0; 32],
            decision_dependencies: None,
            published_dependencies: None,
            reader_guard_target: None,
            activity: super::activity::Activity::default(),
            continuation_actions: std::cell::RefCell::default(),
            source_policy: std::cell::RefCell::default(),
        };
        memory.recover()?;
        if memory.checkpoint_schema >= super::record::DEPENDENCY_CHECKPOINT_SCHEMA_VERSION {
            memory.ensure_host_dependencies()?;
        }
        let trace_path = memory.trace_path.clone();
        let cursor = memory.trace_cursor.clone();
        let (torn, recovered_cursor) = crate::trace::local_memory::observations(
            &trace_path,
            memory.store.scope_digest(),
            cursor.as_ref(),
            |invocation, tool_sequence, call, observation| {
                memory.observe_tool_in(invocation, tool_sequence, &call, &observation).map(|_| ())
            },
        )?;
        memory.trace_cursor = recovered_cursor;
        if torn {
            return Err(error(
                "torn trace tail: valid observations recovered; trace repair required before new work",
            ));
        }
        Ok(memory)
    }

    pub(super) fn state_event(&mut self, event: &WorkingEvent) -> Result<(), DeveloperLoopError> {
        self.validate_host_event(event, false)?;
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
        if matches!(
            event,
            WorkingEvent::Delta(_)
                | WorkingEvent::Refresh { .. }
                | WorkingEvent::HostDelta { .. }
                | WorkingEvent::HostInvalidate { .. }
        ) {
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

    pub(super) fn append_source(&mut self, source: ArchivedObservation) {
        self.archive_bytes =
            self.archive_bytes.and_then(|total| total.checked_add(source.artifact.bytes));
        if let Some(sequence) = source.tool_sequence {
            self.tool_sources.entry((source.invocation, sequence)).or_insert(self.sources.len());
            if source.kind == super::record::ArchiveKind::ToolOutput {
                self.latest_tool_sequence.insert(source.invocation, sequence);
            }
        }
        self.sources.push(source);
    }

    fn rebuild_source_indexes(&mut self) {
        self.archive_bytes = Some(0);
        self.tool_sources.clear();
        self.latest_tool_sequence.clear();
        for (index, source) in self.sources.iter().enumerate() {
            self.archive_bytes =
                self.archive_bytes.and_then(|total| total.checked_add(source.artifact.bytes));
            if let Some(sequence) = source.tool_sequence {
                self.tool_sources.entry((source.invocation, sequence)).or_insert(index);
                if source.kind == super::record::ArchiveKind::ToolOutput {
                    self.latest_tool_sequence.insert(source.invocation, sequence);
                }
            }
        }
    }

    pub(super) fn artifact(&self, sequence: u64) -> Result<Vec<u8>, DeveloperLoopError> {
        let record = self.archived(sequence)?;
        self.store.read(record.artifact)
    }
}
