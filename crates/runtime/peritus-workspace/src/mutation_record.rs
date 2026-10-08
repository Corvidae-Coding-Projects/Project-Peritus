//! Restart-visible mutation operation and outcome records owned by the workspace target.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use peritus_patch::PatchIdentity;
use peritus_types::{
    ActionId, EnvironmentId, EventId, Generation, ResourceId, RevisionNumber, Sha256Digest,
    WorkspaceId,
};

use crate::{
    ErrorCode, RecoveryClass, WorkspaceCallerBinding, WorkspaceError, WorkspaceGateway,
    WorkspaceOperation,
    gateway::MutationPermit,
};

const OPERATION_MAGIC: &[u8] = b"PERITUS-WORKSPACE-MUTATION-OPERATION-V1\0";
const OUTCOME_MAGIC: &[u8] = b"PERITUS-WORKSPACE-MUTATION-OUTCOME-V1\0";
const REPOSITORY_OPERATION_MAGIC: &[u8] =
    b"PERITUS-WORKSPACE-REPOSITORY-MUTATION-OPERATION-V1\0";
const REPOSITORY_OUTCOME_MAGIC: &[u8] =
    b"PERITUS-WORKSPACE-REPOSITORY-MUTATION-OUTCOME-V1\0";
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

/// Exact target-owned repository mutation distinguished before its Git effect begins.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RepositoryMutationKind {
    /// Candidate creation from one already applied patch.
    Candidate,
    /// History-preserving restoration of one retained snapshot.
    Rollback,
}

impl RepositoryMutationKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Candidate => 1,
            Self::Rollback => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self, WorkspaceError> {
        match tag {
            1 => Ok(Self::Candidate),
            2 => Ok(Self::Rollback),
            _ => Err(record_error("repository mutation kind is invalid")),
        }
    }
}

/// Optional exact C4 invocation identity incorporated into a repository mutation receipt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct RepositoryMutationCaller {
    environment_id: EnvironmentId,
    descriptor_digest: Sha256Digest,
    prepared_digest: Sha256Digest,
}

impl RepositoryMutationCaller {
    const fn from_binding(value: &WorkspaceCallerBinding) -> Self {
        Self {
            environment_id: value.environment_id(),
            descriptor_digest: value.descriptor_digest(),
            prepared_digest: value.prepared_digest(),
        }
    }
}

/// Durable identity of one authorized candidate or rollback before its Git effect begins.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RepositoryMutationOperationReference {
    kind: RepositoryMutationKind,
    action_id: ActionId,
    action_digest: Sha256Digest,
    workspace_id: WorkspaceId,
    resource_id: ResourceId,
    generation: Generation,
    revision: RevisionNumber,
    dispatch_event: EventId,
    request_digest: Sha256Digest,
    request: Vec<u8>,
    caller: Option<RepositoryMutationCaller>,
}

impl RepositoryMutationOperationReference {
    pub(crate) fn new(
        kind: RepositoryMutationKind,
        permit: &MutationPermit,
        workspace_id: WorkspaceId,
        resource_id: ResourceId,
        request: Vec<u8>,
        caller: Option<&WorkspaceCallerBinding>,
    ) -> Self {
        let request_digest = peritus_codec::sha256(&request);
        Self {
            kind,
            action_id: permit.action_id(),
            action_digest: permit.action_digest(),
            workspace_id,
            resource_id,
            generation: permit.generation(),
            revision: permit.revision(),
            dispatch_event: permit.dispatch_event(),
            request_digest,
            request,
            caller: caller.map(RepositoryMutationCaller::from_binding),
        }
    }

