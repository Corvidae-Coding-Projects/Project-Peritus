//! Closed typed attribute keys and scalar values.

use peritus_types::Sha256Digest;

use crate::{ArtifactVaultReference, StatusCode};

/// Closed safe attribute keys. No caller-controlled text is accepted.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SafeAttributeKey {
    /// Provider request correlation identity.
    ProviderRequest,
    /// Tool descriptor or invocation correlation.
    ToolInvocation,
    /// Gate evaluation correlation.
    GateEvaluation,
    /// Remaining or consumed budget quantity.
    BudgetUnits,
    /// Retry ordinal.
    RetryAttempt,
    /// Cancellation classification.
    Cancellation,
    /// Recovery generation or count.
    Recovery,
    /// CPU nanoseconds.
    CpuNanos,
    /// Memory bytes.
    MemoryBytes,
    /// Input token count.
    InputTokens,
    /// Output token count.
    OutputTokens,
    /// Provider cost in microunits.
    CostMicrounits,
    /// Queue depth.
    QueueDepth,
    /// Dropped observation count.
    DroppedCount,
    /// Closed status.
    Status,
    /// Authorized encrypted raw evidence.
    ArtifactEvidence,
}

impl SafeAttributeKey {
    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::ProviderRequest => 1,
            Self::ToolInvocation => 2,
            Self::GateEvaluation => 3,
            Self::BudgetUnits => 4,
            Self::RetryAttempt => 5,
            Self::Cancellation => 6,
            Self::Recovery => 7,
            Self::CpuNanos => 8,
            Self::MemoryBytes => 9,
            Self::InputTokens => 10,
            Self::OutputTokens => 11,
            Self::CostMicrounits => 12,
            Self::QueueDepth => 13,
            Self::DroppedCount => 14,
            Self::Status => 15,
            Self::ArtifactEvidence => 16,
        }
    }

    pub(crate) const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::ProviderRequest),
            2 => Some(Self::ToolInvocation),
            3 => Some(Self::GateEvaluation),
            4 => Some(Self::BudgetUnits),
            5 => Some(Self::RetryAttempt),
            6 => Some(Self::Cancellation),
            7 => Some(Self::Recovery),
            8 => Some(Self::CpuNanos),
            9 => Some(Self::MemoryBytes),
            10 => Some(Self::InputTokens),
            11 => Some(Self::OutputTokens),
            12 => Some(Self::CostMicrounits),
            13 => Some(Self::QueueDepth),
            14 => Some(Self::DroppedCount),
            15 => Some(Self::Status),
            16 => Some(Self::ArtifactEvidence),
            _ => None,
        }
    }
}

/// Closed safe scalar attribute values.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SafeAttributeValue {
    /// Unsigned count or quantity.
    Count(u64),
    /// Duration in nanoseconds.
    DurationNanos(u64),
    /// Non-secret 16-byte domain identity.
    Identifier([u8; 16]),
    /// Canonical content or descriptor digest.
    Digest(Sha256Digest),
    /// Closed status.
    Status(StatusCode),
    /// Finalized encrypted evidence reference.
    Vault(ArtifactVaultReference),
}

/// One canonical key/value attribute.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SafeAttribute {
    key: SafeAttributeKey,
    value: SafeAttributeValue,
}

impl SafeAttribute {
    /// Creates a closed safe attribute.
    #[must_use]
    pub const fn new(key: SafeAttributeKey, value: SafeAttributeValue) -> Self {
        Self { key, value }
    }
    /// Returns the stable key.
    #[must_use]
    pub const fn key(self) -> SafeAttributeKey {
        self.key
    }
    /// Returns the typed scalar value.
    #[must_use]
    pub const fn value(self) -> SafeAttributeValue {
        self.value
    }
}
