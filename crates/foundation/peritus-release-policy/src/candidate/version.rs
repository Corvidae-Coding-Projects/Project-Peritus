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
    ) -> Result<Self, ConstructionError> {
        super::require_digest(descriptor_digest)?;
        Ok(Self { major, minor, patch, descriptor_digest })
    }

    /// Returns the major component.
    #[must_use]
    pub const fn major(&self) -> u16 { self.major }

    /// Returns the minor component.
    #[must_use]
    pub const fn minor(&self) -> u16 { self.minor }

    /// Returns the patch component.
    #[must_use]
    pub const fn patch(&self) -> u16 { self.patch }

    /// Returns the digest of the canonical complete version string.
    #[must_use]
    pub const fn descriptor_digest(&self) -> Sha256Digest { self.descriptor_digest }
}

} // verus!
