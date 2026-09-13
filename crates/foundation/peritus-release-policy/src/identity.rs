//! Nominal release-policy identities.

use crate::{ConstructionError, ConstructionErrorKind};
use vstd::prelude::*;

verus! {

/// Logical predicate for a nominal identity outside the reserved all-zero value.
pub open spec fn nonzero_identifier(bytes: [u8; 16]) -> bool {
    bytes[0] != 0 || bytes[1] != 0 || bytes[2] != 0 || bytes[3] != 0
        || bytes[4] != 0 || bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0
        || bytes[8] != 0 || bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0
        || bytes[12] != 0 || bytes[13] != 0 || bytes[14] != 0 || bytes[15] != 0
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct StableIdentity([u8; 16]);

impl StableIdentity {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { nonzero_identifier(self.0) }

    const fn new(bytes: [u8; 16]) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == nonzero_identifier(bytes),
            match result {
                Ok(value) => value.spec_bytes() == bytes,
                Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroIdentity,
            },
    {
        if nonzero_identifier_exec(bytes) {
            Ok(Self(bytes))
        } else {
            Err(ConstructionError::new(ConstructionErrorKind::ZeroIdentity))
        }
    }

    closed spec fn spec_bytes(&self) -> [u8; 16] { self.0 }

    const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes()
    {
        &self.0
    }
}

const fn nonzero_identifier_exec(bytes: [u8; 16]) -> (nonzero: bool)
    ensures nonzero == nonzero_identifier(bytes)
{
    bytes[0] != 0 || bytes[1] != 0 || bytes[2] != 0 || bytes[3] != 0
        || bytes[4] != 0 || bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0
        || bytes[8] != 0 || bytes[9] != 0 || bytes[10] != 0 || bytes[11] != 0
        || bytes[12] != 0 || bytes[13] != 0 || bytes[14] != 0 || bytes[15] != 0
}

/// Stable identity of one immutable release candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CandidateId(StableIdentity);

impl CandidateId {
    /// Exact admission predicate for a candidate identity.
    pub open spec fn inputs_valid(bytes: [u8; 16]) -> bool { nonzero_identifier(bytes) }

    /// Creates a nonzero candidate identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(bytes),
            match result {
                Ok(value) => value.spec_bytes() == bytes,
                Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroIdentity,
            },
    {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes()
    {
        self.0.as_bytes()
    }

    /// Logical view of the exact candidate-identity bytes.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
}

/// Stable identity of a reviewer, producer, waiver authority, or qualification signer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PrincipalId(StableIdentity);

impl PrincipalId {
    /// Exact admission predicate for a principal identity.
    pub open spec fn inputs_valid(bytes: [u8; 16]) -> bool { nonzero_identifier(bytes) }

    /// Creates a nonzero principal identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(bytes),
            match result {
                Ok(value) => value.spec_bytes() == bytes,
                Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroIdentity,
            },
    {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes()
    {
        self.0.as_bytes()
    }

    /// Logical view of the exact principal-identity bytes.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
}

/// Stable identity of one independent review.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewId(StableIdentity);

impl ReviewId {
    /// Exact admission predicate for a review identity.
    pub open spec fn inputs_valid(bytes: [u8; 16]) -> bool { nonzero_identifier(bytes) }

    /// Creates a nonzero review identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(bytes),
            match result {
                Ok(value) => value.spec_bytes() == bytes,
                Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroIdentity,
            },
    {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes()
    {
        self.0.as_bytes()
    }

    /// Logical view of the exact review-identity bytes.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
}

/// Stable identity of one review finding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FindingId(StableIdentity);

impl FindingId {
    /// Exact admission predicate for a finding identity.
    pub open spec fn inputs_valid(bytes: [u8; 16]) -> bool { nonzero_identifier(bytes) }

    /// Creates a nonzero finding identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value.
    pub const fn new(bytes: [u8; 16]) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(bytes),
            match result {
                Ok(value) => value.spec_bytes() == bytes,
                Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroIdentity,
            },
    {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> (bytes: &[u8; 16])
        ensures *bytes == self.spec_bytes()
    {
        self.0.as_bytes()
    }

    /// Logical view of the exact finding-identity bytes.
    pub closed spec fn spec_bytes(&self) -> [u8; 16] { self.0.spec_bytes() }
}

pub open spec fn principal_ids_match(left: PrincipalId, right: PrincipalId) -> bool {
    crate::candidate::equality::same_bytes_16(left.spec_bytes(), right.spec_bytes())
}

pub open spec fn review_ids_match(left: ReviewId, right: ReviewId) -> bool {
    crate::candidate::equality::same_bytes_16(left.spec_bytes(), right.spec_bytes())
}

pub open spec fn finding_ids_match(left: FindingId, right: FindingId) -> bool {
    crate::candidate::equality::same_bytes_16(left.spec_bytes(), right.spec_bytes())
}

/// Executable equality for exact principal identities.
pub const fn principal_ids_equal(left: PrincipalId, right: PrincipalId) -> (equal: bool)
    ensures equal == principal_ids_match(left, right),
{
    crate::candidate::equality::bytes_16_equal(*left.as_bytes(), *right.as_bytes())
}

/// Executable equality for exact review identities.
pub const fn review_ids_equal(left: ReviewId, right: ReviewId) -> (equal: bool)
    ensures equal == review_ids_match(left, right),
{
    crate::candidate::equality::bytes_16_equal(*left.as_bytes(), *right.as_bytes())
}

/// Executable equality for exact finding identities.
pub const fn finding_ids_equal(left: FindingId, right: FindingId) -> (equal: bool)
    ensures equal == finding_ids_match(left, right),
{
    crate::candidate::equality::bytes_16_equal(*left.as_bytes(), *right.as_bytes())
}

} // verus!
