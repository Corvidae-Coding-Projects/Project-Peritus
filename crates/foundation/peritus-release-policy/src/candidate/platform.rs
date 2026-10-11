//! Exact tier-one platform matrix identities.

use crate::{ConstructionError, ConstructionErrorKind};
use peritus_types::Sha256Digest;

#[doc = " Tier-one operating-system family."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OperatingSystem {
    #[doc = " Linux production target."]
    Linux,
    #[doc = " macOS production target."]
    MacOs,
    #[doc = " Windows production target."]
    Windows,
}
#[doc = " Supported release architecture."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Architecture {
    #[doc = " 64-bit x86."]
    X86_64,
    #[doc = " 64-bit Arm."]
    Aarch64,
}
#[doc = " Exact native target and its reviewed platform-profile digest."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformIdentity {
    operating_system: OperatingSystem,
    architecture: Architecture,
    profile_digest: Sha256Digest,
}
impl PlatformIdentity {
    #[doc = " Creates one exact native target identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ConstructionErrorKind::ZeroDigest`] for a placeholder profile digest."]
    pub fn new(
        operating_system: OperatingSystem,
        architecture: Architecture,
        profile_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        crate::validation::require_digest(profile_digest)?;
        Ok(Self { operating_system, architecture, profile_digest })
    }
    #[doc = " Returns the operating-system family."]
    #[must_use]
    pub const fn operating_system(&self) -> OperatingSystem {
        self.operating_system
    }
    #[doc = " Returns the processor architecture."]
    #[must_use]
    pub const fn architecture(&self) -> Architecture {
        self.architecture
    }
    #[doc = " Returns the reviewed platform-profile digest."]
    #[must_use]
    pub const fn profile_digest(&self) -> Sha256Digest {
        self.profile_digest
    }
}
#[doc = " Exact Linux, macOS, and Windows release target matrix."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformMatrix {
    linux: PlatformIdentity,
    macos: PlatformIdentity,
    windows: PlatformIdentity,
}
impl PlatformMatrix {
    #[doc = " Creates a complete tier-one matrix in canonical Linux/macOS/Windows order."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ConstructionErrorKind::InvalidPlatformMatrix`] when a slot names another OS."]
    pub const fn new(
        linux: PlatformIdentity,
        macos: PlatformIdentity,
        windows: PlatformIdentity,
    ) -> Result<Self, ConstructionError> {
        if matches!(linux.operating_system(), OperatingSystem::Linux)
            && matches!(macos.operating_system(), OperatingSystem::MacOs)
            && matches!(windows.operating_system(), OperatingSystem::Windows)
        {
            Ok(Self { linux, macos, windows })
        } else {
            Err(ConstructionError::new(ConstructionErrorKind::InvalidPlatformMatrix))
        }
    }
    #[doc = " Returns the exact Linux target."]
    #[must_use]
    pub const fn linux(&self) -> PlatformIdentity {
        self.linux
    }
    #[doc = " Returns the exact macOS target."]
    #[must_use]
    pub const fn macos(&self) -> PlatformIdentity {
        self.macos
    }
    #[doc = " Returns the exact Windows target."]
    #[must_use]
    pub const fn windows(&self) -> PlatformIdentity {
        self.windows
    }
}
