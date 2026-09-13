//! Exact tier-one platform matrix identities.

use crate::{ConstructionError, ConstructionErrorKind};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Tier-one operating-system family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OperatingSystem {
    /// Linux production target.
    Linux,
    /// macOS production target.
    MacOs,
    /// Windows production target.
    Windows,
}

/// Supported release architecture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Architecture {
    /// 64-bit x86.
    X86_64,
    /// 64-bit Arm.
    Aarch64,
}

/// Exact native target and its reviewed platform-profile digest.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformIdentity {
    operating_system: OperatingSystem,
    architecture: Architecture,
    profile_digest: Sha256Digest,
}

impl PlatformIdentity {
    /// Creates one exact native target identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroDigest`] for a placeholder profile digest.
    pub fn new(
        operating_system: OperatingSystem,
        architecture: Architecture,
        profile_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        super::require_digest(profile_digest)?;
        Ok(Self { operating_system, architecture, profile_digest })
    }

    /// Returns the operating-system family.
    #[must_use]
    pub const fn operating_system(&self) -> OperatingSystem { self.operating_system }

    /// Returns the processor architecture.
    #[must_use]
    pub const fn architecture(&self) -> Architecture { self.architecture }

    /// Returns the reviewed platform-profile digest.
    #[must_use]
    pub const fn profile_digest(&self) -> Sha256Digest { self.profile_digest }
}

/// Exact Linux, macOS, and Windows release target matrix.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformMatrix {
    linux: PlatformIdentity,
    macos: PlatformIdentity,
    windows: PlatformIdentity,
}

impl PlatformMatrix {
    /// Creates a complete tier-one matrix in canonical Linux/macOS/Windows order.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::InvalidPlatformMatrix`] when a slot names another OS.
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

    /// Returns the exact Linux target.
    #[must_use]
    pub const fn linux(&self) -> PlatformIdentity { self.linux }

    /// Returns the exact macOS target.
    #[must_use]
    pub const fn macos(&self) -> PlatformIdentity { self.macos }

    /// Returns the exact Windows target.
    #[must_use]
    pub const fn windows(&self) -> PlatformIdentity { self.windows }
}

} // verus!
