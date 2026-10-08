//! Restart-visible mutation operation and outcome records owned by the workspace target.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use peritus_patch::PatchIdentity;
use peritus_types::{
    ActionId, Generation, ResourceId, RevisionNumber, Sha256Digest, WorkspaceId,
};

use crate::{
    ErrorCode, RecoveryClass, WorkspaceError, WorkspaceOperation,
    gateway::MutationPermit,
};

const OPERATION_MAGIC: &[u8] = b"PERITUS-WORKSPACE-MUTATION-OPERATION-V1\0";
const OUTCOME_MAGIC: &[u8] = b"PERITUS-WORKSPACE-MUTATION-OUTCOME-V1\0";
const RECORD_DIRECTORY: &str = "workspace-mutation-records-v1";

pub(crate) fn record_root(namespace: &Path) -> PathBuf {
    namespace.join(RECORD_DIRECTORY)
}

/// Durable identity of one authorized patch operation before its filesystem effect begins.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MutationOperationReference {
    action_id: ActionId,
    action_digest: Sha256Digest,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    generation: Generation,
    revision: RevisionNumber,
    patch_identity: PatchIdentity,
}

impl MutationOperationReference {
    pub(crate) const fn new(
        permit: &MutationPermit,
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        patch_identity: PatchIdentity,
    ) -> Self {
        Self {
            action_id: permit.action_id(),
            action_digest: permit.action_digest(),
            workspace_id,
            resource_id,
            generation: permit.generation(),
            revision: permit.revision(),
            patch_identity,
        }
    }

    /// Returns the exact authorized action.
    #[must_use]
    pub const fn action_id(self) -> ActionId { self.action_id }
    /// Returns the authenticated action-intent digest.
    #[must_use]
    pub const fn action_digest(self) -> Sha256Digest { self.action_digest }
    /// Returns the exact workspace lineage.
    #[must_use]
    pub const fn workspace_id(self) -> WorkspaceId { self.workspace_id }
    /// Returns the exact target resource.
    #[must_use]
    pub const fn resource_id(self) -> ResourceId { self.resource_id }
    /// Returns the authorized workspace generation.
    #[must_use]
    pub const fn generation(self) -> Generation { self.generation }
    /// Returns the authorized workspace revision.
    #[must_use]
    pub const fn revision(self) -> RevisionNumber { self.revision }
    /// Returns the exact canonical patch identity.
    #[must_use]
    pub const fn patch_identity(self) -> PatchIdentity { self.patch_identity }

    /// Returns canonical versioned bytes suitable for durable handoff.
    #[must_use]
    pub fn canonical_bytes(self) -> Vec<u8> {
        let mut bytes = OPERATION_MAGIC.to_vec();
        bytes.extend_from_slice(self.action_id.as_bytes());
        bytes.extend_from_slice(self.action_digest.as_bytes());
        bytes.extend_from_slice(self.workspace_id.as_bytes());
        bytes.extend_from_slice(self.resource_id.as_bytes());
        bytes.extend_from_slice(&self.generation.get().to_be_bytes());
        bytes.extend_from_slice(&self.revision.get().to_be_bytes());
        bytes.extend_from_slice(self.patch_identity.as_bytes());
        bytes
    }

    /// Returns the digest naming the exact canonical operation record.
    #[must_use]
    pub fn digest(self) -> Sha256Digest { peritus_codec::sha256(&self.canonical_bytes()) }

    /// Decodes the exact version-one representation.
    ///
    /// # Errors
    /// Rejects truncated, extended, zero-identity, or zero-counter records.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        let mut reader = Reader::new(bytes, OPERATION_MAGIC)?;
        let value = Self {
            action_id: ActionId::new(reader.array()?).map_err(|_| record_error("mutation operation action identity is invalid"))?,
            action_digest: Sha256Digest::new(reader.array()?),
            workspace_id: WorkspaceId::new(reader.array()?).map_err(|_| record_error("mutation operation workspace identity is invalid"))?,
            resource_id: ResourceId::new(reader.array()?).map_err(|_| record_error("mutation operation resource identity is invalid"))?,
            generation: Generation::new(reader.u64()?).map_err(|_| record_error("mutation operation generation is invalid"))?,
            revision: RevisionNumber::new(reader.u64()?).map_err(|_| record_error("mutation operation revision is invalid"))?,
            patch_identity: PatchIdentity::from_digest(Sha256Digest::new(reader.array()?)),
        };
        reader.finish()?;
        Ok(value)
    }
}

