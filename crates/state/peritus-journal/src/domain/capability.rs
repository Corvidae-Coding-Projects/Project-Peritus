//! Commit-once capability-use persistence.

use peritus_policy::{ActorRole, CapabilityUseTransition};
use peritus_types::Sha256Digest;

use crate::{
    AppendRequest, CommittedBatch, JournalError, SqliteJournal, StateInstall,
    domain::{commit, encoding},
};

const NAMESPACE: u16 = 101;
const DOMAIN: &[u8] = b"peritus.capability-use.v1";
const VALUE_KIND: u16 = 1;

/// Move-only request binding one B1 capability-use successor to one atomic journal append.
pub struct CapabilityCommitRequest {
    append: AppendRequest,
    transition: CapabilityUseTransition,
    install: StateInstall,
}

impl CapabilityCommitRequest {
    /// Consumes a logical capability use and binds its complete successor projection to a CAS row.
    ///
    /// # Errors
    ///
    /// Returns an input or revision-overflow error before any I/O.
    pub fn new(
        append: AppendRequest,
        transition: CapabilityUseTransition,
        expected_revision: Option<u64>,
    ) -> Result<Self, JournalError> {
        let revision = commit::successor(expected_revision)?;
        let key = transition.successor().issuance_command_id().as_bytes().to_vec();
        let value = encode_transition(&transition);
        let install = StateInstall::new(NAMESPACE, key, expected_revision, revision, value)?;
        Ok(Self { append, transition, install })
    }
}

/// Opaque post-commit observation retaining the exact move-only logical capability transition.
pub struct CommittedCapabilityUse {
    batch: CommittedBatch,
    transition: CapabilityUseTransition,
    state_revision: u64,
    state_digest: Sha256Digest,
}

impl CommittedCapabilityUse {
    /// Borrows the exact committed event batch.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }

    /// Borrows the exact B1 transition whose successor became durable.
    #[must_use]
    pub const fn transition(&self) -> &CapabilityUseTransition {
        &self.transition
    }

    /// Returns the installed state revision.
    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Returns the digest of the complete durable successor projection.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }

    /// Consumes the receipt into the committed batch and move-only successor capability.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, peritus_policy::Capability) {
        (self.batch, self.transition.into_successor())
    }
}

impl SqliteJournal {
    /// Commits one capability use and its successor state exactly once.
    ///
    /// # Errors
    ///
    /// Returns journal CAS, idempotency, storage, or integrity failures.
    pub fn commit_capability_use(
        &mut self,
        request: CapabilityCommitRequest,
    ) -> Result<CommittedCapabilityUse, JournalError> {
        let CapabilityCommitRequest { append, transition, install } = request;
        let (batch, state) = commit::commit_state(self, append, DOMAIN, install)?;
        Ok(CommittedCapabilityUse {
            batch,
            transition,
            state_revision: state.revision(),
            state_digest: state.digest(),
        })
    }
}

fn encode_transition(transition: &CapabilityUseTransition) -> Vec<u8> {
    let mut payload = Vec::with_capacity(512);
    encoding::digest(&mut payload, transition.transition_digest());
    payload.extend_from_slice(transition.action_id().as_bytes());
    encoding::digest(&mut payload, transition.action_digest());
    let permission = transition.permission();
    payload.extend_from_slice(permission.resource_id().as_bytes());
    encoding::bytes_value(&mut payload, permission.capability_name().as_str().as_bytes());
    let scope = transition.scope();
    payload.extend_from_slice(scope.actor_id().as_bytes());
    encoding::u8_value(&mut payload, role_tag(scope.role()));
    payload.extend_from_slice(scope.environment_id().as_bytes());
    encoding::revision(&mut payload, scope.revision());
    encoding::instant(&mut payload, scope.validity().not_before());
    encoding::instant(&mut payload, scope.validity().expires_at());
    encoding::use_limit(&mut payload, scope.use_limit());
    encoding::u64_value(&mut payload, scope.permissions().as_slice().len() as u64);
    for permission in scope.permissions().as_slice() {
        payload.extend_from_slice(permission.resource_id().as_bytes());
        encoding::bytes_value(&mut payload, permission.capability_name().as_str().as_bytes());
    }
    encoding::instant(&mut payload, transition.used_at());
    encoding::use_limit(&mut payload, transition.previous_remaining());
    let successor = transition.successor();
    encoding::use_limit(&mut payload, successor.remaining_uses());
    encoding::instant(&mut payload, successor.issued_at());
    encoding::digest(&mut payload, successor.issuance_digest());
    payload.extend_from_slice(successor.issuance_command_id().as_bytes());
    encoding::u64_value(&mut payload, successor.time_state().epoch().get());
    encoding::u64_value(&mut payload, successor.time_state().greatest_tick_millis());
    encoding::value(VALUE_KIND, &payload)
}

const fn role_tag(role: ActorRole) -> u8 {
    match role {
        ActorRole::Writer => 1,
        ActorRole::Fixer => 2,
        ActorRole::Reviewer => 3,
        ActorRole::Evaluator => 4,
        ActorRole::GateRunner => 5,
        ActorRole::Orchestrator => 6,
        ActorRole::EvolutionAgent => 7,
        ActorRole::HumanAuthority => 8,
        ActorRole::DaemonService => 9,
        ActorRole::ProviderToolWorker => 10,
        ActorRole::Plugin => 11,
    }
}
