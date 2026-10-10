//! Nominal release-policy identities.

use crate::{ConstructionError, ConstructionErrorKind};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct StableIdentity([u8; 16]);
impl StableIdentity {
    const fn new(bytes: [u8; 16]) -> Result<Self, ConstructionError> {
        if nonzero_identifier_exec(bytes) {
            Ok(Self(bytes))
        } else {
            Err(ConstructionError::new(ConstructionErrorKind::ZeroIdentity))
        }
    }
    const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}
const fn nonzero_identifier_exec(bytes: [u8; 16]) -> bool {
    bytes[0] != 0
        || bytes[1] != 0
        || bytes[2] != 0
        || bytes[3] != 0
        || bytes[4] != 0
        || bytes[5] != 0
        || bytes[6] != 0
        || bytes[7] != 0
        || bytes[8] != 0
        || bytes[9] != 0
        || bytes[10] != 0
        || bytes[11] != 0
        || bytes[12] != 0
        || bytes[13] != 0
        || bytes[14] != 0
        || bytes[15] != 0
}
#[doc = " Stable identity of one immutable release candidate."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CandidateId(StableIdentity);
impl CandidateId {
    #[doc = " Creates a nonzero candidate identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ConstructionError> {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Returns the exact identity bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}
#[doc = " Stable identity of a reviewer, producer, waiver authority, or qualification signer."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PrincipalId(StableIdentity);
impl PrincipalId {
    #[doc = " Creates a nonzero principal identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ConstructionError> {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Returns the exact identity bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}
#[doc = " Stable identity of one independent review."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewId(StableIdentity);
impl ReviewId {
    #[doc = " Creates a nonzero review identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ConstructionError> {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Returns the exact identity bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}
#[doc = " Stable identity of one review finding."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FindingId(StableIdentity);
impl FindingId {
    #[doc = " Creates a nonzero finding identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ConstructionErrorKind::ZeroIdentity`] for the reserved zero value."]
    pub const fn new(bytes: [u8; 16]) -> Result<Self, ConstructionError> {
        match StableIdentity::new(bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Returns the exact identity bytes."]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}
#[doc = " Executable equality for exact principal identities."]
pub const fn principal_ids_equal(left: PrincipalId, right: PrincipalId) -> bool {
    crate::candidate::equality::bytes_16_equal(*left.as_bytes(), *right.as_bytes())
}
#[doc = " Executable equality for exact review identities."]
pub const fn review_ids_equal(left: ReviewId, right: ReviewId) -> bool {
    crate::candidate::equality::bytes_16_equal(*left.as_bytes(), *right.as_bytes())
}
#[doc = " Executable equality for exact finding identities."]
pub const fn finding_ids_equal(left: FindingId, right: FindingId) -> bool {
    crate::candidate::equality::bytes_16_equal(*left.as_bytes(), *right.as_bytes())
}