/// Durable identity and exact installed-manifest evidence for one successful patch.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MutationOutcomeReference {
    operation: MutationOperationReference,
    installed_manifest_digest: Sha256Digest,
}

impl MutationOutcomeReference {
    pub(crate) const fn new(
        operation: MutationOperationReference,
        installed_manifest_digest: Sha256Digest,
    ) -> Self {
        Self { operation, installed_manifest_digest }
    }

    /// Returns the authorized operation incorporated by this outcome.
    #[must_use]
    pub const fn operation(self) -> MutationOperationReference { self.operation }
    /// Returns the installed patch-transaction manifest digest.
    #[must_use]
    pub const fn installed_manifest_digest(self) -> Sha256Digest {
        self.installed_manifest_digest
    }
    /// Returns the exact authorized action.
    #[must_use]
    pub const fn action_id(self) -> ActionId { self.operation.action_id() }
    /// Returns the exact workspace lineage.
    #[must_use]
    pub const fn workspace_id(self) -> WorkspaceId { self.operation.workspace_id() }
    /// Returns the exact target resource.
    #[must_use]
    pub const fn resource_id(self) -> ResourceId { self.operation.resource_id() }
    /// Returns the authorized workspace generation.
    #[must_use]
    pub const fn generation(self) -> Generation { self.operation.generation() }
    /// Returns the authorized workspace revision.
    #[must_use]
    pub const fn revision(self) -> RevisionNumber { self.operation.revision() }
    /// Returns the exact applied patch identity.
    #[must_use]
    pub const fn patch_identity(self) -> PatchIdentity { self.operation.patch_identity() }

    /// Returns canonical versioned bytes suitable for restart handoff.
    #[must_use]
    pub fn canonical_bytes(self) -> Vec<u8> {
        let operation = self.operation.canonical_bytes();
        let mut bytes = OUTCOME_MAGIC.to_vec();
        let operation_length = u64::try_from(operation.len())
            .expect("canonical mutation operation length fits u64");
        bytes.extend_from_slice(&operation_length.to_be_bytes());
        bytes.extend_from_slice(&operation);
        bytes.extend_from_slice(self.installed_manifest_digest.as_bytes());
        bytes
    }

    /// Returns the digest naming the exact canonical outcome record.
    #[must_use]
    pub fn digest(self) -> Sha256Digest { peritus_codec::sha256(&self.canonical_bytes()) }

    /// Decodes the exact version-one representation.
    ///
    /// # Errors
    /// Rejects truncated, extended, or invalid nested operation records.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        let mut reader = Reader::new(bytes, OUTCOME_MAGIC)?;
        let operation_bytes = reader.bytes()?;
        let operation = MutationOperationReference::from_canonical_bytes(operation_bytes)?;
        let installed_manifest_digest = Sha256Digest::new(reader.array()?);
        reader.finish()?;
        Ok(Self { operation, installed_manifest_digest })
    }
}

pub(crate) fn persist_operation(
    namespace: &Path,
    reference: MutationOperationReference,
) -> Result<(), WorkspaceError> {
    persist(namespace, RecordKind::Operation, reference.digest(), &reference.canonical_bytes())
}

pub(crate) fn persist_outcome(
    namespace: &Path,
    reference: MutationOutcomeReference,
) -> Result<(), WorkspaceError> {
    persist(namespace, RecordKind::Outcome, reference.digest(), &reference.canonical_bytes())
}

