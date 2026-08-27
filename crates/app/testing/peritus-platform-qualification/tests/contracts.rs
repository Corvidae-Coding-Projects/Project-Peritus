//! H2 layout, transport, service, manifest, lifecycle, and sandbox contract tests.
//! Each layout fixture passes a borrowed home path to the production constructor.

use peritus_platform_qualification::{
    Architecture, ArtifactDigest, ArtifactRole, InstallPath, LifecycleAction, LifecyclePlan,
    ManifestArtifact, PackageManifest, PackageVersion, PathOwnership, Platform, PlatformContract,
    PlatformVersion, QualificationTarget, RelativePackagePath, ReleaseLayout, ServiceContract,
    Sha256Digest, StoreIdentity, SupervisorKind, digest_bytes,
};

#[test]
fn production_layouts_keep_configuration_and_state_outside_package_ownership() {
    for (platform, home) in [
        (Platform::Linux, "/home/alice"),
        (Platform::Macos, "/Users/alice"),
        (Platform::Windows, "C:/Users/Alice"),
    ] {
        let home = InstallPath::new(platform, home).expect("canonical home");
        let layout = ReleaseLayout::production(platform, &home).expect("production layout");
        let config = layout
            .entries()
            .iter()
            .find(|entry| entry.path() == layout.config_file())
            .expect("config entry");
        let state = layout
            .entries()
            .iter()
            .find(|entry| entry.path() == layout.state_root())
            .expect("state entry");
        assert_eq!(config.ownership(), PathOwnership::Operator);
        assert_eq!(state.ownership(), PathOwnership::Runtime);
        assert!(config.preserve_on_uninstall());
        assert!(state.preserve_on_uninstall());
    }
}

#[test]
fn endpoint_derivation_matches_g0_address_shapes() {
    let store =
        StoreIdentity::from_hex("11111111111111111111111111111111").expect("store identity");
    let linux_root =
        InstallPath::new(Platform::Linux, "/home/alice/.local/state/peritus").expect("state path");
    let linux = peritus_platform_qualification::EndpointExpectation::derive(
        Platform::Linux,
        &linux_root,
        store,
    )
    .expect("Linux endpoint");
    assert!(linux.address().as_argument().starts_with("/home/alice/.local/state/peritus/peritus-"));
    assert!(
        std::path::Path::new(linux.address().as_argument())
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sock"))
    );
    assert_eq!(linux.unix_socket_mode(), Some(0o600));
    assert!(!linux.remote_listener_permitted());

    let windows_root =
        InstallPath::new(Platform::Windows, "C:/Users/Alice/AppData/Local/Peritus/state")
            .expect("state path");
    let windows = peritus_platform_qualification::EndpointExpectation::derive(
        Platform::Windows,
        &windows_root,
        store,
    )
    .expect("Windows endpoint");
    assert!(windows.address().as_argument().starts_with(r"\\.\pipe\peritus-"));
    assert_eq!(windows.unix_socket_mode(), None);
}

#[test]
fn services_directly_run_the_only_production_daemon_mode() {
    for (platform, home, supervisor) in [
        (Platform::Linux, "/home/alice", SupervisorKind::SystemdUser),
        (Platform::Macos, "/Users/alice", SupervisorKind::LaunchAgent),
        (Platform::Windows, "C:/Users/Alice", SupervisorKind::WindowsTaskScheduler),
    ] {
        let home = InstallPath::new(platform, home).expect("home");
        let layout = ReleaseLayout::production(platform, &home).expect("layout");
        let service = ServiceContract::production(&layout).expect("service");
        assert_eq!(service.supervisor(), supervisor);
        assert_eq!(service.arguments()[0], "serve");
        assert_eq!(service.arguments()[1], "--config");
        assert_eq!(service.arguments()[2], layout.config_file().as_str());
        assert!(!service.shell_wrapped());
        assert!(service.user_scoped());
    }
}

#[test]
fn canonical_manifest_round_trips_and_binds_lifecycle_preservation() {
    let home = InstallPath::new(Platform::Linux, "/home/alice").expect("home");
    let layout = ReleaseLayout::production(Platform::Linux, &home).expect("layout");
    let manifest = manifest(&layout);
    let parsed = PackageManifest::parse(manifest.canonical_bytes()).expect("canonical parse");
    assert_eq!(parsed, manifest);
    assert_eq!(parsed.checksums().lines().count(), 8);

    let upgrade = LifecyclePlan::production(LifecycleAction::Upgrade, &layout, &manifest)
        .expect("upgrade plan");
    assert!(upgrade.preserved_paths().contains(layout.config_file()));
    assert!(upgrade.preserved_paths().contains(layout.state_root()));
    assert!(!upgrade.compensation().is_empty());
}

#[test]
fn production_platform_minimums_reject_older_subjects() {
    let contract = PlatformContract::production(Platform::Linux);
    let old = QualificationTarget::new(
        Platform::Linux,
        Architecture::X86_64,
        PlatformVersion::new(6, 5, 0, 0),
    );
    assert!(contract.validate_target(old).is_err());
    assert!(
        contract
            .validate_target(QualificationTarget::new(
                Platform::Linux,
                Architecture::X86_64,
                PlatformVersion::new(6, 6, 0, 0),
            ))
            .is_ok()
    );
}

fn manifest(layout: &ReleaseLayout) -> PackageManifest {
    let roles = [
        (ArtifactRole::Daemon, "bin/peritusd", true),
        (ArtifactRole::Cli, "bin/peritus", true),
        (ArtifactRole::Tui, "bin/peritus-tui", true),
        (ArtifactRole::SandboxHelper, "libexec/peritus-linux-sandbox-helper", true),
        (ArtifactRole::ServiceDefinition, "share/peritus/peritus.service", false),
        (ArtifactRole::Installer, "Install-Peritus.sh", true),
        (ArtifactRole::Uninstaller, "Uninstall-Peritus.sh", true),
        (ArtifactRole::Upgrader, "Upgrade-Peritus.sh", true),
    ];
    let artifacts = roles
        .into_iter()
        .enumerate()
        .map(|(index, (role, path, executable))| {
            let bytes = vec![u8::try_from(index + 1).expect("small index"); index + 1];
            let digest = digest_bytes(&bytes);
            ManifestArtifact::new(
                role,
                RelativePackagePath::new(path).expect("path"),
                ArtifactDigest::new(digest.byte_length(), digest.sha256()),
                executable,
            )
            .expect("artifact")
        })
        .collect();
    PackageManifest::new(
        PackageVersion::new("0.1.0").expect("version"),
        Platform::Linux,
        Architecture::X86_64,
        layout.digest(),
        artifacts,
    )
    .expect("manifest")
}

#[test]
fn digest_parser_rejects_non_sha256_text() {
    assert!(Sha256Digest::from_hex("abc").is_err());
    assert!(Sha256Digest::from_hex(&"0".repeat(64)).is_ok());
}
