//! Exact release-version identity.

use crate::ConstructionError;
use peritus_types::Sha256Digest;

#[doc = " Exact release version, including a digest of pre-release/build text."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReleaseVersion {
    major: u16,
    minor: u16,
    patch: u16,
    descriptor_digest: Sha256Digest,
}
impl ReleaseVersion {
    #[doc = " Creates an exact semantic release version."]
    #[doc = ""]
    #[doc = " `descriptor_digest` binds the canonical complete version string, including any pre-release"]
    #[doc = " and build metadata."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`crate::ConstructionErrorKind::ZeroDigest`] for a placeholder digest."]
    pub fn new(
        major: u16,
        minor: u16,
        patch: u16,
        descriptor_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        crate::validation::require_digest(descriptor_digest)?;
        Ok(Self { major, minor, patch, descriptor_digest })
    }
    #[doc = " Returns the major component."]
    #[must_use]
    pub const fn major(&self) -> u16 {
        self.major
    }
    #[doc = " Returns the minor component."]
    #[must_use]
    pub const fn minor(&self) -> u16 {
        self.minor
    }
    #[doc = " Returns the patch component."]
    #[must_use]
    pub const fn patch(&self) -> u16 {
        self.patch
    }
    #[doc = " Returns the digest of the canonical complete version string."]
    #[must_use]
    pub const fn descriptor_digest(&self) -> Sha256Digest {
        self.descriptor_digest
    }
}