pub(crate) fn resolve_outcome(
    namespace: &Path,
    expected: MutationOutcomeReference,
) -> Result<MutationOutcomeReference, WorkspaceError> {
    let path = record_path(namespace, RecordKind::Outcome, expected.digest());
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| record_error("mutation outcome record is unavailable"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(record_error("mutation outcome record is not a real file"));
    }
    let bytes = fs::read(&path).map_err(|_| record_error("mutation outcome record cannot be read"))?;
    let observed = MutationOutcomeReference::from_canonical_bytes(&bytes)?;
    if observed != expected || observed.digest() != expected.digest() {
        return Err(record_error("mutation outcome record differs from its durable reference"));
    }
    Ok(observed)
}

#[derive(Clone, Copy)]
enum RecordKind { Operation, Outcome }

impl RecordKind {
    const fn suffix(self) -> &'static str {
        match self { Self::Operation => "operation", Self::Outcome => "outcome" }
    }
}

fn persist(
    namespace: &Path,
    kind: RecordKind,
    digest: Sha256Digest,
    bytes: &[u8],
) -> Result<(), WorkspaceError> {
    let directory = checked_directory(namespace)?;
    let path = record_path(namespace, kind, digest);
    let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            let existing = fs::read(&path)
                .map_err(|_| record_error("existing mutation record cannot be read"))?;
            if existing == bytes { return Ok(()); }
            return Err(record_error("existing mutation record differs from its digest identity"));
        }
        Err(_) => return Err(record_error("mutation record cannot be created exclusively")),
    };
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| record_error("mutation record cannot be synchronized"))?;
    crate::filesystem::sync_directory(&directory)
        .map_err(|_| record_error("mutation record directory cannot be synchronized"))
}

fn checked_directory(namespace: &Path) -> Result<PathBuf, WorkspaceError> {
    let directory = record_root(namespace);
    fs::create_dir_all(&directory)
        .map_err(|_| record_error("mutation record directory cannot be created"))?;
    let metadata = fs::symlink_metadata(&directory)
        .map_err(|_| record_error("mutation record directory cannot be inspected"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(record_error("mutation record path is not a real directory"));
    }
    let canonical = fs::canonicalize(&directory)
        .map_err(|_| record_error("mutation record directory cannot be canonicalized"))?;
    if !canonical.starts_with(namespace) {
        return Err(record_error("mutation record directory escaped its transaction namespace"));
    }
    Ok(canonical)
}

fn record_path(namespace: &Path, kind: RecordKind, digest: Sha256Digest) -> PathBuf {
    record_root(namespace).join(format!("{}.{}", hex(digest.as_bytes()), kind.suffix()))
}

fn hex(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use core::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("writing to String cannot fail");
    }
    value
}

struct Reader<'a> { bytes: &'a [u8], offset: usize }

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], magic: &[u8]) -> Result<Self, WorkspaceError> {
        if !bytes.starts_with(magic) { return Err(record_error("mutation record magic is invalid")); }
        Ok(Self { bytes, offset: magic.len() })
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], WorkspaceError> {
        let end = self.offset.checked_add(N).ok_or_else(|| record_error("mutation record length overflowed"))?;
        let value = self.bytes.get(self.offset..end)
            .ok_or_else(|| record_error("mutation record is truncated"))?
            .try_into().map_err(|_| record_error("mutation record field has invalid width"))?;
        self.offset = end;
        Ok(value)
    }
    fn u64(&mut self) -> Result<u64, WorkspaceError> { self.array().map(u64::from_be_bytes) }
    fn bytes(&mut self) -> Result<&'a [u8], WorkspaceError> {
        let length = usize::try_from(self.u64()?)
            .map_err(|_| record_error("mutation record nested length is not representable"))?;
        let end = self.offset.checked_add(length)
            .ok_or_else(|| record_error("mutation record nested length overflowed"))?;
        let value = self.bytes.get(self.offset..end)
            .ok_or_else(|| record_error("mutation record nested value is truncated"))?;
        self.offset = end;
        Ok(value)
    }
    fn finish(self) -> Result<(), WorkspaceError> {
        if self.offset == self.bytes.len() { Ok(()) } else { Err(record_error("mutation record has trailing bytes")) }
    }
}

const fn record_error(detail: &'static str) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::Indeterminate,
        WorkspaceOperation::Reconcile,
        RecoveryClass::Reconcile,
        detail,
    )
}
