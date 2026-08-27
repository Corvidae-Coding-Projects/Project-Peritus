//! Exact candidate, version, toolchain, and platform bindings.

use serde::Serialize;

use crate::{ArtifactError, ArtifactErrorCode, BoundedId, Sha256Digest, digest_bytes};

/// Exact Git object identity for a release candidate.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CandidateCommit(String);

impl CandidateCommit {
    /// Validates a full lowercase SHA-1 or SHA-256 Git object identity.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for abbreviated, uppercase, or nonhexadecimal input.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactError> {
        let value = value.into();
        if !matches!(value.len(), 40 | 64)
            || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(invalid_binding(
                "candidate commit must be a full 40 or 64 byte lowercase hexadecimal object ID",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the exact object identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated semantic release version without a leading `v`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ReleaseVersion(String);

impl ReleaseVersion {
    /// Validates a bounded semantic version.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] unless the core contains three canonical numeric components and
    /// optional prerelease/build identifiers use the `SemVer` portable alphabet.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactError> {
        let value = value.into();
        if value.is_empty() || value.len() > 96 || !valid_semver(&value) {
            return Err(invalid_binding("release version is not canonical SemVer"));
        }
        Ok(Self(value))
    }

    /// Borrows the exact semantic version.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Exact compiler and verification toolchain identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ToolchainId(BoundedId);

impl ToolchainId {
    /// Validates an exact, portable toolchain identity.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] when the identity is not portable and bounded.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactError> {
        BoundedId::new(value).map(Self)
    }

    /// Borrows the toolchain identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Exact target triple and native platform revision under qualification.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PlatformTriple(String);

impl PlatformTriple {
    /// Validates a bounded platform descriptor such as a target triple plus native release.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for empty, oversized, whitespace, or control-bearing input.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 160
            || !value.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(invalid_binding(
                "platform descriptor must contain 1 through 160 visible ASCII bytes",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the exact platform descriptor.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Immutable identity shared by every release artifact and qualification observation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ReleaseBinding {
    candidate_commit: CandidateCommit,
    version: ReleaseVersion,
    toolchain: ToolchainId,
    platform: PlatformTriple,
    source_tree_digest: Sha256Digest,
}

impl ReleaseBinding {
    /// Creates an exact candidate binding from already validated parts.
    #[must_use]
    pub const fn new(
        candidate_commit: CandidateCommit,
        version: ReleaseVersion,
        toolchain: ToolchainId,
        platform: PlatformTriple,
        source_tree_digest: Sha256Digest,
    ) -> Self {
        Self { candidate_commit, version, toolchain, platform, source_tree_digest }
    }

    /// Returns the exact candidate commit.
    #[must_use]
    pub const fn candidate_commit(&self) -> &CandidateCommit {
        &self.candidate_commit
    }

    /// Returns the release version.
    #[must_use]
    pub const fn version(&self) -> &ReleaseVersion {
        &self.version
    }

    /// Returns the exact toolchain identity.
    #[must_use]
    pub const fn toolchain(&self) -> &ToolchainId {
        &self.toolchain
    }

    /// Returns the exact target platform descriptor.
    #[must_use]
    pub const fn platform(&self) -> &PlatformTriple {
        &self.platform
    }

    /// Returns the independently computed source-tree digest.
    #[must_use]
    pub const fn source_tree_digest(&self) -> Sha256Digest {
        self.source_tree_digest
    }

    /// Returns a deterministic digest of the complete binding.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if canonical serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, ArtifactError> {
        serde_json::to_vec(self)
            .map(|bytes| digest_bytes(&bytes))
            .map_err(|source| ArtifactError::serialization("serialize release binding", source))
    }
}

fn invalid_binding(detail: &'static str) -> ArtifactError {
    ArtifactError::new(ArtifactErrorCode::InvalidValue, "validate release binding", detail)
}

fn valid_semver(value: &str) -> bool {
    let without_build = value
        .split_once('+')
        .map_or(value, |(left, right)| if valid_identifiers(right, false) { left } else { "" });
    if without_build.is_empty() {
        return false;
    }
    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(left, right)| (left, Some(right)));
    let mut components = core.split('.');
    let core_valid =
        (0..3).all(|_| components.next().is_some_and(valid_numeric)) && components.next().is_none();
    core_valid && prerelease.is_none_or(|part| valid_identifiers(part, true))
}

fn valid_numeric(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn valid_identifiers(value: &str, forbid_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (!forbid_numeric_leading_zero
                    || !identifier.bytes().all(|byte| byte.is_ascii_digit())
                    || valid_numeric(identifier))
        })
}

#[cfg(test)]
mod tests {
    use super::{CandidateCommit, ReleaseVersion};

    #[test]
    fn commit_requires_full_lowercase_identity() {
        assert!(CandidateCommit::new("a".repeat(40)).is_ok());
        assert!(CandidateCommit::new("A".repeat(40)).is_err());
        assert!(CandidateCommit::new("abc123").is_err());
    }

    #[test]
    fn version_rejects_leading_zero() {
        assert!(ReleaseVersion::new("1.0.0-rc.1+build.7").is_ok());
        assert!(ReleaseVersion::new("01.0.0").is_err());
    }
}
