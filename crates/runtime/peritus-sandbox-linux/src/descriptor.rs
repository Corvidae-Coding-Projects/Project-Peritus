//! Descriptor construction from observed Linux support.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxProbe, LinuxRecovery};
use peritus_sandbox::{
    BackendDescriptor, BackendKind, BackendName, BackendVersion, FeatureSet, PathSemantics,
    ResourceFidelity, SandboxFeature,
};
use peritus_types::Sha256Digest;

/// Exact helper and runner identity bound to a Linux descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "each identity component is intentionally and explicitly a digest"
)]
pub struct LinuxIdentity {
    helper_digest: Sha256Digest,
    bubblewrap_digest: Sha256Digest,
    probe_digest: Sha256Digest,
}

impl LinuxIdentity {
    /// Returns the helper executable digest.
    #[must_use]
    pub const fn helper_digest(self) -> Sha256Digest {
        self.helper_digest
    }
    /// Returns the bubblewrap executable digest.
    #[must_use]
    pub const fn bubblewrap_digest(self) -> Sha256Digest {
        self.bubblewrap_digest
    }
    /// Returns the complete runtime probe digest.
    #[must_use]
    pub const fn probe_digest(self) -> Sha256Digest {
        self.probe_digest
    }
}

/// C2 descriptor plus Linux-specific runtime identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinuxBackendDescriptor {
    common: BackendDescriptor,
    identity: LinuxIdentity,
}

impl LinuxBackendDescriptor {
    /// Builds a descriptor containing only capabilities proved by `probe`.
    ///
    /// # Errors
    /// Returns unsupported when the exact helper or runner cannot be identified.
    pub fn from_probe(probe: &LinuxProbe) -> Result<Self, LinuxError> {
        Self::from_probe_with_managed_proxy(probe, false)
    }

    pub(crate) fn from_probe_with_managed_proxy(
        probe: &LinuxProbe,
        managed_proxy: bool,
    ) -> Result<Self, LinuxError> {
        let helper_digest = probe.helper_digest().ok_or_else(|| unsupported("helper is absent"))?;
        let bubblewrap_digest = probe
            .bubblewrap()
            .executable_digest()
            .ok_or_else(|| unsupported("bubblewrap is absent"))?;
        let name = BackendName::new(crate::BACKEND_NAME)
            .map_err(|_| descriptor_error("backend name is invalid"))?;
        let version = BackendVersion::new(crate::BACKEND_VERSION)
            .map_err(|_| descriptor_error("backend version is invalid"))?;
        let common = BackendDescriptor::new(
            name,
            version,
            BackendKind::Native,
            PathSemantics::UnixNative,
            if probe.cgroup().delegated() {
                ResourceFidelity::Hard
            } else {
                ResourceFidelity::Supervisor
            },
            supported_features(probe, managed_proxy),
        );
        Ok(Self {
            common,
            identity: LinuxIdentity {
                helper_digest,
                bubblewrap_digest,
                probe_digest: probe.digest(),
            },
        })
    }

    /// Returns the common descriptor used by C2 admission.
    #[must_use]
    pub const fn common(&self) -> &BackendDescriptor {
        &self.common
    }
    /// Returns exact runtime identity.
    #[must_use]
    pub const fn identity(&self) -> LinuxIdentity {
        self.identity
    }
}

fn supported_features(probe: &LinuxProbe, managed_proxy: bool) -> FeatureSet {
    let mut features = FeatureSet::empty();
    if probe.baseline_supported() {
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
            SandboxFeature::ProcessSignals,
            SandboxFeature::ProcessTree,
            SandboxFeature::EnvironmentClear,
            SandboxFeature::EnvironmentAllowList,
            SandboxFeature::SecretEnvironment,
            SandboxFeature::SecretFile,
            SandboxFeature::SecretHandle,
            SandboxFeature::NetworkDeny,
            SandboxFeature::Pipes,
            SandboxFeature::Stdin,
            SandboxFeature::WallTime,
            SandboxFeature::Disk,
            SandboxFeature::Output,
            SandboxFeature::Concurrency,
            SandboxFeature::OpenHandles,
        ] {
            features.insert(feature);
        }
        if probe.pty() {
            for feature in [
                SandboxFeature::Pty,
                SandboxFeature::TerminalResize,
                SandboxFeature::TerminalSignals,
            ] {
                features.insert(feature);
            }
        }
        if managed_proxy {
            features.insert(SandboxFeature::NetworkEgress);
        }
        if probe.cgroup().delegated() {
            for feature in
                [SandboxFeature::CpuTime, SandboxFeature::Memory, SandboxFeature::ProcessCount]
            {
                features.insert(feature);
            }
        }
    }
    features
}

fn unsupported(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::UnsupportedHost,
        LinuxOperation::Probe,
        LinuxRecovery::ConfigureHost,
        detail,
    )
}

fn descriptor_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::DescriptorMismatch,
        LinuxOperation::Probe,
        LinuxRecovery::Replan,
        detail,
    )
}
