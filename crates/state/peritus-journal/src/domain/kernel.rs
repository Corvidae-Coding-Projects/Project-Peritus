//! B0 durable-transition adapter and reducer-driven restart replay.

mod capsule;
mod commit;
mod recovery;
mod state_digest;

use core::fmt;

use peritus_kernel::{
    CommandEnvelope, KernelAggregate, KernelCommand, KernelGenesis, KernelTransition,
};
use peritus_types::{ProjectId, SessionId, Sha256Digest};

use crate::{AggregateId, AggregateKey, AggregateKind, ExactFrame, JournalError, JournalErrorKind};

pub use commit::{CommittedKernelTransition, KernelCommitRequest};
pub use recovery::RecoveredKernelAggregate;

const NAMESPACE: u16 = 100;
const DOMAIN: &[u8] = b"peritus.kernel-transition.v1";
const MAX_INPUT_REFERENCES: usize = 64;
const MAX_INPUT_IDENTITY_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CapsuleKind {
    Genesis,
    Transition,
}

/// Immutable digest-bound reference to one B1/B2 reducer input persisted elsewhere.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct KernelInputReference {
    kind: u16,
    identity: Vec<u8>,
    digest: Sha256Digest,
}

impl KernelInputReference {
    /// Creates a bounded input reference.
    ///
    /// # Errors
    ///
    /// Rejects a zero kind or empty/oversized identity.
    pub fn new(kind: u16, identity: Vec<u8>, digest: Sha256Digest) -> Result<Self, JournalError> {
        if kind == 0 || identity.is_empty() || identity.len() > MAX_INPUT_IDENTITY_BYTES {
            return Err(input("kernel input reference is outside its canonical bounds"));
        }
        Ok(Self { kind, identity, digest })
    }

    /// Returns the stable nonzero input kind.
    #[must_use]
    pub const fn kind(&self) -> u16 {
        self.kind
    }

    /// Borrows the stable input identity.
    #[must_use]
    pub fn identity(&self) -> &[u8] {
        &self.identity
    }

    /// Returns the exact input digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Checked replay capsule loaded from the same transaction as its B0 event.
pub struct KernelReplayCapsule {
    kind: CapsuleKind,
    project_id: ProjectId,
    session_id: SessionId,
    envelope: CommandEnvelope,
    envelope_frame: ExactFrame,
    command: Option<KernelCommand>,
    command_frame: Option<ExactFrame>,
    inputs: Vec<KernelInputReference>,
    successor_digest: Sha256Digest,
}

impl KernelReplayCapsule {
    /// Returns whether this capsule opens the aggregate.
    #[must_use]
    pub const fn is_genesis(&self) -> bool {
        matches!(self.kind, CapsuleKind::Genesis)
    }

    /// Returns the project identity supplied to genesis replay.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns the session identity and aggregate key.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Returns the exact decoded reducer envelope.
    #[must_use]
    pub const fn envelope(&self) -> CommandEnvelope {
        self.envelope
    }

    /// Borrows the decoded command for a non-genesis transition.
    #[must_use]
    pub const fn command(&self) -> Option<&KernelCommand> {
        self.command.as_ref()
    }

    /// Borrows canonical reducer input references.
    #[must_use]
    pub fn inputs(&self) -> &[KernelInputReference] {
        &self.inputs
    }

    /// Returns the expected canonical successor-state digest.
    #[must_use]
    pub const fn successor_digest(&self) -> Sha256Digest {
        self.successor_digest
    }
}

/// Bounded failure reported by the application input resolver during reducer replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelReplayFailure {
    detail: &'static str,
}

impl KernelReplayFailure {
    /// Creates a replay-input failure with static bounded detail.
    #[must_use]
    pub const fn new(detail: &'static str) -> Self {
        Self { detail }
    }
}

impl fmt::Display for KernelReplayFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.detail)
    }
}

impl std::error::Error for KernelReplayFailure {}

/// Application port that resolves capsule references and invokes the verified B0 reducers.
pub trait KernelReplayDriver {
    /// Replays the session-open reducer from the exact genesis capsule.
    ///
    /// # Errors
    ///
    /// Returns a bounded failure when an input reference cannot be resolved or replayed.
    fn replay_genesis(
        &mut self,
        capsule: &KernelReplayCapsule,
    ) -> Result<KernelGenesis, KernelReplayFailure>;

    /// Replays one command against the exact previously reconstructed aggregate.
    ///
    /// # Errors
    ///
    /// Returns a bounded failure when an input reference cannot be resolved or replayed.
    fn replay_transition(
        &mut self,
        before: KernelAggregate,
        capsule: &KernelReplayCapsule,
    ) -> Result<KernelTransition, KernelReplayFailure>;
}

fn validate_inputs(inputs: &[KernelInputReference]) -> Result<(), JournalError> {
    if inputs.len() > MAX_INPUT_REFERENCES {
        return Err(input("too many kernel replay input references"));
    }
    for pair in inputs.windows(2) {
        if pair[0] >= pair[1] {
            return Err(input("kernel replay input references are not strictly canonical"));
        }
    }
    Ok(())
}

fn aggregate_key(aggregate: &KernelAggregate) -> Result<AggregateKey, JournalError> {
    let id = AggregateId::new(*aggregate.session().id().as_bytes())
        .map_err(|_| input("kernel session identity cannot form an aggregate key"))?;
    Ok(AggregateKey::new(AggregateKind::Kernel, id))
}

const fn input(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "plan kernel commit", detail)
}

const fn corrupt(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::CorruptJournal, "recover kernel aggregate", detail)
}
