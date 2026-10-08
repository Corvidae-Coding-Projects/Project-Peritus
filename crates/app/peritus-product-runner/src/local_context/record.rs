//! Version-one host journal payloads and rebuildable transcript/source projections.

use super::{error, storage::StoredArtifact};
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{ObservationKind, ObservationSource};
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub(super) const LEGACY_CHECKPOINT_SCHEMA_VERSION: u16 = 1;
pub(super) const SNAPSHOT_CHECKPOINT_SCHEMA_VERSION: u16 = 2;
pub(super) const INDEXED_CHECKPOINT_SCHEMA_VERSION: u16 = 3;
pub(super) const PAGED_CHECKPOINT_SCHEMA_VERSION: u16 = 4;
pub(super) const CHECKPOINT_SCHEMA_VERSION: u16 = 5;
pub(super) const INDEX_PAGE_SCHEMA_VERSION: u16 = 1;
pub(super) const HOST_INDEX_SCHEMA_VERSION: u16 = 1;
pub(super) const LEGACY_CONTEXT_UPDATE_SCHEMA_VERSION: u16 = 1;
pub(super) const TRANSCRIPT_CONTEXT_UPDATE_SCHEMA_VERSION: u16 = 2;
pub(super) const CONTEXT_UPDATE_SCHEMA_VERSION: u16 = 3;
pub(super) const CONTEXT_UPDATE_PAGE_SCHEMA_VERSION: u16 = 1;
pub(super) const CONTEXT_UPDATE_FILE_PAGE_SCHEMA_VERSION: u16 = 2;
pub(super) const CONTEXT_UPDATE_REDUCER_SCHEMA_VERSION: u16 = 1;
pub(super) const CONTEXT_UPDATE_ENTRY_PAGE_SCHEMA_VERSION: u16 = 1;
pub(super) const PAGED_GENESIS_SCHEMA_VERSION: u16 = 2;
pub(super) const LEGACY_SEGMENT_CONTINUATION_SCHEMA_VERSION: u16 = 1;
pub(super) const SEGMENT_CONTINUATION_SCHEMA_VERSION: u16 = 2;
pub(super) const WORKING_SELECTION_FRONTIER_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ArchiveKind {
    Policy,
    User,
    Assistant,
    ToolMessage,
    ToolOutput,
}

