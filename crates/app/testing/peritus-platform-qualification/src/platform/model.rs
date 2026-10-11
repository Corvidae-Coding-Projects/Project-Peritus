//! Reusable value models for packaged-host platform contracts.

use super::Platform;

/// Comparable operating-system version observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlatformVersion {
    major: u16,
    minor: u16,
    patch: u16,
    build: u32,
}

impl PlatformVersion {
    /// Creates a semantic operating-system version with an optional platform build number.
    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u16, build: u32) -> Self {
        Self { major, minor, patch, build }
    }

    /// Returns the major version.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the minor version.
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }

    /// Returns the patch version.
    #[must_use]
    pub const fn patch(self) -> u16 {
        self.patch
    }

    /// Returns the platform build number, or zero where the platform does not use one.
    #[must_use]
    pub const fn build(self) -> u32 {
        self.build
    }
}

/// Exact native prerequisite that must be observed on a packaged host.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativePrerequisite {
    id: &'static str,
    description: &'static str,
    external: bool,
}

impl NativePrerequisite {
    pub(super) const fn new(id: &'static str, description: &'static str, external: bool) -> Self {
        Self { id, description, external }
    }

    /// Returns the stable prerequisite identity.
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.id
    }

    /// Returns a non-secret operator-facing description.
    #[must_use]
    pub const fn description(self) -> &'static str {
        self.description
    }

    /// Reports whether the prerequisite is supplied by the host rather than the Peritus package.
    #[must_use]
    pub const fn external(self) -> bool {
        self.external
    }
}

/// One explicit semantic difference accepted across operating systems.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlatformDelta {
    pub(super) id: &'static str,
    pub(super) linux: &'static str,
    pub(super) macos: &'static str,
    pub(super) windows: &'static str,
}

impl PlatformDelta {
    /// Returns the stable delta identity.
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.id
    }

    /// Returns the platform-specific behavior allowed by this declaration.
    #[must_use]
    pub const fn behavior(self, platform: Platform) -> &'static str {
        match platform {
            Platform::Linux => self.linux,
            Platform::Macos => self.macos,
            Platform::Windows => self.windows,
        }
    }
}