    /// Returns the exact repository operation kind.
    #[must_use]
    pub const fn kind(&self) -> RepositoryMutationKind { self.kind }
    /// Returns the authorized action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId { self.action_id }
    /// Returns the authenticated action-intent digest.
    #[must_use]
    pub const fn action_digest(&self) -> Sha256Digest { self.action_digest }
    /// Returns the exact workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId { self.workspace_id }
    /// Returns the exact target resource.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId { self.resource_id }
    /// Returns the authorized workspace generation.
    #[must_use]
    pub const fn generation(&self) -> Generation { self.generation }
    /// Returns the authorized logical revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber { self.revision }
    /// Returns the exact dispatch event consumed by the operation.
    #[must_use]
    pub const fn dispatch_event(&self) -> EventId { self.dispatch_event }
    /// Returns the digest of the exact authorized domain request.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest { self.request_digest }
    /// Borrows the complete canonical authorized domain request for reconciliation.
    #[must_use]
    pub fn request_bytes(&self) -> &[u8] { &self.request }
    /// Returns the bound tool descriptor digest when invoked through C4.
    #[must_use]
    pub const fn descriptor_digest(&self) -> Option<Sha256Digest> {
        match self.caller {
            Some(caller) => Some(caller.descriptor_digest),
            None => None,
        }
    }
    /// Returns the bound prepared-call digest when invoked through C4.
    #[must_use]
    pub const fn prepared_digest(&self) -> Option<Sha256Digest> {
        match self.caller {
            Some(caller) => Some(caller.prepared_digest),
            None => None,
        }
    }
    /// Returns the bound execution environment when invoked through C4.
    #[must_use]
    pub const fn environment_id(&self) -> Option<EnvironmentId> {
        match self.caller {
            Some(caller) => Some(caller.environment_id),
            None => None,
        }
    }

    /// Returns canonical versioned bytes suitable for restart handoff.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = REPOSITORY_OPERATION_MAGIC.to_vec();
        bytes.push(self.kind.tag());
        bytes.extend_from_slice(self.action_id.as_bytes());
        bytes.extend_from_slice(self.action_digest.as_bytes());
        bytes.extend_from_slice(self.workspace_id.as_bytes());
        bytes.extend_from_slice(self.resource_id.as_bytes());
        bytes.extend_from_slice(&self.generation.get().to_be_bytes());
        bytes.extend_from_slice(&self.revision.get().to_be_bytes());
        bytes.extend_from_slice(self.dispatch_event.as_bytes());
        bytes.extend_from_slice(self.request_digest.as_bytes());
        put_bytes(&mut bytes, &self.request);
        if let Some(caller) = self.caller {
            bytes.push(1);
            bytes.extend_from_slice(caller.environment_id.as_bytes());
            bytes.extend_from_slice(caller.descriptor_digest.as_bytes());
            bytes.extend_from_slice(caller.prepared_digest.as_bytes());
        } else {
            bytes.push(0);
        }
        bytes
    }

    /// Returns the digest naming this exact authorized repository operation.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest { peritus_codec::sha256(&self.canonical_bytes()) }

    /// Decodes the exact version-one representation.
    ///
    /// # Errors
    /// Rejects truncated, extended, zero-identity, or invalid tagged records.
    pub(crate) fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        let mut reader = Reader::new(bytes, REPOSITORY_OPERATION_MAGIC)?;
        let kind = RepositoryMutationKind::from_tag(reader.u8()?)?;
        let action_id = ActionId::new(reader.array()?)
            .map_err(|_| record_error("repository mutation action identity is invalid"))?;
        let action_digest = Sha256Digest::new(reader.array()?);
        let workspace_id = WorkspaceId::new(reader.array()?)
            .map_err(|_| record_error("repository mutation workspace identity is invalid"))?;
        let resource_id = ResourceId::new(reader.array()?)
            .map_err(|_| record_error("repository mutation resource identity is invalid"))?;
        let generation = Generation::new(reader.u64()?)
            .map_err(|_| record_error("repository mutation generation is invalid"))?;
        let revision = RevisionNumber::new(reader.u64()?)
            .map_err(|_| record_error("repository mutation revision is invalid"))?;
        let dispatch_event = EventId::new(reader.array()?)
            .map_err(|_| record_error("repository mutation dispatch identity is invalid"))?;
        let request_digest = Sha256Digest::new(reader.array()?);
        let request = reader.bytes()?.to_vec();
        if request.is_empty() || peritus_codec::sha256(&request) != request_digest {
            return Err(record_error("repository mutation request digest differs"));
        }
        let caller = match reader.u8()? {
            0 => None,
            1 => Some(RepositoryMutationCaller {
                environment_id: EnvironmentId::new(reader.array()?).map_err(|_| {
                    record_error("repository mutation environment identity is invalid")
                })?,
                descriptor_digest: Sha256Digest::new(reader.array()?),
                prepared_digest: Sha256Digest::new(reader.array()?),
            }),
            _ => return Err(record_error("repository mutation caller tag is invalid")),
        };
        reader.finish()?;
        Ok(Self {
            kind,
            action_id,
            action_digest,
            workspace_id,
            resource_id,
            generation,
            revision,
            dispatch_event,
            request_digest,
            request,
            caller,
        })
    }
}

