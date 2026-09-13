//! Typed checked-construction failures.

use vstd::prelude::*;

verus! {

/// Stable category for rejected release-policy input.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConstructionErrorKind {
    /// A nominal identity used the reserved all-zero value.
    ZeroIdentity,
    /// A content digest used the reserved all-zero placeholder value.
    ZeroDigest,
    /// A revision or sequence that must be positive was zero.
    ZeroRevision,
    /// An observation expires before it was produced.
    InvalidValidityInterval,
    /// The platform matrix omitted or mislabeled a tier-one operating system.
    InvalidPlatformMatrix,
    /// An evidence collection exceeded its explicit bound.
    CollectionLimitExceeded,
}

/// Checked-construction error with a stable machine code.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstructionError {
    kind: ConstructionErrorKind,
}

impl ConstructionError {
    pub(crate) const fn new(kind: ConstructionErrorKind) -> Self { Self { kind } }

    /// Returns the stable error category.
    #[must_use]
    pub const fn kind(&self) -> ConstructionErrorKind { self.kind }

    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self.kind {
            ConstructionErrorKind::ZeroIdentity => "H4_INPUT_ZERO_IDENTITY",
            ConstructionErrorKind::ZeroDigest => "H4_INPUT_ZERO_DIGEST",
            ConstructionErrorKind::ZeroRevision => "H4_INPUT_ZERO_REVISION",
            ConstructionErrorKind::InvalidValidityInterval => "H4_INPUT_INVALID_VALIDITY",
            ConstructionErrorKind::InvalidPlatformMatrix => "H4_INPUT_PLATFORM_MATRIX",
            ConstructionErrorKind::CollectionLimitExceeded => "H4_INPUT_COLLECTION_LIMIT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_kind_has_a_stable_h4_code() {
        let kinds = [
            ConstructionErrorKind::ZeroIdentity,
            ConstructionErrorKind::ZeroDigest,
            ConstructionErrorKind::ZeroRevision,
            ConstructionErrorKind::InvalidValidityInterval,
            ConstructionErrorKind::InvalidPlatformMatrix,
            ConstructionErrorKind::CollectionLimitExceeded,
        ];
        for kind in kinds {
            assert!(ConstructionError::new(kind).code().starts_with("H4_INPUT_"));
        }
    }
}

} // verus!
