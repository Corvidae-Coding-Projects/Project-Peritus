//! Backend descriptor derived only from runtime probe evidence.

use peritus_sandbox::{
    BackendDescriptor, BackendKind, BackendName, BackendVersion, PathSemantics, ResourceFidelity,
};

use crate::{MacosError, MacosErrorKind, MacosHostProbe, MacosOperation, RecoveryAction};

/// Stable backend implementation identity.
pub const BACKEND_NAME: &str = "peritus-macos-seatbelt";
/// Stable backend implementation version.
pub const BACKEND_VERSION: &str = "1";

/// A descriptor paired with the exact probe from which its support set was derived.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacosDescriptor {
    descriptor: BackendDescriptor,
    probe: MacosHostProbe,
}

impl MacosDescriptor {
    /// Builds a native descriptor advertising only controls proved by the supplied probe.
    ///
    /// # Errors
    /// Returns a typed internal failure if crate-owned identity constants become invalid.
    pub fn from_probe(probe: MacosHostProbe) -> Result<Self, MacosError> {
        let name = BackendName::new(BACKEND_NAME).map_err(|_| identity_error())?;
        let version = BackendVersion::new(BACKEND_VERSION).map_err(|_| identity_error())?;
        let descriptor = BackendDescriptor::new(
            name,
            version,
            BackendKind::Native,
            PathSemantics::UnixNative,
            ResourceFidelity::Supervisor,
            probe.supported_features(),
        );
        Ok(Self { descriptor, probe })
    }

    /// Returns the C2 descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    /// Returns the exact capability probe bound to this descriptor.
    #[must_use]
    pub const fn probe(&self) -> &MacosHostProbe {
        &self.probe
    }
}

fn identity_error() -> MacosError {
    MacosError::new(
        MacosErrorKind::DescriptorMismatch,
        MacosOperation::Validate,
        RecoveryAction::Quarantine,
        "crate-owned macOS backend identity is invalid",
    )
}
