//! Versioned transaction metadata, independent of inline patch payload admission.

use std::collections::BTreeSet;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_types::{Generation, RevisionNumber, Sha256Digest, WorkspaceId};

use crate::{
    ErrorCode, FileMode, PatchError, PatchIdentity, PatchOperation, PatchOperationContext,
    PatchOperationKind, Preimage, RecoveryClass, RollbackStatus, WorkspacePath,
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
    max_string_bytes: crate::MAX_PATH_BYTES,
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
pub(super) struct FileIdentity {
    pub(super) digest: Sha256Digest,
    pub(super) size: u64,
    pub(super) mode: FileMode,
}

impl FileIdentity {
    pub(super) const fn from_preimage(preimage: Preimage) -> Option<Self> {
        match preimage {
            Preimage::Absent => None,
            Preimage::Present { digest, size, mode } => Some(Self { digest, size, mode }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ManifestEntry {
    pub(super) kind: PatchOperationKind,
    pub(super) path: WorkspacePath,
    pub(super) preimage: Option<FileIdentity>,
    pub(super) postimage: Option<FileIdentity>,
}

impl ManifestEntry {
    fn from_operation(operation: &PatchOperation) -> Self {
        let postimage = FileIdentity::from_preimage(operation.postimage());
        Self {
            kind: operation.kind(),
            path: operation.path().clone(),
            preimage: FileIdentity::from_preimage(operation.preimage()),
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
            schema: if patch.is_snapshot() { 2 } else { SCHEMA_VERSION },
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
        let mut writer = CanonicalWriter::new(if self.schema == 1 {
            CodecLimits::PRODUCTION
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
        result.map_err(|_| corrupt_manifest())?;
        let checksum = peritus_codec::sha256(writer.as_slice());
        writer.write_fixed(checksum.as_bytes()).map_err(|_| corrupt_manifest())?;
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
            if schema != 1 && schema != 2 {
                return None;
            }
            if schema == 1 && bytes.len() > CodecLimits::PRODUCTION.max_payload_bytes {
                return None;
            }
            let phase = TransactionPhase::from_tag(reader.read_u8().ok()?)?;
            let workspace_id = WorkspaceId::new(reader.read_fixed::<16>().ok()?).ok()?;
            let generation = Generation::new(reader.read_u64().ok()?).ok()?;
            let revision = RevisionNumber::new(reader.read_u64().ok()?).ok()?;
            let identity = PatchIdentity::new(Sha256Digest::new(reader.read_fixed::<32>().ok()?));
            let entry_count = read_count(&mut reader, schema, 8)?;
            if entry_count == 0 || (schema == 1 && entry_count > crate::MAX_PATCH_OPERATIONS) {
                return None;
            }
            let mut entries = Vec::with_capacity(entry_count);
            for _ in 0..entry_count {
                let kind = kind_from_tag(reader.read_u8().ok()?)?;
                let path = WorkspacePath::new(reader.read_str().ok()?).ok()?;
                let preimage = read_identity(&mut reader, schema).ok()?;
                let postimage = read_identity(&mut reader, schema).ok()?;
                if !shape_valid(kind, preimage, postimage) {
                    return None;
                }
                entries.push(ManifestEntry { kind, path, preimage, postimage });
            }
            let directory_count = read_count(&mut reader, schema, 5)?;
            let mut created_directories = Vec::with_capacity(directory_count);
            for _ in 0..directory_count {
                created_directories.push(WorkspacePath::new(reader.read_str().ok()?).ok()?);
            }
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

fn write_count(
    writer: &mut CanonicalWriter,
    count: usize,
    schema: u16,
) -> Result<(), peritus_codec::CodecError> {
    if schema == 1 { writer.write_collection_len(count) } else { writer.write_u64(count as u64) }
}

fn read_count(
    reader: &mut CanonicalReader<'_>,
    schema: u16,
    minimum_bytes: usize,
) -> Option<usize> {
    let count = if schema == 1 {
        reader.read_collection_len().ok()?
    } else {
        usize::try_from(reader.read_u64().ok()?).ok()?
    };
    (count <= reader.remaining() / minimum_bytes
        && (schema != 1 || count <= CodecLimits::PRODUCTION.max_collection_items))
        .then_some(count)
}

pub(super) fn validate_patch_capacity(patch: &crate::PatchSet) -> Result<(), PatchError> {
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
    Manifest::from_patch(patch, directories).encode().map(|_| ())
}

fn write_identity(
    writer: &mut CanonicalWriter,
    identity: Option<FileIdentity>,
) -> Result<(), peritus_codec::CodecError> {
    writer.write_option_tag(identity.is_some())?;
    if let Some(identity) = identity {
        writer.write_fixed(identity.digest.as_bytes())?;
        writer.write_u64(identity.size)?;
        writer.write_u8(identity.mode.tag())?;
    }
    Ok(())
}

fn read_identity(
    reader: &mut CanonicalReader<'_>,
    schema: u16,
) -> Result<Option<FileIdentity>, ()> {
    if !reader.read_option_tag().map_err(|_| ())? {
        return Ok(None);
    }
    let digest = Sha256Digest::new(reader.read_fixed::<32>().map_err(|_| ())?);
    let size = reader.read_u64().map_err(|_| ())?;
    if schema == 1 && size > crate::set::MAX_FILE_BYTES as u64 {
        return Err(());
    }
    let mode = FileMode::from_tag(reader.read_u8().map_err(|_| ())?).ok_or(())?;
    Ok(Some(FileIdentity { digest, size, mode }))
}

const fn kind_tag(kind: PatchOperationKind) -> u8 {
    match kind {
        PatchOperationKind::Create => 1,
        PatchOperationKind::Replace => 2,
        PatchOperationKind::Delete => 3,
    }
}

const fn kind_from_tag(tag: u8) -> Option<PatchOperationKind> {
    match tag {
        1 => Some(PatchOperationKind::Create),
        2 => Some(PatchOperationKind::Replace),
        3 => Some(PatchOperationKind::Delete),
        _ => None,
    }
}

const fn shape_valid(
    kind: PatchOperationKind,
    preimage: Option<FileIdentity>,
    postimage: Option<FileIdentity>,
) -> bool {
    matches!(
        (kind, preimage.is_some(), postimage.is_some()),
        (PatchOperationKind::Create, false, true)
            | (PatchOperationKind::Replace, true, true)
            | (PatchOperationKind::Delete, true, false)
    )
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
