//! Runtime capability probing with a deterministic, testable evidence projection.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use peritus_sandbox::{FeatureSet, SandboxFeature};
use peritus_types::Sha256Digest;

use crate::{
    EnforcementLevel, MacosError, MacosErrorKind, MacosOperation, ProxyRoute, RecoveryAction,
};

#[cfg(target_os = "macos")]
mod native;

/// Dimension-specific resource support observed on one host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceProbe {
    levels: [EnforcementLevel; 8],
}

impl ResourceProbe {
    /// Creates an exact resource-support report in sandbox resource discriminant order.
    #[must_use]
    pub const fn new(levels: [EnforcementLevel; 8]) -> Self {
        Self { levels }
    }

    /// Returns support levels for wall, CPU, memory, disk, output, open handles, processes, and
    /// concurrency, in that order.
    #[must_use]
    pub const fn levels(self) -> [EnforcementLevel; 8] {
        self.levels
    }

    /// Returns the expected macOS split between kernel rlimits and C2 supervision.
    #[must_use]
    pub const fn macos_production() -> Self {
        Self::new([
            EnforcementLevel::Supervisor,
            EnforcementLevel::Hard,
            EnforcementLevel::Hard,
            EnforcementLevel::Supervisor,
            EnforcementLevel::Supervisor,
            EnforcementLevel::Hard,
            EnforcementLevel::Supervisor,
            EnforcementLevel::Supervisor,
        ])
    }

    /// Returns an entirely unsupported mapping.
    #[must_use]
    pub const fn unsupported() -> Self {
        Self::new([EnforcementLevel::Unsupported; 8])
    }
}

/// Raw bounded evidence from a macOS host probe.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independently probed host control remains explicit and digest-bound"
)]
pub struct ProbeEvidence {
    /// Parsed macOS product version.
    pub os_version: Option<(u16, u16, u16)>,
    /// Whether the build target and OS are macOS.
    pub platform: bool,
    /// Whether the CPU architecture is Apple-supported x86-64 or `AArch64`.
    pub architecture: bool,
    /// Whether the reviewed helper exists and is executable.
    pub helper: bool,
    /// Whether the checked Seatbelt executable exists.
    pub seatbelt: bool,
    /// Whether a minimal deny-default profile compiled and executed successfully.
    pub profile_compilation: bool,
    /// Whether a fresh process group and descendant containment are available.
    pub process_containment: bool,
    /// Whether a PTY device is available.
    pub pty: bool,
    /// Whether macOS Keychain tooling is available.
    pub credential_store: bool,
    /// Whether loopback managed-proxy transport is available and any requested route was reachable.
    pub proxy: bool,
    /// Dimension-specific native or supervisor resource enforcement.
    pub resources: ResourceProbe,
    /// Digest of the checked helper bytes when readable.
    pub helper_digest: Option<Sha256Digest>,
}

impl ProbeEvidence {
    /// Returns evidence for a fully available macOS 15 production host.
    #[must_use]
    pub const fn supported_fixture() -> Self {
        Self {
            os_version: Some((15, 0, 0)),
            platform: true,
            architecture: true,
            helper: true,
            seatbelt: true,
            profile_compilation: true,
            process_containment: true,
            pty: true,
            credential_store: true,
            proxy: true,
            resources: ResourceProbe::macos_production(),
            helper_digest: Some(Sha256Digest::new([0xA5; 32])),
        }
    }
}

/// Immutable, canonicalized host capability report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacosHostProbe {
    evidence: ProbeEvidence,
    supported_features: FeatureSet,
    digest: Sha256Digest,
}

