//! Version-one host journal payloads and rebuildable transcript/source projections.

use super::{error, storage::StoredArtifact};
use peritus_agent::DeveloperLoopError;
use peritus_context::working::{ObservationKind, ObservationSource};
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

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
                    && call.id.len() <= 256
                    && !call.name.is_empty()
                    && call.name.len() <= 128 =>
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
    Invocation { sequence: u64, request_prefix: String },
    Observation { observation: ArchivedObservation, reducer: StoredArtifact },
    StateEvent { reducer: StoredArtifact },
    Transcript { manifest: TranscriptManifest },
    Checkpoint { manifest: StoredArtifact },
    Compactor { input: Option<StoredArtifact>, output: Option<StoredArtifact>, failed: bool },
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
pub(super) struct PendingDescriptor {
    pub(super) key: [u8; 16],
    pub(super) invocation: u64,
    pub(super) call: CallIdentity,
    pub(super) source: u64,
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
}

pub(super) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, DeveloperLoopError> {
    let bytes = serde_json::to_vec(value).map_err(|_| error("encode host memory record"))?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(error("host record capacity exceeded"));
    }
    Ok(bytes)
}

pub(super) fn decode<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
) -> Result<T, DeveloperLoopError> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(error("host record capacity exceeded"));
    }
    let value: T = serde_json::from_slice(bytes).map_err(|_| error("decode host memory record"))?;
    if encode(&value)? != bytes {
        return Err(error("noncanonical host memory record"));
    }
    Ok(value)
}
