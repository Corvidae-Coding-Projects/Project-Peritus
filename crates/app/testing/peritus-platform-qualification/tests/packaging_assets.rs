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

#[test]
fn windows_installer_hashes_without_optional_command_modules() {
    let installer = bundled_packaging_assets()
        .iter()
        .find(|asset| asset.relative_path() == "windows/Install-Peritus.ps1")
        .expect("Windows installer asset must be embedded");
    let script = str::from_utf8(installer.bytes()).expect("Windows installer must be UTF-8");

    assert!(script.contains("[Security.Cryptography.SHA256]::Create()"));
    assert!(!script.contains("Get-FileHash"));
}

#[test]
fn windows_lifecycle_supports_explicit_private_roots_and_user_defaults() {
    let assets = bundled_packaging_assets();
    for name in ["Install-Peritus.ps1", "Upgrade-Peritus.ps1"] {
        let script = windows_script(assets, name);
        assert!(script.contains("[string]$InstallRoot"));
        assert!(script.contains("$env:LOCALAPPDATA"));
    }
    let uninstall = windows_script(assets, "Uninstall-Peritus.ps1");
    assert!(uninstall.contains("[string]$InstallRoot"));
    assert!(uninstall.contains("[string]$DataRoot"));
    assert!(uninstall.contains("$env:LOCALAPPDATA"));
}

#[test]
fn windows_supervisor_template_keeps_exact_direct_command_placeholders() {
    let template = bundled_packaging_assets()
        .iter()
        .find(|asset| asset.relative_path() == "windows/Peritus.Task.xml.in")
        .expect("Windows supervisor template must be embedded");
    let xml = str::from_utf8(template.bytes()).expect("Windows supervisor template must be UTF-8");

    for required in [
        "<Command>@PERITUSD@</Command>",
        "<Arguments>serve --config &quot;@CONFIG_FILE@&quot;</Arguments>",
        "<RestartOnFailure>",
    ] {
        assert!(xml.contains(required), "missing Windows supervisor control: {required}");
    }
    assert!(!xml.contains("cmd.exe /c"));
    assert!(!xml.contains("powershell -Command"));
}

fn windows_script<'a>(
    assets: &'a [peritus_platform_qualification::BundledPackagingAsset],
    name: &str,
) -> &'a str {
    let relative_path = format!("windows/{name}");
    let asset = assets
        .iter()
        .find(|asset| asset.relative_path() == relative_path)
        .expect("Windows lifecycle asset must be embedded");
    str::from_utf8(asset.bytes()).expect("Windows lifecycle asset must be UTF-8")
}
