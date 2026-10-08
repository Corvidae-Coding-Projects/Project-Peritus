//! Bounded evidence for optional provider observations that could not be trusted.

use peritus_types::Sha256Digest;

/// Optional provider datum that was present but could not be normalized safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OptionalObservationKind {
    /// A configured provider request/response identity header.
    MappedRequestId,
    /// A configured rate-limit ceiling header.
    RateLimitLimit,
    /// A configured rate-limit remaining header.
    RateLimitRemaining,
    /// A configured rate-limit reset header.
    RateLimitReset,
    /// A rate-limit window whose individually parsed fields contradicted each other.
    RateLimitWindow,
    /// Provider usage accounting.
    Usage,
    /// Provider-specific ancillary metadata.
    Ancillary,
}

/// Why an optional provider datum was retained as evidence instead of normalized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OptionalObservationStatus {
    /// Provider bytes were not valid UTF-8 where text was required.
    InvalidEncoding,
    /// The value did not have the required primitive shape.
    InvalidValue,
    /// Individually valid values contradicted each other or prior trusted state.
    Inconsistent,
    /// The provider emitted an observation the negotiated profile did not declare.
    Undeclared,
    /// The value exceeded the selected normalization bound.
    ExceededBound,
    /// The wire value was recognized but could not be represented safely.
    Unrepresentable,
    /// The optional field or event has no portable mapping.
    Unsupported,
}

/// Fixed-size identity and classification for one rejected optional provider datum.
///
/// Raw provider bytes are never retained. Their exact digest and byte count distinguish an
/// unsupported value from a missing value without making response success depend on diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptionalObservation {
    kind: OptionalObservationKind,
    status: OptionalObservationStatus,
    value_digest: Sha256Digest,
    value_bytes: u64,
}

impl OptionalObservation {
    /// Records the exact identity of an optional provider value without retaining the value.
    #[must_use]
    pub fn new(
        kind: OptionalObservationKind,
        status: OptionalObservationStatus,
        value: &[u8],
    ) -> Self {
        Self {
            kind,
            status,
            value_digest: peritus_codec::sha256(value),
            value_bytes: u64::try_from(value.len()).unwrap_or(u64::MAX),
        }
    }

    pub(crate) const fn from_encoded(
        kind: OptionalObservationKind,
        status: OptionalObservationStatus,
        value_digest: Sha256Digest,
        value_bytes: u64,
    ) -> Self {
        Self { kind, status, value_digest, value_bytes }
    }

    /// Returns the optional datum class.
    #[must_use]
    pub const fn kind(self) -> OptionalObservationKind {
        self.kind
    }

    /// Returns why normalization rejected the optional value.
    #[must_use]
    pub const fn status(self) -> OptionalObservationStatus {
        self.status
    }

    /// Returns the digest of the exact rejected value bytes.
    #[must_use]
    pub const fn value_digest(self) -> Sha256Digest {
        self.value_digest
    }

    /// Returns the exact rejected value byte count.
    #[must_use]
    pub const fn value_bytes(self) -> u64 {
        self.value_bytes
    }
}