/// Restart-visible exact result bytes for one authorized repository mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryMutationOutcomeReference {
    operation: RepositoryMutationOperationReference,
    result: Vec<u8>,
    result_digest: Sha256Digest,
}

impl RepositoryMutationOutcomeReference {
    pub(crate) fn new(
        operation: RepositoryMutationOperationReference,
        result: Vec<u8>,
    ) -> Result<Self, WorkspaceError> {
        if result.is_empty() {
            return Err(record_error("repository mutation result is empty"));
        }
        let result_digest = peritus_codec::sha256(&result);
        Ok(Self { operation, result, result_digest })
    }

    /// Borrows the exact authorized operation.
    #[must_use]
    pub const fn operation(&self) -> &RepositoryMutationOperationReference { &self.operation }
    /// Borrows canonical domain-result bytes retained as the logical-state settlement record.
    #[must_use]
    pub fn result_bytes(&self) -> &[u8] { &self.result }
    /// Returns the digest of the complete canonical domain result.
    #[must_use]
    pub const fn result_digest(&self) -> Sha256Digest { self.result_digest }
    /// Returns the digest used to adopt this operation after restart.
    #[must_use]
    pub fn operation_digest(&self) -> Sha256Digest { self.operation.digest() }

    /// Returns canonical versioned bytes suitable for restart handoff.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let operation = self.operation.canonical_bytes();
        let mut bytes = REPOSITORY_OUTCOME_MAGIC.to_vec();
        put_bytes(&mut bytes, &operation);
        put_bytes(&mut bytes, &self.result);
        bytes
    }

    /// Returns the digest naming this exact operation and result pair.
    #[must_use]
    pub fn digest(&self) -> Sha256Digest { peritus_codec::sha256(&self.canonical_bytes()) }

    /// Decodes the exact version-one representation.
    ///
    /// # Errors
    /// Rejects truncated, extended, invalid nested, or empty result records.
    pub(crate) fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        let mut reader = Reader::new(bytes, REPOSITORY_OUTCOME_MAGIC)?;
        let operation = RepositoryMutationOperationReference::from_canonical_bytes(reader.bytes()?)?;
        let result = reader.bytes()?.to_vec();
        reader.finish()?;
        Self::new(operation, result)
    }
}

/// Restart adoption state for an operation that must never be applied a second time implicitly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryMutationAdoption {
    /// The authorized operation exists but needs explicit repository reconciliation.
    Reconcile(RepositoryMutationOperationReference),
    /// The exact durable outcome is available for terminal-result replay.
    Completed(RepositoryMutationOutcomeReference),
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

pub(crate) fn persist_repository_operation(
    namespace: &Path,
    reference: &RepositoryMutationOperationReference,
) -> Result<(), WorkspaceError> {
    persist(
        namespace,
        RecordKind::RepositoryOperation,
        reference.digest(),
        &reference.canonical_bytes(),
    )
}

pub(crate) fn persist_repository_outcome(
    namespace: &Path,
    reference: &RepositoryMutationOutcomeReference,
) -> Result<(), WorkspaceError> {
    persist(
        namespace,
        RecordKind::RepositoryOutcome,
        reference.digest(),
        &reference.canonical_bytes(),
    )
}

impl WorkspaceGateway {
    /// Adopts an exact durable repository mutation without applying its effect again.
    ///
    /// # Errors
    /// Rejects an absent, malformed, foreign, or digest-mismatched operation or outcome record.
    pub fn adopt_repository_mutation(
        &self,
        operation_digest: Sha256Digest,
    ) -> Result<RepositoryMutationAdoption, WorkspaceError> {
        let adoption = resolve_repository_adoption(self.transaction_namespace(), operation_digest)?;
        let operation = match &adoption {
            RepositoryMutationAdoption::Reconcile(operation) => operation,
            RepositoryMutationAdoption::Completed(outcome) => outcome.operation(),
        };
        if operation.workspace_id() != self.state().binding().workspace_id()
            || operation.resource_id() != self.state().binding().resource_id()
        {
            return Err(record_error("repository mutation record belongs to another target"));
        }
        Ok(adoption)
    }

