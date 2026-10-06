//! Versioned transaction metadata, independent of inline patch payload admission.

use std::collections::BTreeSet;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_types::{Generation, RevisionNumber, Sha256Digest, WorkspaceId};

use crate::{
    DirectoryMode, ErrorCode, FileMode, PatchError, PatchIdentity, PatchOperation,
    PatchOperationContext, PatchOperationKind, Preimage, RecoveryClass, RollbackStatus,
    WorkspacePath,
};

mod codec;
mod extended;
mod pages;
use codec::{
    kind_from_tag, kind_tag, read_count, read_identity, shape_valid, write_count, write_identity,
};

const MAGIC: &[u8; 20] = b"peritus-patch-txn-v1";
const SCHEMA_VERSION: u16 = 1;

// Snapshot metadata contains only canonical paths and fixed-width identities. It has no
// command-payload or collection admission ceiling; decoding checks counts against the actual
// remaining bytes before allocating. Path validity remains owned by WorkspacePath.
const SNAPSHOT_METADATA_LIMITS: CodecLimits = CodecLimits {
    max_frame_bytes: usize::MAX,
    max_payload_bytes: usize::MAX,
    max_collection_items: usize::MAX,
    max_string_bytes: usize::MAX,
    max_opaque_bytes: usize::MAX,
    max_nesting_depth: 1,
};

/// Durable transaction progress phase.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TransactionPhase {
    /// Finals and backups are prepared; no workspace target has been changed.
    Prepared,
    /// Workspace mutation may be partially complete.
    Installing,
    /// Every postimage was installed and re-read successfully.
    Installed,
}

impl TransactionPhase {
    const fn tag(self) -> u8 {
        match self {
            Self::Prepared => 1,
            Self::Installing => 2,
            Self::Installed => 3,
        }
    }

