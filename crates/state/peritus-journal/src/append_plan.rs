//! Deterministic validation and hash-chain append planning.

mod validation;

use crate::{
    AggregateHead, AggregateKey, ArtifactDependency, CredentialRegistryInstall, EventDraft,
    JournalError, JournalErrorKind, OutboxDraft, StateInstall, StoreId, hash_chain::batch_hash,
};
use peritus_types::{CommandId, Sha256Digest};

use validation::{
    validate_and_hash_events, validate_artifacts, validate_bounds, validate_heads, validate_outbox,
    validate_state_installs,
};

/// Maximum immutable events in one atomic batch.
pub const MAX_BATCH_EVENTS: usize = 4_096;
/// Maximum aggregate heads in one atomic batch.
pub const MAX_BATCH_AGGREGATES: usize = 1_024;
/// Maximum state installs in one atomic batch.
pub const MAX_STATE_INSTALLS: usize = 4_096;
/// Maximum outbox rows in one atomic batch.
pub const MAX_OUTBOX_ENTRIES: usize = 4_096;
/// Maximum artifact dependencies in one atomic batch.
pub const MAX_ARTIFACT_DEPENDENCIES: usize = 4_096;

/// Exact aggregate-head precondition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HeadExpectation {
    /// The aggregate must not yet exist.
    Absent(AggregateKey),
    /// The aggregate must match this exact observed head.
    Present(AggregateHead),
}

impl HeadExpectation {
    /// Returns the aggregate key named by the precondition.
    #[must_use]
    pub const fn key(self) -> AggregateKey {
        match self {
            Self::Absent(key) => key,
            Self::Present(head) => head.key(),
        }
    }

    /// Returns the exact observed head, or absence.
    #[must_use]
    pub const fn observed(self) -> Option<AggregateHead> {
        match self {
            Self::Absent(_) => None,
            Self::Present(head) => Some(head),
        }
    }
}

/// Untrusted complete append input retained by the caller until planning succeeds.
#[derive(Debug, Eq, PartialEq)]
pub struct AppendRequest {
    store_id: StoreId,
    command_id: CommandId,
    request_digest: Sha256Digest,
    heads: Vec<HeadExpectation>,
    events: Vec<EventDraft>,
    state_installs: Vec<StateInstall>,
    artifact_dependencies: Vec<ArtifactDependency>,
    expected_authority_epoch: Option<crate::ExpectedAuthorityEpoch>,
    expected_registry: Option<crate::authority::RegistryExpectation>,
    registry_install: Option<CredentialRegistryInstall>,
    outbox: Vec<OutboxDraft>,
}

