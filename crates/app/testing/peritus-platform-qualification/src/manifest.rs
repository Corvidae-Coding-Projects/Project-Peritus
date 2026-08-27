//! Deterministic package manifests and checksum material.

use std::collections::BTreeSet;

use crate::{
    Architecture, ArtifactDigest, Platform, QualificationError, QualificationErrorCode,
    QualificationRecovery, Sha256Digest, digest_bytes,
};

mod wire;

use wire::ManifestWire;

const MANIFEST_SCHEMA: u16 = 1;
const MAX_ARTIFACTS: usize = 128;
const MAX_MANIFEST_BYTES: usize = 256 * 1024;

/// Validated release-version text retained exactly in package evidence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageVersion(String);

impl PackageVersion {
    /// Validates a bounded ASCII release version.
    ///
    /// # Errors
    ///
    /// Rejects empty, control-bearing, whitespace-bearing, or path-like values.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_')
            })
        {
            return Err(manifest_error("release version is not bounded canonical ASCII"));
        }
        Ok(Self(value))
    }

    /// Borrows the exact release version.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated package-relative artifact path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RelativePackagePath(String);

impl RelativePackagePath {
    /// Validates a slash-separated relative package path.
    ///
    /// # Errors
    ///
    /// Rejects absolute, traversal-bearing, control-bearing, or empty paths.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 1_024
            || value.starts_with('/')
            || value.contains('\\')
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_')
            })
            || value
                .split('/')
                .any(|component| component.is_empty() || matches!(component, "." | ".."))
        {
            return Err(manifest_error("artifact path is not canonical package-relative text"));
        }
        Ok(Self(value))
    }

    /// Borrows the canonical path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Closed release-artifact role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtifactRole {
    /// G0 `peritusd` application daemon.
    Daemon,
    /// G1 `peritus` automation client.
    Cli,
    /// G2 `peritus-tui` interactive client.
    Tui,
    /// Target-native C3 sandbox helper.
    SandboxHelper,
    /// Native user-service/autostart definition.
    ServiceDefinition,
    /// Mechanical installer script.
    Installer,
    /// Mechanical uninstaller script.
    Uninstaller,
    /// Mechanical upgrade/rollback script.
    Upgrader,
}

impl ArtifactRole {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Daemon => "daemon",
            Self::Cli => "cli",
            Self::Tui => "tui",
            Self::SandboxHelper => "sandbox-helper",
            Self::ServiceDefinition => "service-definition",
            Self::Installer => "installer",
            Self::Uninstaller => "uninstaller",
            Self::Upgrader => "upgrader",
        }
    }

    fn parse(value: &str) -> Result<Self, QualificationError> {
        match value {
            "daemon" => Ok(Self::Daemon),
            "cli" => Ok(Self::Cli),
            "tui" => Ok(Self::Tui),
            "sandbox-helper" => Ok(Self::SandboxHelper),
            "service-definition" => Ok(Self::ServiceDefinition),
            "installer" => Ok(Self::Installer),
            "uninstaller" => Ok(Self::Uninstaller),
            "upgrader" => Ok(Self::Upgrader),
            _ => Err(manifest_error("artifact role is not recognized")),
        }
    }

    const fn is_executable(self) -> bool {
        !matches!(self, Self::ServiceDefinition)
    }
}

/// One checksummed package artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestArtifact {
    role: ArtifactRole,
    path: RelativePackagePath,
    digest: ArtifactDigest,
    executable: bool,
}

impl ManifestArtifact {
    /// Creates an exact artifact declaration.
    ///
    /// # Errors
    ///
    /// Rejects executable-bit drift from the role contract or empty executable artifacts.
    pub fn new(
        role: ArtifactRole,
        path: RelativePackagePath,
        digest: ArtifactDigest,
        executable: bool,
    ) -> Result<Self, QualificationError> {
        if executable != role.is_executable() {
            return Err(manifest_error("artifact executable flag differs from its role"));
        }
        if executable && digest.byte_length() == 0 {
            return Err(manifest_error("executable package artifact is empty"));
        }
        Ok(Self { role, path, digest, executable })
    }

    /// Returns the closed artifact role.
    #[must_use]
    pub const fn role(&self) -> ArtifactRole {
        self.role
    }

    /// Borrows the package-relative path.
    #[must_use]
    pub const fn path(&self) -> &RelativePackagePath {
        &self.path
    }

    /// Returns the exact artifact digest.
    #[must_use]
    pub const fn digest(&self) -> ArtifactDigest {
        self.digest
    }

    /// Reports whether installation grants execute permission.
    #[must_use]
    pub const fn executable(&self) -> bool {
        self.executable
    }

    /// Returns one deterministic `SHA256SUMS` line.
    #[must_use]
    pub fn checksum_line(&self) -> String {
        format!("{}  {}", self.digest.sha256(), self.path.as_str())
    }
}

/// Complete checksummed package manifest in canonical artifact-path order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageManifest {
    release: PackageVersion,
    platform: Platform,
    architecture: Architecture,
    layout_digest: Sha256Digest,
    artifacts: Vec<ManifestArtifact>,
    canonical: Vec<u8>,
    digest: Sha256Digest,
}