impl ArchiveKind {
    pub(super) const fn source_kind(self) -> ObservationKind {
        match self {
            Self::Policy => ObservationKind::HostPolicy,
            Self::User => ObservationKind::UserInstruction,
            Self::Assistant => ObservationKind::AgentMessage,
            Self::ToolMessage | Self::ToolOutput => ObservationKind::ToolOutput,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct CallIdentity {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) arguments_digest: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ArchivedObservation {
    pub(super) sequence: u64,
    pub(super) invocation: u64,
    pub(super) tool_sequence: Option<u64>,
    pub(super) kind: ArchiveKind,
    pub(super) artifact: StoredArtifact,
    pub(super) call: Option<CallIdentity>,
    pub(super) is_error: bool,
}

impl ArchivedObservation {
    pub(super) fn validate_locator(
        &self,
        locator: ObservationSource,
    ) -> Result<(), DeveloperLoopError> {
        if locator.id().get() != self.sequence
            || locator.kind() != self.kind.source_kind()
            || locator.artifact() != self.artifact.digest
            || locator.artifact_bytes() != self.artifact.bytes
            || locator.start() != 0
            || locator.end() != self.artifact.bytes
            || self.invocation == 0
        {
            return Err(error("source locator origin or exact artifact mismatch"));
        }
        match (&self.call, self.tool_sequence, self.kind) {
            (Some(call), Some(sequence), ArchiveKind::ToolOutput)
                if sequence > 0
                    && !call.id.is_empty()
                    && !call.name.is_empty() =>
            {
                Ok(())
            }
            (None, None, kind) if kind != ArchiveKind::ToolOutput && !self.is_error => Ok(()),
            _ => Err(error("invalid source call or tool sequence metadata")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub(super) enum MemoryRecord {
    Genesis { state: StoredArtifact },
    GenesisRoot {
        schema_version: u16,
        state: StoredArtifact,
        source_index: StoredArtifact,
    },
    Invocation { sequence: u64, request_prefix: String },
    InvocationCompleted {
        schema_version: u16,
        invocation: u64,
        request_prefix: String,
        segment_sequence: u64,
    },
    ToolEffectUncertain {
        invocation: u64,
        request_prefix: String,
        tool_sequence: u64,
        call: CallIdentity,
    },
    Observation { observation: ArchivedObservation, reducer: StoredArtifact },
    StateEvent { reducer: StoredArtifact },
    ContextUpdate(ContextUpdateRecord),
    Transcript { manifest: TranscriptManifest },
    TranscriptIndex { transcript: StoredArtifact },
    CheckpointIndex { source: StoredArtifact, transcript: StoredArtifact },
    HostIndex { root: StoredArtifact },
    CheckpointHostIndex { source: StoredArtifact, root: StoredArtifact },
    Checkpoint { manifest: StoredArtifact },
    Compactor { input: Option<StoredArtifact>, output: Option<StoredArtifact>, failed: bool },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub(super) enum ContextUpdateRecord {
    Inline(InlineContextUpdate),
    Root(RootContextUpdate),
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct InlineContextUpdate {
    pub(super) schema_version: u16,
    pub(super) base_model_revision: u64,
    pub(super) reducers: Vec<StoredArtifact>,
    pub(super) transcript: TranscriptManifest,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct RootContextUpdate {
    pub(super) schema_version: u16,
    pub(super) root: StoredArtifact,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ContextUpdateRoot {
    pub(super) schema_version: u16,
    pub(super) base_model_revision: u64,
    pub(super) source_index: StoredArtifact,
    pub(super) reducer_head: StoredArtifact,
    pub(super) reducer_count: u64,
    pub(super) transcript_before: [u8; 32],
    pub(super) transcript_after: [u8; 32],
    pub(super) transcript_head: Option<StoredArtifact>,
    pub(super) transcript_change_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ContextUpdateReducerPage {
    pub(super) schema_version: u16,
    pub(super) next: Option<StoredArtifact>,
    pub(super) first_reducer: u64,
    pub(super) reducers: Vec<StoredArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub(super) enum ContextUpdateReducer {
    Event {
        schema_version: u16,
        event: StoredArtifact,
    },
    Refresh {
        schema_version: u16,
        base_revision: u64,
        state: StoredArtifact,
    },
    DeltaSnapshot {
        schema_version: u16,
        base_revision: u64,
        state: StoredArtifact,
        entry_head: StoredArtifact,
        entry_count: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ContextUpdateEntryPage {
    pub(super) schema_version: u16,
    pub(super) next: Option<StoredArtifact>,
    pub(super) first_entry: u64,
    pub(super) entries: Vec<ContextUpdateEntryIdentity>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ContextUpdateEntryIdentity {
    pub(super) id: [u8; 16],
    pub(super) status: ContextUpdateEntryStatus,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ContextUpdateEntryStatus {
    Open,
    Contradicted,
    Resolved,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ContextUpdateTranscriptPage {
    pub(super) schema_version: u16,
    pub(super) next: Option<StoredArtifact>,
    pub(super) first_change: u64,
    pub(super) before: [u8; 32],
    pub(super) after: [u8; 32],
    pub(super) files_removed: Vec<String>,
    pub(super) files_added: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceIndexPage {
    pub(super) schema_version: u16,
    pub(super) previous: Option<StoredArtifact>,
    pub(super) first_sequence: u64,
    pub(super) observations: Vec<ArchivedObservation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct TranscriptDeltaPage {
    pub(super) schema_version: u16,
    pub(super) previous: Option<StoredArtifact>,
    pub(super) before: [u8; 32],
    pub(super) after: [u8; 32],
    pub(super) invocation: u64,
    pub(super) request_prefix: String,
    pub(super) reset_messages: bool,
    pub(super) messages: Vec<u64>,
    pub(super) current_inputs: Vec<u64>,
    pub(super) pending_upserts: Vec<PendingDescriptor>,
    pub(super) pending_removed: Vec<[u8; 16]>,
    pub(super) files_added: Vec<String>,
    pub(super) files_removed: Vec<String>,
    pub(super) facts_through: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingIndexPage {
    pub(super) schema_version: u16,
    pub(super) previous: Option<StoredArtifact>,
    pub(super) before: [u8; 32],
    pub(super) after: [u8; 32],
    pub(super) upserts: Vec<PendingDescriptor>,
    pub(super) removed: Vec<[u8; 16]>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ObservedFileIndexPage {
    pub(super) schema_version: u16,
    pub(super) previous: Option<StoredArtifact>,
    pub(super) before: [u8; 32],
    pub(super) after: [u8; 32],
    pub(super) added: Vec<String>,
    pub(super) removed: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct HostIndexRoot {
    pub(super) schema_version: u16,
    pub(super) invocation: u64,
    pub(super) request_prefix: String,
    pub(super) transcript: StoredArtifact,
    pub(super) pending: StoredArtifact,
    pub(super) observed_files: StoredArtifact,
    pub(super) message_count: u64,
    pub(super) pending_count: u64,
    pub(super) observed_file_count: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct TranscriptManifest {
    pub(super) invocation: u64,
    pub(super) request_prefix: String,
    pub(super) current_inputs: Vec<u64>,
    pub(super) message_ids: Vec<u64>,
    pub(super) pending: Vec<PendingDescriptor>,
    pub(super) files: Vec<String>,
    pub(super) facts_through: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct SegmentContinuation {
    pub(super) schema_version: u16,
    pub(super) invocation: u64,
    pub(super) request_prefix: String,
    pub(super) segment_sequence: u64,
    pub(super) protocol_limits_sha256: [u8; 32],
    #[serde(default)]
    pub(super) progress: SegmentProgress,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) pending_batch: Option<SegmentPendingBatch>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct SegmentProgress {
    pub(super) model_turns: u64,
    pub(super) tool_calls: u64,
    pub(super) compactions: u64,
    pub(super) retries: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct SegmentPendingBatch {
    pub(super) assistant_source: u64,
    pub(super) calls: Vec<CallIdentity>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingDescriptor {
    pub(super) key: [u8; 16],
    pub(super) invocation: u64,
    pub(super) call: CallIdentity,
    pub(super) source: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) source_artifact_sha256: Option<[u8; 32]>,
    pub(super) handle: Option<String>,
    pub(super) state: PendingState,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum PendingState {
    Proposed,
    Running,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckpointManifest {
    pub(super) schema_version: u16,
    pub(super) scope: [u8; 32],
    pub(super) generation: u64,
    pub(super) previous: Option<[u8; 32]>,
    pub(super) through_event: u64,
    pub(super) working_state: StoredArtifact,
    pub(super) transcript_manifest: StoredArtifact,
    pub(super) source_index: StoredArtifact,
    pub(super) view: StoredArtifact,
    pub(super) render_policy: [u8; 32],
    pub(super) validation: StoredArtifact,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) view_binding: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ViewValidation {
    pub(super) model_revision: u64,
    pub(super) state_revision: u64,
    pub(super) through_observation: u64,
    pub(super) profile: [u8; 16],
    pub(super) profile_revision: u64,
    pub(super) estimated_input_tokens: u64,
    pub(super) uncompacted_input_tokens: u64,
    pub(super) input_tokens_saved: u64,
    pub(super) archive_bytes: u64,
    pub(super) stale_entries: usize,
    pub(super) max_input_tokens: u64,
    pub(super) selected_observations: Vec<u64>,
    pub(super) omitted_entries: usize,
    pub(super) pending_operations: usize,
    pub(super) local_compactor_failures: u64,
    pub(super) retrieval_calls: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) working_selection_frontier: Option<WorkingSelectionFrontier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) tool_policy: Option<[u8; 32]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) segment_continuation: Option<SegmentContinuation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkingSelectionFrontier {
    pub(super) schema_version: u16,
    pub(super) state_revision: u64,
    pub(super) available_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) minimum_required_tokens: Option<u64>,
    pub(super) required_digest: [u8; 32],
    pub(super) required_entries: usize,
    pub(super) referenced_entries: Vec<[u8; 16]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) focus_entry: Option<[u8; 16]>,
}

pub(super) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, DeveloperLoopError> {
    let bytes = serde_json::to_vec(value).map_err(|_| error("encode host memory record"))?;
    u32::try_from(bytes.len()).map_err(|_| error("host record representation exceeded"))?;
    Ok(bytes)
}

pub(super) fn decode<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
) -> Result<T, DeveloperLoopError> {
    u32::try_from(bytes.len()).map_err(|_| error("host record representation exceeded"))?;
    let value: T = serde_json::from_slice(bytes).map_err(|_| error("decode host memory record"))?;
    if encode(&value)? != bytes {
        return Err(error("noncanonical host memory record"));
    }
    Ok(value)
}
