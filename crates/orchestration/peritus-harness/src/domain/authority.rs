//! Closed descriptive authority vocabulary and non-minting sets.

use crate::domain::{HarnessDomainError, HarnessDomainErrorKind};

/// Intended exposure declared by a harness component.
///
/// These values never create a B1 capability and never authorize an effect.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Authority {
    /// Read bounded assembled context.
    ContextRead = 1,
    /// Read through the C1 workspace boundary.
    WorkspaceRead = 2,
    /// Request mutation through the C1 workspace boundary.
    WorkspaceMutation = 3,
    /// Request bounded process execution.
    ProcessExecution = 4,
    /// Request governed network access.
    NetworkAccess = 5,
    /// Refer to a secret without obtaining its bytes as configuration.
    SecretReference = 6,
    /// Ask for a human approval decision.
    ApprovalRequest = 7,
    /// Observe an acceptance result.
    AcceptanceObservation = 8,
    /// Supply input to a sealed evaluator.
    EvaluationInput = 9,
    /// Propose a production promotion for a separate authority to decide.
    PromotionProposal = 10,
}

impl Authority {
    /// Every defined authority in canonical tag order.
    pub const ALL: [Self; 10] = [
        Self::ContextRead,
        Self::WorkspaceRead,
        Self::WorkspaceMutation,
        Self::ProcessExecution,
        Self::NetworkAccess,
        Self::SecretReference,
        Self::ApprovalRequest,
        Self::AcceptanceObservation,
        Self::EvaluationInput,
        Self::PromotionProposal,
    ];

    /// Returns the immutable schema-v1 numeric tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self as u8
    }
}

/// Canonical closed authority set represented without unknown bits.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AuthoritySet(u16);

impl AuthoritySet {
    const KNOWN_BITS: u16 = (1 << Authority::ALL.len()) - 1;

    /// Returns an empty authority declaration.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Constructs a set from authorities in strict canonical order.
    ///
    /// # Errors
    ///
    /// Rejects duplicate or descending tags instead of silently canonicalizing input.
    pub fn new(authorities: Vec<Authority>) -> Result<Self, HarnessDomainError> {
        let mut previous = None;
        let mut bits = 0_u16;
        for authority in authorities {
            if previous.is_some_and(|value| value >= authority) {
                return Err(HarnessDomainError::detail(
                    HarnessDomainErrorKind::NonCanonicalOrder,
                    "authority set is not in strict canonical order",
                ));
            }
            bits |= Self::bit(authority);
            previous = Some(authority);
        }
        Ok(Self(bits))
    }

    pub(crate) const fn from_known_bits(bits: u16) -> Self {
        Self(bits & Self::KNOWN_BITS)
    }

    pub(crate) fn from_canonical_bits(bits: u16) -> Result<Self, HarnessDomainError> {
        if bits & !Self::KNOWN_BITS != 0 {
            Err(HarnessDomainError::detail(
                HarnessDomainErrorKind::InvalidCanonicalEncoding,
                "authority set contains an unknown bit",
            ))
        } else {
            Ok(Self(bits))
        }
    }

    const fn bit(authority: Authority) -> u16 {
        1_u16 << (authority.tag() - 1)
    }

    /// Returns whether the set contains one descriptive authority.
    #[must_use]
    pub const fn contains(self, authority: Authority) -> bool {
        self.0 & Self::bit(authority) != 0
    }

    /// Returns whether every authority is also present in `ceiling`.
    #[must_use]
    pub const fn is_subset_of(self, ceiling: Self) -> bool {
        self.0 & !ceiling.0 == 0
    }

    /// Returns the union of two known authority sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self::from_known_bits(self.0 | other.0)
    }

    /// Returns whether no authority is declared.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns the canonical bit representation for canonical encoding and diagnostics.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Returns the number of declared authority tags.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }
}