impl PackageManifest {
    /// Constructs and validates a deterministic manifest.
    ///
    /// # Errors
    ///
    /// Rejects missing application binaries/helper/service assets, duplicate paths or roles,
    /// oversized manifests, and noncanonical artifact ordering inputs.
    pub fn new(
        release: PackageVersion,
        platform: Platform,
        architecture: Architecture,
        layout_digest: Sha256Digest,
        mut artifacts: Vec<ManifestArtifact>,
    ) -> Result<Self, QualificationError> {
        if artifacts.len() > MAX_ARTIFACTS {
            return Err(manifest_error("package artifact count exceeds the H2 bound"));
        }
        artifacts.sort_by(|left, right| left.path.cmp(&right.path));
        if artifacts.windows(2).any(|pair| pair[0].path == pair[1].path) {
            return Err(manifest_error("package manifest repeats an artifact path"));
        }
        let roles = artifacts.iter().map(ManifestArtifact::role).collect::<BTreeSet<_>>();
        for required in [
            ArtifactRole::Daemon,
            ArtifactRole::Cli,
            ArtifactRole::Tui,
            ArtifactRole::SandboxHelper,
            ArtifactRole::ServiceDefinition,
            ArtifactRole::Installer,
            ArtifactRole::Uninstaller,
            ArtifactRole::Upgrader,
        ] {
            if !roles.contains(&required) {
                return Err(manifest_error("package manifest omits a required artifact role"));
            }
        }
        for singleton in [
            ArtifactRole::Daemon,
            ArtifactRole::Cli,
            ArtifactRole::Tui,
            ArtifactRole::SandboxHelper,
            ArtifactRole::ServiceDefinition,
        ] {
            if artifacts.iter().filter(|artifact| artifact.role == singleton).count() != 1 {
                return Err(manifest_error("singleton package artifact role is repeated"));
            }
        }
        let canonical = render(&release, platform, architecture, layout_digest, &artifacts);
        if canonical.len() > MAX_MANIFEST_BYTES {
            return Err(manifest_error("canonical package manifest exceeds the H2 byte bound"));
        }
        let digest = digest_bytes(&canonical).sha256();
        Ok(Self { release, platform, architecture, layout_digest, artifacts, canonical, digest })
    }

    /// Parses the deterministic TOML representation and revalidates every contract.
    ///
    /// # Errors
    ///
    /// Rejects malformed, unknown-field, noncanonical, or digest-invalid manifests.
    pub fn parse(bytes: &[u8]) -> Result<Self, QualificationError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(manifest_error("package manifest exceeds the H2 byte bound"));
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| manifest_error("package manifest is not UTF-8"))?;
        let wire: ManifestWire = toml::from_str(text)
            .map_err(|_| manifest_error("package manifest is not strict H2 TOML"))?;
        if wire.schema != MANIFEST_SCHEMA {
            return Err(manifest_error("package manifest schema is unsupported"));
        }
        let release = PackageVersion::new(wire.release)?;
        let platform = Platform::parse(&wire.platform)?;
        let architecture = Architecture::parse(&wire.architecture)?;
        let layout_digest = Sha256Digest::from_hex(&wire.layout_sha256)?;
        let artifacts = wire
            .artifact
            .into_iter()
            .map(|artifact| {
                ManifestArtifact::new(
                    ArtifactRole::parse(&artifact.role)?,
                    RelativePackagePath::new(artifact.path)?,
                    ArtifactDigest::new(artifact.bytes, Sha256Digest::from_hex(&artifact.sha256)?),
                    artifact.executable,
                )
            })
            .collect::<Result<Vec<_>, QualificationError>>()?;
        let manifest = Self::new(release, platform, architecture, layout_digest, artifacts)?;
        if manifest.canonical != bytes {
            return Err(manifest_error("package manifest bytes are not canonical"));
        }
        Ok(manifest)
    }

    /// Borrows the release version.
    #[must_use]
    pub const fn release(&self) -> &PackageVersion {
        &self.release
    }

    /// Returns the target platform.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Returns the target architecture.
    #[must_use]
    pub const fn architecture(&self) -> Architecture {
        self.architecture
    }

    /// Returns the bound release-layout digest.
    #[must_use]
    pub const fn layout_digest(&self) -> Sha256Digest {
        self.layout_digest
    }

    /// Borrows artifact declarations in canonical path order.
    #[must_use]
    pub fn artifacts(&self) -> &[ManifestArtifact] {
        &self.artifacts
    }

    /// Borrows canonical TOML bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    /// Returns the exact canonical manifest digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Renders all artifact checksums in canonical path order.
    #[must_use]
    pub fn checksums(&self) -> String {
        let mut output = self
            .artifacts
            .iter()
            .map(ManifestArtifact::checksum_line)
            .collect::<Vec<_>>()
            .join("\n");
        output.push('\n');
        output
    }
}

fn render(
    release: &PackageVersion,
    platform: Platform,
    architecture: Architecture,
    layout_digest: Sha256Digest,
    artifacts: &[ManifestArtifact],
) -> Vec<u8> {
    let mut output = format!(
        "schema = {MANIFEST_SCHEMA}\nrelease = \"{}\"\nplatform = \"{}\"\narchitecture = \"{}\"\nlayout_sha256 = \"{}\"\n",
        release.as_str(),
        platform.as_str(),
        architecture.as_str(),
        layout_digest,
    );
    for artifact in artifacts {
        use core::fmt::Write as _;
        write!(
            &mut output,
            "\n[[artifact]]\nrole = \"{}\"\npath = \"{}\"\nbytes = {}\nsha256 = \"{}\"\nexecutable = {}\n",
            artifact.role.as_str(),
            artifact.path.as_str(),
            artifact.digest.byte_length(),
            artifact.digest.sha256(),
            artifact.executable,
        )
        .expect("writing to String cannot fail");
    }
    output.into_bytes()
}

fn manifest_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::Integrity,
        QualificationRecovery::RebuildRelease,
        "validate package manifest",
        detail,
    )
}
