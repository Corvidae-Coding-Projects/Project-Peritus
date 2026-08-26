//! Validated journal-owned identities and aggregate kinds.

use crate::{JournalError, JournalErrorKind};
use vstd::prelude::*;

verus! {

/// Returns whether an identifier is not the reserved all-zero value.
pub closed spec fn valid_identity(bytes: [u8; 16]) -> bool {
    bytes[0] != 0 || bytes[1] != 0 || bytes[2] != 0 || bytes[3] != 0
        || bytes[4] != 0 || bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0
        || bytes[8] != 0 || bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0
        || bytes[12] != 0 || bytes[13] != 0 || bytes[14] != 0 || bytes[15] != 0
}

const fn validate_identity(bytes: [u8; 16]) -> (valid: bool)
    ensures valid == valid_identity(bytes)
{
    bytes[0] != 0 || bytes[1] != 0 || bytes[2] != 0 || bytes[3] != 0
        || bytes[4] != 0 || bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0
        || bytes[8] != 0 || bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0
        || bytes[12] != 0 || bytes[13] != 0 || bytes[14] != 0 || bytes[15] != 0
}

} // verus!

/// Identifies one durable journal store.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StoreId([u8; 16]);

impl StoreId {
    /// Creates an identity, rejecting the reserved all-zero representation.
    ///
    /// # Errors
    ///
    /// Returns an invalid-input error when all bytes are zero.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, JournalError> {
        if validate_identity(bytes) { Ok(Self(bytes)) } else { Err(invalid_identity()) }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one aggregate within its closed kind.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AggregateId([u8; 16]);

impl AggregateId {
    /// Creates an identity, rejecting the reserved all-zero representation.
    ///
    /// # Errors
    ///
    /// Returns an invalid-input error when all bytes are zero.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, JournalError> {
        if validate_identity(bytes) { Ok(Self(bytes)) } else { Err(invalid_identity()) }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Identifies one transactional outbox message.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutboxId([u8; 16]);

impl OutboxId {
    /// Creates an identity, rejecting the reserved all-zero representation.
    ///
    /// # Errors
    ///
    /// Returns an invalid-input error when all bytes are zero.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, JournalError> {
        if validate_identity(bytes) { Ok(Self(bytes)) } else { Err(invalid_identity()) }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

const fn invalid_identity() -> JournalError {
    JournalError::new(
        JournalErrorKind::InvalidInput,
        "validate identity",
        "all-zero identity is reserved",
    )
}

/// Closed aggregate family persisted by the journal.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AggregateKind {
    /// B0 kernel lifecycle state.
    Kernel,
    /// B1 budget state.
    Budget,
    /// B1 lease state.
    Lease,
    /// B1 approval state.
    Approval,
    /// B1 credential-registry history.
    CredentialRegistry,
    /// D0 durable inner-turn state and observations.
    Agent,
    /// D1 durable deterministic gate execution state and observations.
    Gate,
    /// C7 durable causal trace observations.
    Trace,
    /// D2 durable review-cycle and finding-lifecycle state.
    Review,
    /// D3 durable resource scheduler and worker-ownership state.
    Scheduler,
    /// D3 durable causal collaboration and delegation state.
    Collaboration,
    /// E0 durable writer-gate-review-fixer orchestration state.
    Orchestrator,
    /// E1 durable immutable harness revision and materialization state.
    Harness,
    /// E2 durable evidence-linked debugger jobs and reports.
    Debugger,
    /// E3 durable reproducible evaluation campaigns and reports.
    Evaluation,
    /// F0 durable production-harness evolution campaigns.
    EvolutionCampaign,
    /// F0 durable project-scoped production-harness pointer and activation history.
    ProductionHarness,
}

impl AggregateKind {
    pub(crate) const fn tag(self) -> i64 {
        match self {
            Self::Kernel => 1,
            Self::Budget => 2,
            Self::Lease => 3,
            Self::Approval => 4,
            Self::CredentialRegistry => 5,
            Self::Agent => 6,
            Self::Gate => 7,
            Self::Trace => 8,
            Self::Review => 9,
            Self::Scheduler => 10,
            Self::Collaboration => 11,
            Self::Orchestrator => 12,
            Self::Harness => 13,
            Self::Debugger => 14,
            Self::Evaluation => 15,
            Self::EvolutionCampaign => 16,
            Self::ProductionHarness => 17,
        }
    }

    pub(crate) const fn hash_tag(self) -> u16 {
        match self {
            Self::Kernel => 1,
            Self::Budget => 2,
            Self::Lease => 3,
            Self::Approval => 4,
            Self::CredentialRegistry => 5,
            Self::Agent => 6,
            Self::Gate => 7,
            Self::Trace => 8,
            Self::Review => 9,
            Self::Scheduler => 10,
            Self::Collaboration => 11,
            Self::Orchestrator => 12,
            Self::Harness => 13,
            Self::Debugger => 14,
            Self::Evaluation => 15,
            Self::EvolutionCampaign => 16,
            Self::ProductionHarness => 17,
        }
    }

    pub(crate) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Kernel),
            2 => Some(Self::Budget),
            3 => Some(Self::Lease),
            4 => Some(Self::Approval),
            5 => Some(Self::CredentialRegistry),
            6 => Some(Self::Agent),
            7 => Some(Self::Gate),
            8 => Some(Self::Trace),
            9 => Some(Self::Review),
            10 => Some(Self::Scheduler),
            11 => Some(Self::Collaboration),
            12 => Some(Self::Orchestrator),
            13 => Some(Self::Harness),
            14 => Some(Self::Debugger),
            15 => Some(Self::Evaluation),
            16 => Some(Self::EvolutionCampaign),
            17 => Some(Self::ProductionHarness),
            _ => None,
        }
    }
}

/// Exact aggregate identity including its closed family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AggregateKey {
    kind: AggregateKind,
    id: AggregateId,
}

impl AggregateKey {
    /// Creates an exact aggregate key.
    #[must_use]
    pub const fn new(kind: AggregateKind, id: AggregateId) -> Self {
        Self { kind, id }
    }

    /// Returns the aggregate family.
    #[must_use]
    pub const fn kind(self) -> AggregateKind {
        self.kind
    }

    /// Returns the aggregate identity.
    #[must_use]
    pub const fn id(self) -> AggregateId {
        self.id
    }
}
