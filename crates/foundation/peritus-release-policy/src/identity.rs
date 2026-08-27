//! Nominal release-policy identities.

use crate::{ConstructionError, ConstructionErrorKind};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn nonzero_identifier(bytes: [u8; 16]) -> bool {
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
        ensures result.is_ok() == nonzero_identifier(bytes)
    {
        if nonzero_identifier_exec(bytes) {
            Ok(Self(bytes))
        } else {
            Err(ConstructionError::new(ConstructionErrorKind::ZeroIdentity))
        }
    }

    const fn as_bytes(&self) -> &[u8; 16] { &self.0 }
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
    /// Creates a nonzero candidate identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ConstructionError> {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { self.0.as_bytes() }
}

/// Stable identity of a reviewer, producer, waiver authority, or qualification signer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PrincipalId(StableIdentity);

impl PrincipalId {
    /// Creates a nonzero principal identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ConstructionError> {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { self.0.as_bytes() }
}

/// Stable identity of one independent review.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewId(StableIdentity);

impl ReviewId {
    /// Creates a nonzero review identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ConstructionError> {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { self.0.as_bytes() }
}

/// Stable identity of one review finding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FindingId(StableIdentity);

impl FindingId {
    /// Creates a nonzero finding identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ConstructionError> {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] { self.0.as_bytes() }
}

} // verus!
