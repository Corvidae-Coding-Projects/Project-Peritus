//! Reviewed native packaging asset source checks.

use std::collections::BTreeSet;

use peritus_platform_qualification::{Platform, bundled_packaging_assets};

#[test]
fn every_platform_embeds_install_upgrade_uninstall_and_supervisor_assets() {
    let assets = bundled_packaging_assets();
    assert_eq!(assets.len(), 15);
    let paths = assets.iter().map(|asset| asset.relative_path()).collect::<BTreeSet<_>>();
    assert_eq!(paths.len(), assets.len());
    for platform in [Platform::Linux, Platform::Macos, Platform::Windows] {
        let platform_assets =
            assets.iter().filter(|asset| asset.platform() == platform).collect::<Vec<_>>();
        assert_eq!(platform_assets.len(), 5);
        assert!(platform_assets.iter().all(|asset| !asset.bytes().is_empty()));
        assert!(
            platform_assets.iter().any(|asset| asset.relative_path().contains("Install-Peritus"))
        );
        assert!(
            platform_assets.iter().any(|asset| asset.relative_path().contains("Upgrade-Peritus"))
        );
        assert!(
            platform_assets.iter().any(|asset| asset.relative_path().contains("Uninstall-Peritus"))
        );
    }
}

#[test]
fn packaging_assets_have_nonzero_digests() {
    assert!(
        bundled_packaging_assets()
            .iter()
            .all(|asset| asset.digest().sha256().as_bytes() != &[0; 32])
    );
}
