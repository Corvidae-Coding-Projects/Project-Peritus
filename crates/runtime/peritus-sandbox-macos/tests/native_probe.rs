//! Target-native macOS capability checks; compiled and executed only by a macOS runner.

#![cfg(target_os = "macos")]

use std::time::Duration;

use peritus_sandbox_macos::{MacosDescriptor, ProbeRequest, SystemProbe};

#[test]
fn live_macos_probe_reports_installed_capabilities_truthfully() {
    let helper = std::env::current_exe().unwrap();
    let request =
        ProbeRequest::new(helper, "/usr/bin/sandbox-exec".into(), None, Duration::from_secs(1))
            .unwrap();
    let probe = SystemProbe::run(&request).unwrap();
    let evidence = probe.evidence();
    assert!(evidence.platform);
    assert!(evidence.architecture);
    assert!(evidence.os_version.is_some_and(|version| version.0 >= 15));
    assert!(evidence.helper);
    assert!(evidence.helper_digest.is_some());
    assert!(evidence.seatbelt);
    assert!(evidence.profile_compilation);
    assert!(evidence.process_containment);
    assert!(probe.core_supported());

    let supported_features = probe.supported_features();
    let descriptor = MacosDescriptor::from_probe(probe).unwrap();
    assert_eq!(descriptor.descriptor().supported_features(), supported_features);
}
