//! Non-ignored Windows-native probe coverage.

#[cfg(target_os = "windows")]
#[test]
fn native_probe_reports_real_helper_platform_and_architecture() {
    use peritus_sandbox_windows::{ProbeRequest, TokenProfile, WindowsProbe};

    let helper = std::env::current_exe().unwrap();
    let request =
        ProbeRequest::new(helper, TokenProfile::restricted("S-1-1-0").unwrap(), None).unwrap();
    let probe = WindowsProbe::run(&request).unwrap();
    assert!(probe.evidence().platform);
    assert!(probe.evidence().architecture);
    assert!(probe.evidence().helper);
    assert!(probe.evidence().helper_digest.is_some());
    assert!(probe.evidence().os_build.is_some());
    assert!(probe.evidence().restricted_token);
    assert!(probe.evidence().low_integrity);
    assert!(!probe.evidence().managed_network);
}

#[cfg(target_os = "windows")]
#[test]
fn native_probe_derives_and_verifies_an_exact_app_container_identity() {
    use peritus_sandbox_windows::{AppContainerProfile, ProbeRequest, TokenProfile, WindowsProbe};

    let profile = AppContainerProfile::derive_for_current_host("Peritus.Native.Probe").unwrap();
    assert_eq!(profile.name(), "Peritus.Native.Probe");
    assert!(profile.sid().starts_with("S-1-15-2-"));
    let request = ProbeRequest::new(
        std::env::current_exe().unwrap(),
        TokenProfile::AppContainer(profile),
        None,
    )
    .unwrap();
    let probe = WindowsProbe::run(&request).unwrap();
    assert!(probe.evidence().app_container);
    assert!(probe.evidence().app_container_sid_exact);
    assert!(probe.evidence().restricted_token);
    assert!(probe.evidence().low_integrity);
    assert!(probe.evidence().deny_network);
}

#[cfg(target_os = "windows")]
#[test]
fn native_probes_exercise_acl_round_trip_and_independent_job_limits() {
    use peritus_sandbox_windows::{EnforcementLevel, ProbeRequest, TokenProfile, WindowsProbe};
    let root = tempfile::tempdir().unwrap();
    let request = ProbeRequest::new(
        std::env::current_exe().unwrap(),
        TokenProfile::restricted("S-1-1-0").unwrap(),
        None,
    )
    .unwrap()
    .with_acl_probe_root(root.path().join("probe"))
    .unwrap();
    let evidence = WindowsProbe::run(&request).unwrap().evidence().clone();
    assert!(evidence.acl, "real icacls save/grant/restore failed");
    assert!(evidence.inherited_handle_list, "actual handle-list installation failed");
    assert!(evidence.job_object && evidence.kill_on_close);
    for index in [1, 2, 6] {
        assert_eq!(evidence.resources[index], EnforcementLevel::Hard);
    }
    assert!(!root.path().join("probe").exists(), "probe must remove its owned scratch directory");
}

#[cfg(target_os = "windows")]
#[test]
fn native_path_preserves_unpaired_units_and_uses_ordinal_component_comparison() {
    use peritus_sandbox_windows::WindowsPath;
    use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
    let mut units: Vec<u16> = "C:/native/".encode_utf16().collect();
    units.push(0xD800);
    let input = std::ffi::OsString::from_wide(&units);
    let path = WindowsPath::from_os_str(&input).unwrap();
    assert_eq!(path.as_os_str().encode_wide().collect::<Vec<_>>(), units);
    assert!(
        WindowsPath::new("C:/Étage").unwrap().contains(&WindowsPath::new("c:/étage/file").unwrap())
    );
    assert!(
        !WindowsPath::new("C:/Étage")
            .unwrap()
            .contains(&WindowsPath::new("c:/étageSibling").unwrap())
    );
}

#[cfg(target_os = "windows")]
#[path = "support/mod.rs"]
mod support;

#[cfg(target_os = "windows")]
#[test]
fn native_acl_creates_and_reverses_only_the_missing_deny_anchor() {
    use peritus_sandbox_windows::{PathPolicy, WindowsPath, compile_acl_plan};
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(workspace.join("bin")).unwrap();
    std::fs::create_dir(workspace.join("workspace")).unwrap();
    std::fs::write(workspace.join("bin/tool"), b"fixture").unwrap();
    let workspace_path =
        WindowsPath::from_canonicalized(&std::fs::canonicalize(&workspace).unwrap()).unwrap();
    let protected_native = workspace.join("workspace/private");
    let protected = WindowsPath::from_os_str(protected_native.as_os_str()).unwrap();
    let policy = PathPolicy::new(workspace_path, vec![protected]).unwrap();
    let plan = compile_acl_plan(&support::checked_plan(Vec::new()), &policy, "S-1-1-0").unwrap();
    let snapshot = |target: &std::path::Path, label: &str| {
        let output = root.path().join(format!("{label}.acl"));
        assert!(
            std::process::Command::new("icacls.exe")
                .arg(target)
                .arg("/save")
                .arg(&output)
                .arg("/q")
                .status()
                .unwrap()
                .success()
        );
        std::fs::read(output).unwrap()
    };
    let original = snapshot(&workspace.join("workspace"), "before");
    let mut transaction = plan.install(&root.path().join("backup")).unwrap();
    assert!(protected_native.is_dir());
    assert!(transaction.pending_reversal_count() >= 3);
    transaction.restore().unwrap();
    assert!(!protected_native.exists());
    assert!(workspace.join("bin/tool").is_file());
    assert!(workspace.join("workspace").is_dir());
    assert!(transaction.restored());
    assert_eq!(original, snapshot(&workspace.join("workspace"), "after"));
    transaction.restore().unwrap();
}
