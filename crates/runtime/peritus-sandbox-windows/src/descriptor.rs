//! Probe-derived Windows backend descriptor.

use peritus_sandbox::{
    BackendDescriptor, BackendKind, BackendName, BackendVersion, PathSemantics, ResourceFidelity,
};
use peritus_types::Sha256Digest;

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsProbe, WindowsRecovery};

/// Stable C2 backend name.
pub const BACKEND_NAME: &str = "peritus-windows-appcontainer";
/// Native backend implementation/schema version.
pub const BACKEND_VERSION: &str = "1";

/// Exact helper/probe/filter identity paired with the descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "each independently bound identity is explicitly a digest"
)]
pub struct WindowsIdentity {
    helper_digest: Sha256Digest,
    probe_digest: Sha256Digest,
    managed_filter_digest: Option<Sha256Digest>,
}

impl WindowsIdentity {
    /// Returns the reviewed helper digest.
    #[must_use]
    pub const fn helper_digest(self) -> Sha256Digest {
        self.helper_digest
    }

    /// Returns the complete probe digest.
    #[must_use]
    pub const fn probe_digest(self) -> Sha256Digest {
        self.probe_digest
    }

    /// Returns exact managed network-filter identity, when admitted.
    #[must_use]
    pub const fn managed_filter_digest(self) -> Option<Sha256Digest> {
        self.managed_filter_digest
    }
}

/// C2 descriptor plus immutable Windows probe/implementation identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsBackendDescriptor {
    common: BackendDescriptor,
    probe: WindowsProbe,
    identity: WindowsIdentity,
}

impl WindowsBackendDescriptor {
    /// Builds a descriptor advertising only probe-proved controls.
    ///
    /// # Errors
    /// Rejects missing helper identity or invalid crate-owned name/version constants.
    pub fn from_probe(
        probe: WindowsProbe,
        managed_filter_digest: Option<Sha256Digest>,
    ) -> Result<Self, WindowsError> {
        let helper_digest = probe.evidence().helper_digest.ok_or_else(identity_error)?;
        if probe.evidence().managed_network != managed_filter_digest.is_some() {
            return Err(identity_error());
        }
        let name = BackendName::new(BACKEND_NAME).map_err(|_| identity_error())?;
        let version = BackendVersion::new(BACKEND_VERSION).map_err(|_| identity_error())?;
        let common = BackendDescriptor::new(
            name,
            version,
            BackendKind::Native,
            PathSemantics::WindowsNative,
            ResourceFidelity::Supervisor,
            probe.supported_features(),
        );
        let identity =
            WindowsIdentity { helper_digest, probe_digest: probe.digest(), managed_filter_digest };
        Ok(Self { common, probe, identity })
    }

    /// Returns the common C2 descriptor.
    #[must_use]
    pub const fn common(&self) -> &BackendDescriptor {
        &self.common
    }

    /// Returns complete probe evidence.
    #[must_use]
    pub const fn probe(&self) -> &WindowsProbe {
        &self.probe
    }

    /// Returns native implementation identity.
    #[must_use]
    pub const fn identity(&self) -> WindowsIdentity {
        self.identity
    }
}

fn identity_error() -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::DescriptorMismatch,
        WindowsOperation::Validate,
        WindowsRecovery::Quarantine,
        "Windows descriptor identity is absent, contradictory, or invalid",
    )
}
