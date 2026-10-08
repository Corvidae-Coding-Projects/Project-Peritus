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
        "<Arguments>supervise --config &quot;@CONFIG_FILE@&quot;</Arguments>",
        "<AllowHardTerminate>false</AllowHardTerminate>",
        "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
    ] {
        assert!(xml.contains(required), "missing Windows supervisor control: {required}");
    }
    assert!(!xml.contains("cmd.exe /c"));
    assert!(!xml.contains("powershell -Command"));
    assert!(!xml.contains("<RestartOnFailure>"));
    assert!(!xml.contains("<Count>"));
    assert!(!xml.contains("<AllowHardTerminate>true</AllowHardTerminate>"));

    let uninstall = windows_script(bundled_packaging_assets(), "Uninstall-Peritus.ps1");
    assert!(uninstall.contains("& $daemon package-handoff --config $configuration"));
    assert!(uninstall.contains("if ($LASTEXITCODE -ne 0)"));
}

#[test]
fn supervisor_templates_have_no_restart_exhaustion_or_forced_stop_deadline() {
    let assets = bundled_packaging_assets();
    let linux = asset_text(assets, "linux/peritus.service");
    assert!(linux.contains("StartLimitIntervalSec=0"));
    assert!(linux.contains("Restart=on-failure"));
    assert!(linux.contains("TimeoutStopSec=infinity"));
    assert!(!linux.contains("StartLimitBurst="));

    let macos = asset_text(assets, "macos/com.corvidae.peritus.plist.in");
    assert!(macos.contains("<key>SuccessfulExit</key>\n        <false/>"));
    assert!(macos.contains("<key>ExitTimeOut</key>\n    <integer>0</integer>"));
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

fn asset_text<'a>(
    assets: &'a [peritus_platform_qualification::BundledPackagingAsset],
    relative_path: &str,
) -> &'a str {
    let asset = assets
        .iter()
        .find(|asset| asset.relative_path() == relative_path)
        .expect("packaging asset must be embedded");
    str::from_utf8(asset.bytes()).expect("packaging asset must be UTF-8")
}
