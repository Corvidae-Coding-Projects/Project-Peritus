//! Exact C1 snapshot and observed target values used by materialization plans.

use std::collections::BTreeMap;

use peritus_codec::{CanonicalReader, CanonicalWriter};
use peritus_git::{ObjectFormat, ObjectId};
use peritus_patch::{Preimage, WorkspacePath};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};
use peritus_workspace::SnapshotIdentity;

use super::{MaterializationError, MaterializationErrorKind, MaterializationRecovery};

/// Algorithm-tagged immutable Git object observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GitObjectIdentity {
    format: ObjectFormat,
    bytes: [u8; 32],
}

impl GitObjectIdentity {
    fn from_object(value: ObjectId) -> Self {
        let mut bytes = [0_u8; 32];
        bytes[..value.as_bytes().len()].copy_from_slice(value.as_bytes());
        Self { format: value.format(), bytes }
    }

    /// Returns the object format.
    #[must_use]
    pub const fn format(self) -> ObjectFormat {
        self.format
    }

    /// Returns exact object bytes without unused storage.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.format.byte_len()]
    }

    pub(crate) fn encode(
        &self,
        writer: &mut CanonicalWriter,
    ) -> Result<(), peritus_codec::CodecError> {
        writer.write_u8(match self.format {
            ObjectFormat::Sha1 => 1,
            ObjectFormat::Sha256 => 2,
        })?;
        writer.write_bytes(self.as_bytes())
    }

    pub(crate) fn decode(reader: &mut CanonicalReader<'_>) -> Result<Self, MaterializationError> {
        let format = match reader.read_u8().map_err(codec)? {
            1 => ObjectFormat::Sha1,
            2 => ObjectFormat::Sha256,
            _ => return Err(invalid("unknown Git object format")),
        };
        let value = reader.read_bytes().map_err(codec)?;
        if value.len() != format.byte_len() {
            return Err(invalid("Git object length does not match its algorithm"));
        }
        let mut bytes = [0_u8; 32];
        bytes[..value.len()].copy_from_slice(value);
        Ok(Self { format, bytes })
    }
}

/// Exact immutable C1 workspace observation bound into a plan or receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSnapshot {
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    commit: GitObjectIdentity,
    tree: GitObjectIdentity,
}

impl WorkspaceSnapshot {
    /// Copies the exact C1 snapshot identity into durable inert data.
    #[must_use]
    pub fn from_c1(value: &SnapshotIdentity) -> Self {
        Self {
            workspace_id: value.workspace_id(),
            generation: value.generation(),
            revision: value.revision(),
            commit: GitObjectIdentity::from_object(value.commit().object_id()),
            tree: GitObjectIdentity::from_object(value.tree().object_id()),
        }
    }

    #[cfg(test)]
    pub(crate) const fn test_fixture(
        workspace_id: WorkspaceId,
        generation: Generation,
        revision: RevisionNumber,
    ) -> Self {
        Self {
            workspace_id,
            generation,
            revision,
            commit: GitObjectIdentity { format: ObjectFormat::Sha256, bytes: [41; 32] },
            tree: GitObjectIdentity { format: ObjectFormat::Sha256, bytes: [42; 32] },
        }
    }

    /// Returns the workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the fenced generation.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    /// Returns the logical C1 revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }
    /// Returns the immutable commit identity.
    #[must_use]
    pub const fn commit(&self) -> GitObjectIdentity {
        self.commit
    }
    /// Returns the immutable tree identity.
    #[must_use]
    pub const fn tree(&self) -> GitObjectIdentity {
        self.tree
    }

    pub(crate) fn encode(
        &self,
        writer: &mut CanonicalWriter,
    ) -> Result<(), peritus_codec::CodecError> {
        writer.write_fixed(self.workspace_id.as_bytes())?;
        writer.write_u64(self.generation.get())?;
        writer.write_u64(self.revision.get())?;
        self.commit.encode(writer)?;
        self.tree.encode(writer)
    }

    pub(crate) fn decode(reader: &mut CanonicalReader<'_>) -> Result<Self, MaterializationError> {
        let workspace_id = WorkspaceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| invalid("workspace identity is zero"))?;
        let generation = Generation::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| invalid("workspace generation is zero"))?;
        let revision = RevisionNumber::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| invalid("workspace revision is zero"))?;
        Ok(Self {
            workspace_id,
            generation,
            revision,
            commit: GitObjectIdentity::decode(reader)?,
            tree: GitObjectIdentity::decode(reader)?,
        })
    }
}

/// Exact observed regular-file state at one candidate harness target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedFile {
    path: WorkspacePath,
    preimage: Preimage,
}

impl ObservedFile {
    /// Creates one present regular-file observation.
    ///
    /// # Errors
    /// Rejects an absent preimage because absence is represented by omitting the path.
    pub fn new(path: WorkspacePath, preimage: Preimage) -> Result<Self, MaterializationError> {
        if matches!(preimage, Preimage::Absent) {
            return Err(invalid("observed file must carry a present preimage"));
        }
        Ok(Self { path, preimage })
    }
}

/// Bounded exact target observation used by the pure planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedTarget {
    pub(super) snapshot: WorkspaceSnapshot,
    pub(super) files: BTreeMap<WorkspacePath, Preimage>,
}

impl ObservedTarget {
    /// Builds a duplicate-free observation over only candidate harness-owned paths.
    ///
    /// # Errors
    /// Rejects duplicate target observations.
    pub fn new(
        snapshot: WorkspaceSnapshot,
        files: Vec<ObservedFile>,
    ) -> Result<Self, MaterializationError> {
        let mut by_path = BTreeMap::new();
        for file in files {
            if by_path.insert(file.path, file.preimage).is_some() {
                return Err(invalid("target observation repeats a path"));
            }
        }
        Ok(Self { snapshot, files: by_path })
    }

    /// Returns the exact workspace snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &WorkspaceSnapshot {
        &self.snapshot
    }

    /// Returns a present target preimage, or absence when the path was not observed.
    #[must_use]
    pub fn preimage(&self, path: &WorkspacePath) -> Preimage {
        self.files.get(path).copied().unwrap_or(Preimage::Absent)
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