    const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Prepared),
            2 => Some(Self::Installing),
            3 => Some(Self::Installed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TargetIdentity {
    File { digest: Sha256Digest, size: u64, mode: FileMode },
    EmptyDirectory { mode: DirectoryMode },
}

impl TargetIdentity {
    pub(super) const fn from_preimage(preimage: Preimage) -> Option<Self> {
        match preimage {
            Preimage::Absent => None,
            Preimage::Present { digest, size, mode } => Some(Self::File { digest, size, mode }),
            Preimage::EmptyDirectory { mode } => Some(Self::EmptyDirectory { mode }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ManifestEntry {
    pub(super) kind: PatchOperationKind,
    pub(super) path: WorkspacePath,
    pub(super) preimage: Option<TargetIdentity>,
    pub(super) postimage: Option<TargetIdentity>,
}

impl ManifestEntry {
    fn from_operation(operation: &PatchOperation) -> Self {
        let postimage = TargetIdentity::from_preimage(operation.postimage());
        Self {
            kind: operation.kind(),
            path: operation.path().clone(),
            preimage: TargetIdentity::from_preimage(operation.preimage()),
            postimage,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Manifest {
    schema: u16,
    pub(super) phase: TransactionPhase,
    pub(super) workspace_id: WorkspaceId,
    pub(super) generation: Generation,
    pub(super) revision: RevisionNumber,
    pub(super) identity: PatchIdentity,
    pub(super) entries: Vec<ManifestEntry>,
    pub(super) created_directories: Vec<WorkspacePath>,
}

impl Manifest {
    pub(super) const fn binding(&self) -> super::RecoveryBinding {
        super::RecoveryBinding::new(self.workspace_id, self.generation, self.revision)
    }

    pub(super) fn from_plan(
        plan: &crate::PatchPlan,
        created_directories: Vec<WorkspacePath>,
    ) -> Self {
        Self::from_patch(&plan.patch, created_directories)
    }

    fn from_patch(patch: &crate::PatchSet, created_directories: Vec<WorkspacePath>) -> Self {
        Self {
            schema: if patch.has_extended_paths() {
                5
            } else if patch.is_paged() {
                4
            } else if patch.operations().iter().any(PatchOperation::covers_directory) {
                3
            } else if patch.is_snapshot() {
                2
            } else {
                SCHEMA_VERSION
            },
            phase: TransactionPhase::Prepared,
            workspace_id: patch.workspace_id(),
            generation: patch.expected_generation(),
            revision: patch.expected_revision(),
            identity: patch.identity(),
            entries: patch.operations().iter().map(ManifestEntry::from_operation).collect(),
            created_directories,
        }
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, PatchError> {
        if self.schema == 5 {
            let mut bytes = Vec::new();
            extended::write(self, &mut bytes)?;
            return Ok(bytes);
        }
        if self.schema == 4 {
            let mut bytes = Vec::new();
            pages::write(self, &mut bytes)?;
            return Ok(bytes);
        }
        let mut writer = CanonicalWriter::new(if self.schema == 1 {
            CodecLimits::LEGACY_V1
        } else {
            SNAPSHOT_METADATA_LIMITS
        });
        let result = (|| {
            writer.write_fixed(MAGIC)?;
            writer.write_u16(self.schema)?;
            writer.write_u8(self.phase.tag())?;
            writer.write_fixed(self.workspace_id.as_bytes())?;
            writer.write_u64(self.generation.get())?;
            writer.write_u64(self.revision.get())?;
            writer.write_fixed(self.identity.as_bytes())?;
            write_count(&mut writer, self.entries.len(), self.schema)?;
            for entry in &self.entries {
                writer.write_u8(kind_tag(entry.kind))?;
                writer.write_str(entry.path.as_str())?;
                write_identity(&mut writer, entry.preimage)?;
                write_identity(&mut writer, entry.postimage)?;
            }
            write_count(&mut writer, self.created_directories.len(), self.schema)?;
            for directory in &self.created_directories {
                writer.write_str(directory.as_str())?;
            }
            Ok::<(), peritus_codec::CodecError>(())
        })();
        result.map_err(encoding_error)?;
        let checksum = peritus_codec::sha256(writer.as_slice());
        writer.write_fixed(checksum.as_bytes()).map_err(encoding_error)?;
        Ok(writer.into_bytes())
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, PatchError> {
        if bytes.len() < 32 {
            return Err(corrupt_manifest());
        }
        let (payload, checksum) = bytes.split_at(bytes.len() - 32);
        if peritus_codec::sha256(payload).as_bytes() != checksum {
            return Err(corrupt_manifest());
        }
        let mut reader = CanonicalReader::new(payload, SNAPSHOT_METADATA_LIMITS);
        let result = (|| {
            if &reader.read_fixed::<20>().ok()? != MAGIC {
                return None;
            }
            let schema = reader.read_u16().ok()?;
            if !(1..=5).contains(&schema) {
                return None;
            }
            if schema == 5 && reader.read_u8().ok()? != crate::path::native_platform_tag() {
                return None;
            }
            if schema == 1 && bytes.len() > CodecLimits::LEGACY_V1.max_payload_bytes {
                return None;
            }
            let phase = TransactionPhase::from_tag(reader.read_u8().ok()?)?;
            let workspace_id = WorkspaceId::new(reader.read_fixed::<16>().ok()?).ok()?;
            let generation = Generation::new(reader.read_u64().ok()?).ok()?;
            let revision = RevisionNumber::new(reader.read_u64().ok()?).ok()?;
            let identity = PatchIdentity::new(Sha256Digest::new(reader.read_fixed::<32>().ok()?));
            let entry_count = read_count(&mut reader, schema, 8)?;
            if entry_count == 0
                || (schema == 1 && entry_count > crate::set::LEGACY_PATCH_OPERATIONS)
            {
                return None;
            }
            let (entries, created_directories) = read_contents(&mut reader, schema, entry_count)?;
            reader.finish().ok()?;
            let manifest = Self {
                schema,
                phase,
                workspace_id,
                generation,
                revision,
                identity,
                entries,
                created_directories,
            };
            manifest.structure_valid().then_some(manifest)
        })();
        result.ok_or_else(corrupt_manifest)
    }

    pub(super) fn write_to(&self, output: &mut dyn std::io::Write) -> Result<(), PatchError> {
        if self.schema == 5 {
            extended::write(self, output)
        } else if self.schema == 4 {
            pages::write(self, output)
        } else {
            output.write_all(&self.encode()?).map_err(|error| {
                PatchError::io(
                    PatchOperationContext::PersistManifest,
                    RollbackStatus::NotRequired,
                    error,
                )
            })
        }
    }

    fn structure_valid(&self) -> bool {
        for pair in self.entries.windows(2) {
            if pair[0].path >= pair[1].path || pair[0].path.is_ancestor_of(&pair[1].path) {
                return false;
            }
        }
        for pair in self.created_directories.windows(2) {
            let ordering = pair[0]
                .components()
                .count()
                .cmp(&pair[1].components().count())
                .then_with(|| pair[0].cmp(&pair[1]));
            if !ordering.is_lt() {
                return false;
            }
        }
        self.created_directories
            .iter()
            .all(|directory| self.entries.iter().any(|entry| directory.is_ancestor_of(&entry.path)))
    }
}

fn read_contents(
    reader: &mut CanonicalReader<'_>,
    schema: u16,
    count: usize,
) -> Option<(Vec<ManifestEntry>, Vec<WorkspacePath>)> {
    if schema == 5 {
        return extended::read(reader, count);
    }
    let entries = if schema == 4 {
        pages::read_entries(reader, count)?
    } else {
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = kind_from_tag(reader.read_u8().ok()?)?;
            if schema < 3
                && matches!(
                    kind,
                    PatchOperationKind::CreateDirectory | PatchOperationKind::DeleteDirectory
                )
            {
                return None;
            }
            let path = WorkspacePath::new(reader.read_str().ok()?).ok()?;
            if !path.is_legacy_portable() {
                return None;
            }
            let preimage = read_identity(reader, schema).ok()?;
            let postimage = read_identity(reader, schema).ok()?;
            if !shape_valid(kind, preimage, postimage) {
                return None;
            }
            entries.push(ManifestEntry { kind, path, preimage, postimage });
        }
        entries
    };
    let count = read_count(reader, schema, 5)?;
    let directories = if schema == 4 {
        pages::read_directories(reader, count)?
    } else {
        let mut directories = Vec::with_capacity(count);
        for _ in 0..count {
            let path = WorkspacePath::new(reader.read_str().ok()?).ok()?;
            if !path.is_legacy_portable() {
                return None;
            }
            directories.push(path);
        }
        directories
    };
    Some((entries, directories))
}

pub(super) fn legacy_manifest_fits(patch: &crate::PatchSet) -> Result<bool, PatchError> {
    let mut directories = BTreeSet::new();
    for operation in patch.operations() {
        let components: Vec<_> = operation.path().components().collect();
        let mut relative = String::new();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if !relative.is_empty() {
                relative.push('/');
            }
            relative.push_str(component);
            directories.insert(WorkspacePath::new(relative.clone())?);
        }
    }
    let mut directories: Vec<_> = directories.into_iter().collect();
    directories.sort_by(|left, right| {
        left.components().count().cmp(&right.components().count()).then_with(|| left.cmp(right))
    });
    match Manifest::from_patch(patch, directories).encode() {
        Ok(_) => Ok(true),
        Err(error) if error.recovery_class() == RecoveryClass::Retry => Err(error),
        Err(_) => Ok(false),
    }
}

fn encoding_error(error: peritus_codec::CodecError) -> PatchError {
    if error.kind() == peritus_codec::CodecErrorKind::AllocationUnavailable {
        PatchError::codec_allocation(PatchOperationContext::PersistManifest, error)
    } else {
        corrupt_manifest()
    }
}

const fn corrupt_manifest() -> PatchError {
    PatchError::message(
        ErrorCode::CorruptManifest,
        RecoveryClass::FenceWorkspace,
        PatchOperationContext::Recover,
        RollbackStatus::Indeterminate,
        "transaction manifest is malformed or unsupported",
    )
}

#[cfg(test)]
mod tests;
