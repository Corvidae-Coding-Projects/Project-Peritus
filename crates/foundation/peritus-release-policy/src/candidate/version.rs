//! Exact release-version identity.

use crate::ConstructionError;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Exact release version, including a digest of pre-release/build text.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReleaseVersion {
    major: u16,
    minor: u16,
    patch: u16,
    descriptor_digest: Sha256Digest,
}

impl ReleaseVersion {
    /// Exact admission predicate for a release-version identity.
    pub open spec fn inputs_valid(descriptor_digest: Sha256Digest) -> bool {
        crate::validation::spec_digest_nonzero(descriptor_digest)
    }

    /// Creates an exact semantic release version.
    ///
    /// `descriptor_digest` binds the canonical complete version string, including any pre-release
    /// and build metadata.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ConstructionErrorKind::ZeroDigest`] for a placeholder digest.
    pub fn new(
        major: u16,
        minor: u16,
        patch: u16,
        descriptor_digest: Sha256Digest,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(descriptor_digest),
            match result {
                Ok(value) => value.spec_major() == major
                    && value.spec_minor() == minor
                    && value.spec_patch() == patch
                    && value.spec_descriptor_digest() == descriptor_digest,
                Err(error) => error.spec_kind() == crate::ConstructionErrorKind::ZeroDigest,
            },
    {
        crate::validation::require_digest(descriptor_digest)?;
        Ok(Self { major, minor, patch, descriptor_digest })
    }

    /// Returns the major component.
    #[must_use]
    pub const fn major(&self) -> (major: u16) ensures major == self.spec_major() { self.major }

    /// Returns the minor component.
    #[must_use]
    pub const fn minor(&self) -> (minor: u16) ensures minor == self.spec_minor() { self.minor }

    /// Returns the patch component.
    #[must_use]
    pub const fn patch(&self) -> (patch: u16) ensures patch == self.spec_patch() { self.patch }

    /// Returns the digest of the canonical complete version string.
    #[must_use]
    pub const fn descriptor_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_descriptor_digest()
    {
        self.descriptor_digest
    }

    /// Logical view of the major component.
    pub closed spec fn spec_major(&self) -> u16 { self.major }

    /// Logical view of the minor component.
    pub closed spec fn spec_minor(&self) -> u16 { self.minor }

    /// Logical view of the patch component.
    pub closed spec fn spec_patch(&self) -> u16 { self.patch }

    /// Logical view of the canonical complete-version digest.
    pub closed spec fn spec_descriptor_digest(&self) -> Sha256Digest {
        self.descriptor_digest
    }
}

} // verus!
