//! Typed checked-construction failures.

#[doc = " Stable category for rejected release-policy input."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConstructionErrorKind {
    #[doc = " A nominal identity used the reserved all-zero value."]
    ZeroIdentity,
    #[doc = " A content digest used the reserved all-zero placeholder value."]
    ZeroDigest,
    #[doc = " A revision or sequence that must be positive was zero."]
    ZeroRevision,
    #[doc = " An observation expires before it was produced."]
    InvalidValidityInterval,
    #[doc = " The platform matrix omitted or mislabeled a tier-one operating system."]
    InvalidPlatformMatrix,
    #[doc = " An evidence collection exceeded its explicit bound."]
    CollectionLimitExceeded,
}
#[doc = " Checked-construction error with a stable machine code."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstructionError {
    kind: ConstructionErrorKind,
}
impl ConstructionError {
    pub(crate) const fn new(kind: ConstructionErrorKind) -> Self {
        Self { kind }
    }
    #[doc = " Returns the stable error category."]
    #[must_use]
    pub const fn kind(&self) -> ConstructionErrorKind {
        self.kind
    }
    #[doc = " Returns the stable diagnostic code."]
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