    /// Finds and adopts the unique durable repository mutation for an exact action.
    ///
    /// # Errors
    /// Rejects an absent, ambiguous, malformed, or foreign operation or outcome record.
    pub fn adopt_repository_mutation_action(
        &self,
        action_id: ActionId,
    ) -> Result<RepositoryMutationAdoption, WorkspaceError> {
        let directory = checked_directory(self.transaction_namespace())?;
        let mut matching = None;
        let entries = fs::read_dir(&directory)
            .map_err(|_| record_error("repository mutation records cannot be listed"))?;
        for entry in entries {
            let entry = entry
                .map_err(|_| record_error("repository mutation record cannot be inspected"))?;
            let path = entry.path();
            if path.extension().and_then(std::ffi::OsStr::to_str)
                != Some(RecordKind::RepositoryOperation.suffix())
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| record_error("repository mutation record cannot be inspected"))?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(record_error("repository mutation record is not a real file"));
            }
            let bytes = fs::read(&path)
                .map_err(|_| record_error("repository mutation record cannot be read"))?;
            let operation = RepositoryMutationOperationReference::from_canonical_bytes(&bytes)?;
            if operation.workspace_id() != self.state().binding().workspace_id()
                || operation.resource_id() != self.state().binding().resource_id()
            {
                return Err(record_error("repository mutation record belongs to another target"));
            }
            if operation.action_id() == action_id {
                if matching.replace(operation.digest()).is_some() {
                    return Err(record_error("repository mutation action has ambiguous records"));
                }
            }
        }
        let digest = matching
            .ok_or_else(|| record_error("repository mutation action record is unavailable"))?;
        resolve_repository_adoption(self.transaction_namespace(), digest)
    }
}

fn resolve_repository_adoption(
    namespace: &Path,
    operation_digest: Sha256Digest,
) -> Result<RepositoryMutationAdoption, WorkspaceError> {
    let operation_path = record_path(
        namespace,
        RecordKind::RepositoryOperation,
        operation_digest,
    );
    let operation = read_repository_operation(&operation_path, operation_digest)?;
    let directory = checked_directory(namespace)?;
    let entries = fs::read_dir(&directory)
        .map_err(|_| record_error("repository mutation outcomes cannot be listed"))?;
    let mut matching = None;
    for entry in entries {
        let entry = entry
            .map_err(|_| record_error("repository mutation outcome cannot be inspected"))?;
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str)
            != Some(RecordKind::RepositoryOutcome.suffix())
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| record_error("repository mutation outcome cannot be inspected"))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(record_error("repository mutation outcome is not a real file"));
        }
        let bytes = fs::read(&path)
            .map_err(|_| record_error("repository mutation outcome cannot be read"))?;
        let outcome = RepositoryMutationOutcomeReference::from_canonical_bytes(&bytes)?;
        if path != record_path(namespace, RecordKind::RepositoryOutcome, outcome.digest()) {
            return Err(record_error("repository mutation outcome digest differs from its path"));
        }
        if outcome.operation_digest() == operation_digest {
            if matching.replace(outcome).is_some() {
                return Err(record_error("repository mutation operation has ambiguous outcomes"));
            }
        }
    }
    match matching {
        Some(outcome) if outcome.operation() == &operation => {
            Ok(RepositoryMutationAdoption::Completed(outcome))
        }
        Some(_) => Err(record_error("repository mutation outcome differs from its operation")),
        None => Ok(RepositoryMutationAdoption::Reconcile(operation)),
    }
}

fn read_repository_operation(
    path: &Path,
    expected_digest: Sha256Digest,
) -> Result<RepositoryMutationOperationReference, WorkspaceError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| record_error("repository mutation operation is unavailable"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(record_error("repository mutation operation is not a real file"));
    }
    let bytes = fs::read(path)
        .map_err(|_| record_error("repository mutation operation cannot be read"))?;
    let operation = RepositoryMutationOperationReference::from_canonical_bytes(&bytes)?;
    if operation.digest() != expected_digest {
        return Err(record_error("repository mutation operation digest differs"));
    }
    Ok(operation)
}

#[derive(Clone, Copy)]
enum RecordKind {
    Operation,
    Outcome,
    RepositoryOperation,
    RepositoryOutcome,
}

impl RecordKind {
    const fn suffix(self) -> &'static str {
        match self {
            Self::Operation => "operation",
            Self::Outcome => "outcome",
            Self::RepositoryOperation => "repository-operation",
            Self::RepositoryOutcome => "repository-outcome",
        }
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

fn put_bytes(target: &mut Vec<u8>, value: &[u8]) {
    let length = u64::try_from(value.len()).expect("bounded repository mutation record fits u64");
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
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
    fn u8(&mut self) -> Result<u8, WorkspaceError> { self.array::<1>().map(|value| value[0]) }
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
