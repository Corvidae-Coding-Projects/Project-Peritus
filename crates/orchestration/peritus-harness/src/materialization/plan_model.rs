//! Durable inert materialization plan value types.

use peritus_codec::{CanonicalReader, CanonicalWriter};
use peritus_patch::{FileMode, Preimage, WorkspacePath};
use peritus_types::{CommandId, EventId, HarnessId, Sha256Digest};

use crate::domain::RevisionDigest;

use super::{
    MaterializationError, MaterializationErrorKind, MaterializationRecovery, WorkspaceSnapshot,
};

const MAX_ROLLBACK_REASON_BYTES: usize = 1_024;

/// Stable compact identity derived from a complete plan digest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MaterializationPlanId([u8; 16]);

impl MaterializationPlanId {
    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub(crate) fn from_digest(digest: Sha256Digest) -> Self {
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        bytes[0] |= 0x80;
        Self(bytes)
    }

    pub(crate) fn decode(bytes: [u8; 16]) -> Result<Self, MaterializationError> {
        if bytes == [0; 16] {
            return Err(invalid("materialization plan identity is zero"));
        }
        Ok(Self(bytes))
    }
}

/// Why an immutable revision is being materialized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterializationReason {
    /// Normal forward installation.
    Forward,
    /// Explicit ancestor rollback without changing harness history.
    Rollback {
        /// Revision from which rollback was selected.
        source_revision: RevisionDigest,
        /// Required human-readable rollback reason.
        reason: String,
    },
}

impl MaterializationReason {
    /// Constructs a bounded explicit rollback reason.
    ///
    /// # Errors
    /// Rejects empty, control-bearing, or oversized diagnostic text.
    pub fn rollback(
        source_revision: RevisionDigest,
        reason: impl Into<String>,
    ) -> Result<Self, MaterializationError> {
        let reason = reason.into();
        if reason.is_empty()
            || reason.len() > MAX_ROLLBACK_REASON_BYTES
            || reason.chars().any(char::is_control)
        {
            return Err(invalid("rollback reason is empty, oversized, or contains controls"));
        }
        Ok(Self::Rollback { source_revision, reason })
    }

    pub(crate) fn encode(
        &self,
        writer: &mut CanonicalWriter,
    ) -> Result<(), peritus_codec::CodecError> {
        match self {
            Self::Forward => writer.write_u8(1),
            Self::Rollback { source_revision, reason } => {
                writer.write_u8(2)?;
                writer.write_fixed(source_revision.as_bytes())?;
                writer.write_str(reason)
            }
        }
    }

    pub(crate) fn decode(reader: &mut CanonicalReader<'_>) -> Result<Self, MaterializationError> {
        match reader.read_u8().map_err(codec)? {
            1 => Ok(Self::Forward),
            2 => Self::rollback(
                RevisionDigest::new(Sha256Digest::new(reader.read_fixed().map_err(codec)?)),
                reader.read_str().map_err(codec)?.to_owned(),
            ),
            _ => Err(invalid("unknown materialization reason")),
        }
    }
}

/// One exact path mutation in a deterministic materialization plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlannedFileOperation {
    /// Create or replace a target from one finalized C0 artifact.
    Install {
        /// Canonical target path.
        path: WorkspacePath,
        /// Exact expected target preimage.
        preimage: Preimage,
        /// Finalized content artifact digest.
        artifact_digest: Sha256Digest,
        /// Exact output byte length.
        byte_length: u64,
        /// Portable output file mode.
        mode: FileMode,
    },
    /// Delete a path owned by the exact prior receipt.
    Delete {
        /// Canonical target path.
        path: WorkspacePath,
        /// Exact present preimage proved by the prior receipt and observation.
        preimage: Preimage,
    },
}

impl PlannedFileOperation {
    /// Returns the target path.
    #[must_use]
    pub const fn path(&self) -> &WorkspacePath {
        match self {
            Self::Install { path, .. } | Self::Delete { path, .. } => path,
        }
    }

    /// Returns the exact preimage.
    #[must_use]
    pub const fn preimage(&self) -> Preimage {
        match self {
            Self::Install { preimage, .. } | Self::Delete { preimage, .. } => *preimage,
        }
    }

    /// Returns the payload artifact for an install operation.
    #[must_use]
    pub const fn artifact_digest(&self) -> Option<Sha256Digest> {
        match self {
            Self::Install { artifact_digest, .. } => Some(*artifact_digest),
            Self::Delete { .. } => None,
        }
    }
}

/// Complete deterministic C1-bound materialization plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationPlan {
    pub(super) id: MaterializationPlanId,
    pub(super) digest: Sha256Digest,
    pub(super) command_id: CommandId,
    pub(super) causal_event_id: EventId,
    pub(super) harness_id: HarnessId,
    pub(super) revision_digest: RevisionDigest,
    pub(super) revision_number: u64,
    pub(super) graph_digest: Sha256Digest,
    pub(super) target: WorkspaceSnapshot,
    pub(super) reason: MaterializationReason,
    pub(super) prior_receipt: Option<super::MaterializationReceiptId>,
    pub(super) operations: Vec<PlannedFileOperation>,
    pub(super) total_bytes: u64,
}

impl MaterializationPlan {
    /// Returns the stable plan identity.
    #[must_use]
    pub const fn id(&self) -> MaterializationPlanId {
        self.id
    }
    /// Returns the complete canonical plan digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the originating command.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns the causal event identity used for C1 artifact creation.
    #[must_use]
    pub const fn causal_event_id(&self) -> EventId {
        self.causal_event_id
    }
    /// Returns the harness lineage.
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.harness_id
    }
    /// Returns the full target revision digest.
    #[must_use]
    pub const fn revision_digest(&self) -> RevisionDigest {
        self.revision_digest
    }
    /// Returns the target logical harness revision number.
    #[must_use]
    pub const fn revision_number(&self) -> u64 {
        self.revision_number
    }
    /// Returns the complete checked graph digest.
    #[must_use]
    pub const fn graph_digest(&self) -> Sha256Digest {
        self.graph_digest
    }
    /// Returns the exact target snapshot before mutation.
    #[must_use]
    pub const fn target(&self) -> &WorkspaceSnapshot {
        &self.target
    }
    /// Returns the explicit materialization reason.
    #[must_use]
    pub const fn reason(&self) -> &MaterializationReason {
        &self.reason
    }
    /// Returns the prior receipt identity when ownership is inherited.
    #[must_use]
    pub const fn prior_receipt(&self) -> Option<super::MaterializationReceiptId> {
        self.prior_receipt
    }
    /// Returns canonical path-sorted operations.
    #[must_use]
    pub fn operations(&self) -> &[PlannedFileOperation] {
        &self.operations
    }
    /// Returns aggregate installed bytes.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
}

fn codec(error: peritus_codec::CodecError) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Codec,
        MaterializationRecovery::Quarantine,
        error.to_string(),
    )
}

fn invalid(detail: &'static str) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::InvalidPlan,
        MaterializationRecovery::CorrectInput,
        detail,
    )
}
