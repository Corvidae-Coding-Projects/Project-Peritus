//! Canonical release artifact inventory.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::{ArtifactError, ArtifactErrorCode, ReleaseBinding, Sha256Digest, digest_bytes};

/// Maximum artifacts retained in one release inventory.
pub const MAX_ARTIFACTS: usize = 4_096;

/// Canonical release-root-relative path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ReleasePath(String);

impl ReleasePath {
    /// Validates a normalized forward-slash relative path.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for absolute, escaping, empty, noncanonical, or oversized paths.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactError> {
        let value = value.into();
        let path = Path::new(&value);
        let valid = !value.is_empty()
            && value.len() <= 512
            && !value.contains(char::from(92))
            && !value.contains("//")
            && !value.ends_with('/')
            && !path.is_absolute()
            && path.components().all(|component| matches!(component, Component::Normal(_)))
            && value.bytes().all(|byte| byte.is_ascii_graphic());
        if !valid {
            return Err(ArtifactError::new(
                ArtifactErrorCode::InvalidValue,
                "validate release path",
                "path must be normalized visible ASCII below the release root",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the normalized path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated artifact media type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct MediaType(String);

impl MediaType {
    /// Validates an explicit media type without parameters.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for an empty, oversized, whitespace-bearing, or slashless value.
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 120
            || !value.contains('/')
            || !value.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(ArtifactError::new(
                ArtifactErrorCode::InvalidValue,
                "validate artifact media type",
                "media type must contain 1 through 120 visible ASCII bytes and a slash",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the validated media type.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Closed role of an artifact in a release bundle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactRole {
    /// Executable delivered to operators.
    Executable,
    /// Native package or archive.
    Distribution,
    /// Installer, upgrader, or uninstaller program.
    LifecycleTool,
    /// Canonical release manifest.
    Manifest,
    /// SPDX software bill of materials.
    Sbom,
    /// SLSA-style provenance statement.
    Provenance,
    /// Detached public signature.
    Signature,
    /// License notice document.
    LicenseNotice,
    /// Migration documentation.
    MigrationGuide,
    /// Backup documentation.
    BackupGuide,
    /// Restore documentation.
    RestoreGuide,
    /// Rollback documentation.
    RollbackGuide,
    /// Qualification or audit evidence retained outside runtime state.
    QualificationEvidence,
}

/// One immutable release artifact observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArtifactEntry {
    path: ReleasePath,
    byte_length: u64,
    sha256: Sha256Digest,
    media_type: MediaType,
    roles: Vec<ArtifactRole>,
}

impl ArtifactEntry {
    /// Creates a content-addressed artifact entry with one or more canonical roles.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] when no role is supplied or a role is duplicated.
    pub fn new(
        path: ReleasePath,
        byte_length: u64,
        sha256: Sha256Digest,
        media_type: MediaType,
        mut roles: Vec<ArtifactRole>,
    ) -> Result<Self, ArtifactError> {
        if roles.is_empty() {
            return Err(ArtifactError::new(
                ArtifactErrorCode::MissingEvidence,
                "create artifact entry",
                "artifact must declare at least one release role",
            ));
        }
        roles.sort_unstable();
        if roles.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ArtifactError::new(
                ArtifactErrorCode::Duplicate,
                "create artifact entry",
                "artifact repeats a release role",
            ));
        }
        Ok(Self { path, byte_length, sha256, media_type, roles })
    }

    /// Observes exact bytes as an artifact entry.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] when the byte count cannot fit in `u64` or roles are absent.
    pub fn from_bytes(
        path: ReleasePath,
        media_type: MediaType,
        roles: Vec<ArtifactRole>,
        bytes: &[u8],
    ) -> Result<Self, ArtifactError> {
        let byte_length = u64::try_from(bytes.len()).map_err(|_| {
            ArtifactError::new(
                ArtifactErrorCode::BoundExceeded,
                "observe artifact bytes",
                "artifact length cannot be represented",
            )
        })?;
        Self::new(path, byte_length, digest_bytes(bytes), media_type, roles)
    }

    /// Returns the canonical release-relative path.
    #[must_use]
    pub const fn path(&self) -> &ReleasePath {
        &self.path
    }

    /// Returns the exact byte length.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }

    /// Returns the exact SHA-256 identity.
    #[must_use]
    pub const fn sha256(&self) -> Sha256Digest {
        self.sha256
    }

    /// Returns the declared media type.
    #[must_use]
    pub const fn media_type(&self) -> &MediaType {
        &self.media_type
    }

    /// Returns roles in canonical order.
    #[must_use]
    pub fn roles(&self) -> &[ArtifactRole] {
        &self.roles
    }
}

/// Complete path-sorted inventory for one exact candidate and platform.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArtifactInventory {
    schema_version: u32,
    binding: ReleaseBinding,
    artifacts: Vec<ArtifactEntry>,
}

impl ArtifactInventory {
    /// Validates and canonically orders a nonempty artifact inventory.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] for an empty or oversized inventory or duplicate paths.
    pub fn new(
        binding: ReleaseBinding,
        mut artifacts: Vec<ArtifactEntry>,
    ) -> Result<Self, ArtifactError> {
        if artifacts.is_empty() || artifacts.len() > MAX_ARTIFACTS {
            return Err(ArtifactError::new(
                ArtifactErrorCode::BoundExceeded,
                "create artifact inventory",
                "inventory must contain 1 through 4096 artifacts",
            ));
        }
        artifacts.sort_by(|left, right| left.path.cmp(&right.path));
        if let Some(pair) = artifacts.windows(2).find(|pair| pair[0].path == pair[1].path) {
            return Err(ArtifactError::new(
                ArtifactErrorCode::Duplicate,
                "create artifact inventory",
                format!("duplicate artifact path {}", pair[0].path.as_str()),
            ));
        }
        Ok(Self { schema_version: 1, binding, artifacts })
    }

    /// Returns the exact release binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.binding
    }

    /// Returns entries in canonical path order.
    #[must_use]
    pub fn artifacts(&self) -> &[ArtifactEntry] {
        &self.artifacts
    }

    /// Serializes deterministic compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ArtifactError> {
        serde_json::to_vec(self)
            .map_err(|source| ArtifactError::serialization("serialize artifact inventory", source))
    }

    /// Returns the SHA-256 of deterministic compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, ArtifactError> {
        self.canonical_json().map(|bytes| digest_bytes(&bytes))
    }

