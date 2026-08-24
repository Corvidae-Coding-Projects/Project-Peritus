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
