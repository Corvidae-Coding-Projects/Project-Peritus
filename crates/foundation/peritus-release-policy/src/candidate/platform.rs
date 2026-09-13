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
    /// Exact admission predicate for one native target identity.
    pub open spec fn inputs_valid(profile_digest: Sha256Digest) -> bool {
        crate::validation::spec_digest_nonzero(profile_digest)
    }

    /// Creates one exact native target identity.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::ZeroDigest`] for a placeholder profile digest.
    pub fn new(
        operating_system: OperatingSystem,
        architecture: Architecture,
        profile_digest: Sha256Digest,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(profile_digest),
            match result {
                Ok(value) => value.spec_operating_system() == operating_system
                    && value.spec_architecture() == architecture
                    && value.spec_profile_digest() == profile_digest,
                Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroDigest,
            },
    {
        crate::validation::require_digest(profile_digest)?;
        Ok(Self { operating_system, architecture, profile_digest })
    }

    /// Returns the operating-system family.
    #[must_use]
    pub const fn operating_system(&self) -> (value: OperatingSystem)
        ensures value == self.spec_operating_system()
    {
        self.operating_system
    }

    /// Returns the processor architecture.
    #[must_use]
    pub const fn architecture(&self) -> (value: Architecture)
        ensures value == self.spec_architecture()
    {
        self.architecture
    }

    /// Returns the reviewed platform-profile digest.
    #[must_use]
    pub const fn profile_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_profile_digest()
    {
        self.profile_digest
    }

    /// Logical view of the operating-system family.
    pub closed spec fn spec_operating_system(&self) -> OperatingSystem { self.operating_system }

    /// Logical view of the processor architecture.
    pub closed spec fn spec_architecture(&self) -> Architecture { self.architecture }

    /// Logical view of the reviewed platform-profile digest.
    pub closed spec fn spec_profile_digest(&self) -> Sha256Digest { self.profile_digest }
}

/// Exact Linux, macOS, and Windows release target matrix.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformMatrix {
    linux: PlatformIdentity,
    macos: PlatformIdentity,
    windows: PlatformIdentity,
}

impl PlatformMatrix {
    /// Exact admission predicate for the canonical three-slot platform matrix.
    pub open spec fn inputs_valid(
        linux: PlatformIdentity,
        macos: PlatformIdentity,
        windows: PlatformIdentity,
    ) -> bool {
        linux.spec_operating_system() == OperatingSystem::Linux
            && macos.spec_operating_system() == OperatingSystem::MacOs
            && windows.spec_operating_system() == OperatingSystem::Windows
    }

    /// Creates a complete tier-one matrix in canonical Linux/macOS/Windows order.
    ///
    /// # Errors
    ///
    /// Returns [`ConstructionErrorKind::InvalidPlatformMatrix`] when a slot names another OS.
    pub const fn new(
        linux: PlatformIdentity,
        macos: PlatformIdentity,
        windows: PlatformIdentity,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == Self::inputs_valid(linux, macos, windows),
            match result {
                Ok(value) => value.spec_linux() == linux
                    && value.spec_macos() == macos
                    && value.spec_windows() == windows,
                Err(error) => error.spec_kind() == ConstructionErrorKind::InvalidPlatformMatrix,
            },
    {
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
    pub const fn linux(&self) -> (value: PlatformIdentity)
        ensures value == self.spec_linux()
    {
        self.linux
    }

    /// Returns the exact macOS target.
    #[must_use]
    pub const fn macos(&self) -> (value: PlatformIdentity)
        ensures value == self.spec_macos()
    {
        self.macos
    }

    /// Returns the exact Windows target.
    #[must_use]
    pub const fn windows(&self) -> (value: PlatformIdentity)
        ensures value == self.spec_windows()
    {
        self.windows
    }

    /// Logical view of the exact Linux target.
    pub closed spec fn spec_linux(&self) -> PlatformIdentity { self.linux }

    /// Logical view of the exact macOS target.
    pub closed spec fn spec_macos(&self) -> PlatformIdentity { self.macos }

    /// Logical view of the exact Windows target.
    pub closed spec fn spec_windows(&self) -> PlatformIdentity { self.windows }
}

} // verus!