    /// Finds one entry by its canonical path.
    #[must_use]
    pub fn get(&self, path: &ReleasePath) -> Option<&ArtifactEntry> {
        self.artifacts
            .binary_search_by(|entry| entry.path.cmp(path))
            .ok()
            .and_then(|index| self.artifacts.get(index))
    }
}

#[cfg(test)]
mod tests {
    use super::{ArtifactEntry, ArtifactInventory, ArtifactRole, MediaType, ReleasePath};
    use crate::{
        CandidateCommit, PlatformTriple, ReleaseBinding, ReleaseVersion, ToolchainId, digest_bytes,
    };

    fn binding() -> ReleaseBinding {
        ReleaseBinding::new(
            CandidateCommit::new("a".repeat(40)).expect("commit"),
            ReleaseVersion::new("1.0.0").expect("version"),
            ToolchainId::new("rust-1.97.1").expect("toolchain"),
            PlatformTriple::new("x86_64-unknown-linux-gnu@6.6").expect("platform"),
            digest_bytes(b"tree"),
        )
    }

    fn entry(path: &str, bytes: &[u8]) -> ArtifactEntry {
        ArtifactEntry::from_bytes(
            ReleasePath::new(path).expect("path"),
            MediaType::new("application/octet-stream").expect("media type"),
            vec![ArtifactRole::Distribution],
            bytes,
        )
        .expect("entry")
    }

    #[test]
    fn inventory_is_path_sorted_and_deterministic() {
        let inventory = ArtifactInventory::new(
            binding(),
            vec![entry("z/package", b"z"), entry("a/package", b"a")],
        )
        .expect("inventory");
        assert_eq!(inventory.artifacts()[0].path().as_str(), "a/package");
        assert_eq!(inventory.digest().expect("digest"), inventory.digest().expect("digest"));
    }

    #[test]
    fn release_path_rejects_parent_escape() {
        assert!(ReleasePath::new("../outside").is_err());
    }
}
