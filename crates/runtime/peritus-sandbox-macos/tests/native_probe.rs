//! Target-native macOS capability checks; compiled and executed only by a macOS runner.

#![cfg(target_os = "macos")]

use std::time::Duration;

use peritus_sandbox_macos::{MacosDescriptor, ProbeRequest, SystemProbe};

#[test]
fn live_macos_15_probe_compiles_a_deny_default_seatbelt_profile() {
    let helper = std::env::current_exe().unwrap();
    let request =
        ProbeRequest::new(helper, "/usr/bin/sandbox-exec".into(), None, Duration::from_secs(1))
            .unwrap();
    let probe = SystemProbe::run(&request).unwrap();
    assert!(probe.core_supported());
    assert!(MacosDescriptor::from_probe(probe).is_ok());
}