impl MacosHostProbe {
    /// Validates and canonicalizes raw probe evidence.
    ///
    /// # Errors
    /// Rejects internally inconsistent evidence such as successful compilation without Seatbelt.
    pub fn from_evidence(evidence: ProbeEvidence) -> Result<Self, MacosError> {
        if evidence.profile_compilation && !evidence.seatbelt {
            return Err(MacosError::new(
                MacosErrorKind::ProbeFailed,
                MacosOperation::Probe,
                RecoveryAction::SelectSupportedBackend,
                "profile compilation succeeded without a Seatbelt mechanism",
            ));
        }
        if evidence.helper_digest.is_some() && !evidence.helper {
            return Err(MacosError::new(
                MacosErrorKind::ProbeFailed,
                MacosOperation::Probe,
                RecoveryAction::RepairHelper,
                "helper identity was observed without an executable helper",
            ));
        }
        let supported_features = supported_features(&evidence);
        let digest = peritus_codec::sha256(&probe_bytes(&evidence));
        Ok(Self { evidence, supported_features, digest })
    }

    /// Returns a fail-closed report for a non-macOS host.
    #[must_use]
    pub fn unsupported_current_host() -> Self {
        let evidence = ProbeEvidence {
            os_version: None,
            platform: false,
            architecture: false,
            helper: false,
            seatbelt: false,
            profile_compilation: false,
            process_containment: false,
            pty: false,
            credential_store: false,
            proxy: false,
            resources: ResourceProbe::unsupported(),
            helper_digest: None,
        };
        let digest = peritus_codec::sha256(&probe_bytes(&evidence));
        Self { evidence, supported_features: FeatureSet::empty(), digest }
    }

    /// Returns raw, bounded evidence.
    #[must_use]
    pub const fn evidence(&self) -> &ProbeEvidence {
        &self.evidence
    }

    /// Returns the exact support intersection advertised to C2.
    #[must_use]
    pub const fn supported_features(&self) -> FeatureSet {
        self.supported_features
    }

    /// Returns the deterministic digest of every probe fact.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Reports whether the minimum macOS 15 platform and core helper/Seatbelt controls exist.
    #[must_use]
    pub fn core_supported(&self) -> bool {
        self.evidence.platform
            && self.evidence.architecture
            && self.evidence.os_version.is_some_and(|version| version.0 >= 15)
            && self.evidence.helper
            && self.evidence.seatbelt
            && self.evidence.profile_compilation
            && self.evidence.process_containment
    }
}

/// Inputs for a side-effect-bounded native capability probe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeRequest {
    helper_path: PathBuf,
    seatbelt_path: PathBuf,
    proxy: Option<ProxyRoute>,
    connect_timeout: Duration,
}

impl ProbeRequest {
    /// Creates a probe request with exact installed paths and optional proxy route.
    ///
    /// # Errors
    /// Rejects non-absolute paths or a zero/excessive connection timeout.
    pub fn new(
        helper_path: PathBuf,
        seatbelt_path: PathBuf,
        proxy: Option<ProxyRoute>,
        connect_timeout: Duration,
    ) -> Result<Self, MacosError> {
        if !helper_path.is_absolute() || !seatbelt_path.is_absolute() {
            return Err(crate::error::invalid(
                MacosOperation::Probe,
                "probe executable paths must be absolute",
            ));
        }
        if connect_timeout.is_zero() || connect_timeout > Duration::from_secs(10) {
            return Err(crate::error::invalid(
                MacosOperation::Probe,
                "proxy probe timeout is zero or excessive",
            ));
        }
        Ok(Self { helper_path, seatbelt_path, proxy, connect_timeout })
    }

    /// Returns the helper path.
    #[must_use]
    pub fn helper_path(&self) -> &Path {
        &self.helper_path
    }

    /// Returns the Seatbelt executable path.
    #[must_use]
    pub fn seatbelt_path(&self) -> &Path {
        &self.seatbelt_path
    }

    /// Returns the optional exact proxy route.
    #[must_use]
    pub const fn proxy(&self) -> Option<ProxyRoute> {
        self.proxy
    }
}

/// Native probe implementation. It never invokes a shell.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProbe;

