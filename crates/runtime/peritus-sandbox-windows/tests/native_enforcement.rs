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
    assert!(probe.evidence().deny_network);
}
