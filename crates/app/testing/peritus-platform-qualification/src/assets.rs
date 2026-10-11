//! Reviewed native packaging assets embedded for release-builder inspection.

use crate::{ArtifactDigest, Platform, digest_bytes};

/// One repository-reviewed platform packaging asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundledPackagingAsset {
    platform: Platform,
    relative_path: &'static str,
    bytes: &'static [u8],
    executable: bool,
}

impl BundledPackagingAsset {
    const fn new(
        platform: Platform,
        relative_path: &'static str,
        bytes: &'static [u8],
        executable: bool,
    ) -> Self {
        Self { platform, relative_path, bytes, executable }
    }

    /// Returns the target platform.
    #[must_use]
    pub const fn platform(self) -> Platform {
        self.platform
    }

    /// Returns the repository-relative path below `packaging/`.
    #[must_use]
    pub const fn relative_path(self) -> &'static str {
        self.relative_path
    }

    /// Borrows the exact reviewed bytes.
    #[must_use]
    pub const fn bytes(self) -> &'static [u8] {
        self.bytes
    }

    /// Reports whether the installed package marks the asset executable.
    #[must_use]
    pub const fn executable(self) -> bool {
        self.executable
    }

    /// Returns the exact reviewed asset digest.
    #[must_use]
    pub fn digest(self) -> ArtifactDigest {
        digest_bytes(self.bytes)
    }
}

/// Returns all reviewed platform assets in platform/path order.
#[must_use]
pub const fn bundled_packaging_assets() -> &'static [BundledPackagingAsset] {
    &ASSETS
}

const ASSETS: [BundledPackagingAsset; 15] = [
    BundledPackagingAsset::new(
        Platform::Linux,
        "linux/Install-Peritus.sh",
        include_bytes!("../../../../../packaging/linux/Install-Peritus.sh"),
        true,
    ),
    BundledPackagingAsset::new(
        Platform::Linux,
        "linux/Uninstall-Peritus.sh",
        include_bytes!("../../../../../packaging/linux/Uninstall-Peritus.sh"),
        true,
    ),
    BundledPackagingAsset::new(
        Platform::Linux,
        "linux/Upgrade-Peritus.sh",
        include_bytes!("../../../../../packaging/linux/Upgrade-Peritus.sh"),
        true,
    ),
    BundledPackagingAsset::new(
        Platform::Linux,
        "linux/package.toml.in",
        include_bytes!("../../../../../packaging/linux/package.toml.in"),
        false,
    ),
    BundledPackagingAsset::new(
        Platform::Linux,
        "linux/peritus.service",
        include_bytes!("../../../../../packaging/linux/peritus.service"),
        false,
    ),
    BundledPackagingAsset::new(
        Platform::Macos,
        "macos/Install-Peritus.sh",
        include_bytes!("../../../../../packaging/macos/Install-Peritus.sh"),
        true,
    ),
    BundledPackagingAsset::new(
        Platform::Macos,
        "macos/Uninstall-Peritus.sh",
        include_bytes!("../../../../../packaging/macos/Uninstall-Peritus.sh"),
        true,
    ),
    BundledPackagingAsset::new(
        Platform::Macos,
        "macos/Upgrade-Peritus.sh",
        include_bytes!("../../../../../packaging/macos/Upgrade-Peritus.sh"),
        true,
    ),
    BundledPackagingAsset::new(
        Platform::Macos,
        "macos/com.corvidae.peritus.plist.in",
        include_bytes!("../../../../../packaging/macos/com.corvidae.peritus.plist.in"),
        false,
    ),
    BundledPackagingAsset::new(
        Platform::Macos,
        "macos/package.toml.in",
        include_bytes!("../../../../../packaging/macos/package.toml.in"),
        false,
    ),
    BundledPackagingAsset::new(
        Platform::Windows,
        "windows/Install-Peritus.ps1",
        include_bytes!("../../../../../packaging/windows/Install-Peritus.ps1"),
        true,
    ),
    BundledPackagingAsset::new(
        Platform::Windows,
        "windows/Uninstall-Peritus.ps1",
        include_bytes!("../../../../../packaging/windows/Uninstall-Peritus.ps1"),
        true,
    ),
    BundledPackagingAsset::new(
        Platform::Windows,
        "windows/Upgrade-Peritus.ps1",
        include_bytes!("../../../../../packaging/windows/Upgrade-Peritus.ps1"),
        true,
    ),
    BundledPackagingAsset::new(
        Platform::Windows,
        "windows/Peritus.Task.xml.in",
        include_bytes!("../../../../../packaging/windows/Peritus.Task.xml.in"),
        false,
    ),
    BundledPackagingAsset::new(
        Platform::Windows,
        "windows/package.toml.in",
        include_bytes!("../../../../../packaging/windows/package.toml.in"),
        false,
    ),
];