impl AppendRequest {
    /// Creates an append request. [`Self::plan`] performs complete deterministic validation.
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "the durability preconditions remain explicit")]
    pub const fn new(
        store_id: StoreId,
        command_id: CommandId,
        request_digest: Sha256Digest,
        heads: Vec<HeadExpectation>,
        events: Vec<EventDraft>,
        state_installs: Vec<StateInstall>,
        artifact_dependencies: Vec<ArtifactDependency>,
        expected_authority_epoch: Option<crate::ExpectedAuthorityEpoch>,
        registry_install: Option<CredentialRegistryInstall>,
        outbox: Vec<OutboxDraft>,
    ) -> Self {
        Self {
            store_id,
            command_id,
            request_digest,
            heads,
            events,
            state_installs,
            artifact_dependencies,
            expected_authority_epoch,
            expected_registry: None,
            registry_install,
            outbox,
        }
    }

    /// Validates ordering, identities, sequences, predecessors, CAS successors, and hashes.
    ///
    /// This function performs no I/O. A returned plan is the only input accepted by the `SQLite`
    /// append boundary.
    ///
    /// # Errors
    ///
    /// Returns a stable typed validation error for every rejected precondition or bound.
    pub fn plan(self) -> Result<AppendPlan, JournalError> {
        validate_bounds(&self)?;
        validate_heads(&self.heads)?;
        validate_state_installs(&self.state_installs)?;
        validate_artifacts(&self.artifact_dependencies)?;
        validate_outbox(&self.outbox)?;
        let planned_events = validate_and_hash_events(&self.heads, self.events, self.command_id)?;
        let batch_hash = batch_hash(
            self.store_id,
            self.command_id,
            self.request_digest,
            planned_events.iter().map(|event| event.event_hash),
            planned_events.len(),
            self.artifact_dependencies.iter().map(|dependency| dependency.digest()),
            self.artifact_dependencies.len(),
        );
        Ok(AppendPlan {
            store_id: self.store_id,
            command_id: self.command_id,
            request_digest: self.request_digest,
            heads: self.heads,
            events: planned_events,
            state_installs: self.state_installs,
            artifact_dependencies: self.artifact_dependencies,
            expected_authority_epoch: self.expected_authority_epoch,
            expected_registry: self.expected_registry,
            registry_install: self.registry_install,
            outbox: self.outbox,
            batch_hash,
        })
    }

    pub(crate) fn bind_domain_state(
        mut self,
        domain: &[u8],
        mut installs: Vec<StateInstall>,
    ) -> Result<Self, JournalError> {
        self.state_installs.append(&mut installs);
        self.state_installs.sort_by(|left, right| {
            (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
        });
        validate_state_installs(&self.state_installs)?;
        let mut binding = Vec::with_capacity(64 + domain.len() + self.state_installs.len() * 90);
        binding.extend_from_slice(b"PERITUS-C0-DOMAIN-STATE\0");
        binding.extend_from_slice(&(domain.len() as u64).to_be_bytes());
        binding.extend_from_slice(domain);
        binding.extend_from_slice(self.request_digest.as_bytes());
        binding.extend_from_slice(&(self.state_installs.len() as u64).to_be_bytes());
        for install in &self.state_installs {
            binding.extend_from_slice(&install.namespace().to_be_bytes());
            binding.extend_from_slice(&(install.key().len() as u64).to_be_bytes());
            binding.extend_from_slice(install.key());
            match install.expected_revision() {
                Some(revision) => {
                    binding.push(1);
                    binding.extend_from_slice(&revision.to_be_bytes());
                }
                None => binding.push(0),
            }
            binding.extend_from_slice(&install.revision().to_be_bytes());
            binding.extend_from_slice(install.digest().as_bytes());
        }
        self.request_digest = peritus_codec::sha256(&binding);
        Ok(self)
    }

    pub(crate) fn bind_registry_current(
        mut self,
        revision: u64,
        generation: u64,
        digest: Sha256Digest,
    ) -> Self {
        self.expected_registry =
            Some(crate::authority::RegistryExpectation { revision, generation, digest });
        let mut bytes = Vec::with_capacity(80);
        bytes.extend_from_slice(b"PERITUS-C0-REGISTRY-CURRENT\0");
        bytes.extend_from_slice(self.request_digest.as_bytes());
        bytes.extend_from_slice(&revision.to_be_bytes());
        bytes.extend_from_slice(&generation.to_be_bytes());
        bytes.extend_from_slice(digest.as_bytes());
        self.request_digest = peritus_codec::sha256(&bytes);
        self
    }

    pub(crate) fn validate_single_kernel_event(
        &self,
        aggregate: AggregateKey,
        event: peritus_kernel::KernelEvent,
        frame: &crate::ExactFrame,
        revision_digest: Sha256Digest,
    ) -> Result<(), JournalError> {
        let exact = self.events.as_slice();
        if self.command_id != event.command_id()
            || self.heads.len() != 1
            || self.heads[0].key() != aggregate
            || exact.len() != 1
            || exact[0].aggregate() != aggregate
            || exact[0].sequence() != event.sequence()
            || exact[0].event_id() != event.id()
            || exact[0].previous_event_id() != event.previous_event_id()
            || exact[0].frame() != frame
            || exact[0].revision_digest() != revision_digest
        {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "plan kernel commit",
                "append request does not contain exactly the accepted B0 event",
            ));
        }
        Ok(())
    }
}

/// Complete validated effect-free append plan accepted by the `SQLite` boundary.
#[derive(Debug, Eq, PartialEq)]
pub struct AppendPlan {
    pub(crate) store_id: StoreId,
    pub(crate) command_id: CommandId,
    pub(crate) request_digest: Sha256Digest,
    pub(crate) heads: Vec<HeadExpectation>,
    pub(crate) events: Vec<PlannedEvent>,
    pub(crate) state_installs: Vec<StateInstall>,
    pub(crate) artifact_dependencies: Vec<ArtifactDependency>,
    pub(crate) expected_authority_epoch: Option<crate::ExpectedAuthorityEpoch>,
    pub(crate) expected_registry: Option<crate::authority::RegistryExpectation>,
    pub(crate) registry_install: Option<CredentialRegistryInstall>,
    pub(crate) outbox: Vec<OutboxDraft>,
    pub(crate) batch_hash: Sha256Digest,
}

impl AppendPlan {
    /// Returns the exact store identity.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    /// Returns the command idempotency identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    /// Returns the request digest bound to the command identity.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }

    /// Returns the deterministic batch hash.
    #[must_use]
    pub const fn batch_hash(&self) -> Sha256Digest {
        self.batch_hash
    }

    /// Returns the event count.
    #[must_use]
    pub const fn event_count(&self) -> usize {
        self.events.len()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PlannedEvent {
    pub(crate) draft: EventDraft,
    pub(crate) previous_hash: Sha256Digest,
    pub(crate) event_hash: Sha256Digest,
}