impl SystemProbe {
    /// Executes bounded native checks and returns a fail-closed support report.
    ///
    /// # Errors
    /// Returns a typed probe failure only when observed output is malformed or unbounded.
    pub fn run(request: &ProbeRequest) -> Result<MacosHostProbe, MacosError> {
        #[cfg(target_os = "macos")]
        {
            native::run_macos_probe(request)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = request;
            Ok(MacosHostProbe::unsupported_current_host())
        }
    }
}

fn supported_features(evidence: &ProbeEvidence) -> FeatureSet {
    let core = evidence.platform
        && evidence.architecture
        && evidence.os_version.is_some_and(|version| version.0 >= 15)
        && evidence.helper
        && evidence.seatbelt
        && evidence.profile_compilation
        && evidence.process_containment;
    let mut features = FeatureSet::empty();
    if core {
        for feature in [
            SandboxFeature::FilesystemDiscover,
            SandboxFeature::FilesystemMetadata,
            SandboxFeature::FilesystemRead,
            SandboxFeature::FilesystemExecute,
            SandboxFeature::FilesystemCreate,
            SandboxFeature::FilesystemWrite,
            SandboxFeature::FilesystemRemove,
            SandboxFeature::ProcessRoot,
            SandboxFeature::ProcessDescendants,
            SandboxFeature::EnvironmentClear,
            SandboxFeature::EnvironmentAllowList,
            SandboxFeature::NetworkDeny,
            SandboxFeature::Pipes,
            SandboxFeature::Stdin,
        ] {
            features.insert(feature);
        }
    }
    if core && evidence.process_containment {
        features.insert(SandboxFeature::ProcessSignals);
        features.insert(SandboxFeature::ProcessTree);
        features.insert(SandboxFeature::TerminalSignals);
    }
    if core && evidence.pty {
        features.insert(SandboxFeature::Pty);
        features.insert(SandboxFeature::TerminalResize);
    }
    if core && evidence.proxy {
        features.insert(SandboxFeature::NetworkEgress);
    }
    if core && evidence.credential_store {
        features.insert(SandboxFeature::SecretEnvironment);
        features.insert(SandboxFeature::SecretFile);
        features.insert(SandboxFeature::SecretHandle);
    }
    let resource_features = [
        SandboxFeature::WallTime,
        SandboxFeature::CpuTime,
        SandboxFeature::Memory,
        SandboxFeature::Disk,
        SandboxFeature::Output,
        SandboxFeature::OpenHandles,
        SandboxFeature::ProcessCount,
        SandboxFeature::Concurrency,
    ];
    if core {
        for (feature, level) in resource_features.into_iter().zip(evidence.resources.levels()) {
            if matches!(level, EnforcementLevel::Hard | EnforcementLevel::Supervisor) {
                features.insert(feature);
            }
        }
    }
    features
}

fn probe_bytes(evidence: &ProbeEvidence) -> Vec<u8> {
    let mut bytes = b"peritus.macos.probe.v1\0".to_vec();
    match evidence.os_version {
        Some((major, minor, patch)) => {
            bytes.push(1);
            bytes.extend_from_slice(&major.to_be_bytes());
            bytes.extend_from_slice(&minor.to_be_bytes());
            bytes.extend_from_slice(&patch.to_be_bytes());
        }
        None => bytes.push(0),
    }
    bytes.extend([
        u8::from(evidence.platform),
        u8::from(evidence.architecture),
        u8::from(evidence.helper),
        u8::from(evidence.seatbelt),
        u8::from(evidence.profile_compilation),
        u8::from(evidence.process_containment),
        u8::from(evidence.pty),
        u8::from(evidence.credential_store),
        u8::from(evidence.proxy),
    ]);
    bytes.extend(evidence.resources.levels().map(EnforcementLevel::ordinal));
    match evidence.helper_digest {
        Some(digest) => {
            bytes.push(1);
            bytes.extend_from_slice(digest.as_bytes());
        }
        None => bytes.push(0),
    }
    bytes
}
