mod fixture;

use fixture::{Fixture, assert_success};
use std::{fs, os::unix::fs::symlink};

#[test]
fn bootstrap_installs_upgrades_sets_path_once_and_preserves_state_on_removal() {
    let fixture = Fixture::new();
    let profile = fixture.home.join(".profile");
    fs::write(&profile, "# Keep my settings\n").expect("existing shell settings");
    let marker = fixture.home.join("private-state");
    fs::write(&marker, "retain this").expect("state marker");
    assert_success(&fixture.run());
    assert!(fixture.installed().is_file());
    assert_success(&fixture.run());
    let text = fs::read_to_string(profile).expect("shell settings");
    assert!(text.starts_with("# Keep my settings\n"));
    assert_eq!(text.matches("export PATH=").count(), 1);
    assert!(!fixture.log.exists(), "present dependencies must not invoke a package manager");
    assert_success(&fixture.uninstall());
    assert!(!fixture.installed().exists());
    assert_eq!(fs::read_to_string(marker).expect("state preserved"), "retain this");
}

#[test]
fn bootstrap_rejects_archive_corruption_and_package_tampering_before_install() {
    for outer in [false, true] {
        let fixture = Fixture::new();
        if outer {
            fs::write(&fixture.checksum, format!("{}\n", "0".repeat(64))).expect("bad checksum");
        } else {
            fs::write(fixture.bundle.join("bin/peritus"), "bad bytes").expect("corrupt artifact");
            fixture.repack();
        }
        assert!(!fixture.run().status.success());
        assert!(!fixture.installed().exists());
        assert!(!fixture.log.exists());
    }
}

#[test]
fn bootstrap_rejects_links_and_outside_archive_paths() {
    let fixture = Fixture::new();
    symlink("/tmp", fixture.bundle.join("outside-link")).expect("malicious link fixture");
    fixture.repack();
    let output = fixture.run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("link or special file"));
    assert!(!fixture.installed().exists());
}

#[test]
fn installer_does_not_follow_a_shell_profile_symlink() {
    let fixture = Fixture::new();
    let actual = fixture.home.join("my-dotfiles");
    fs::write(&actual, "keep exactly\n").expect("dotfiles");
    symlink(&actual, fixture.home.join(".profile")).expect("profile symlink");
    assert_success(&fixture.run());
    assert_eq!(fs::read_to_string(actual).expect("dotfiles preserved"), "keep exactly\n");
}

#[cfg(target_os = "linux")]
#[test]
fn missing_dependencies_use_the_system_manager_and_recheck_success() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.commands.join("git")).expect("missing Git");
    fixture.mock("apt-get", "if [ \"$1\" = install ]; then\n  printf '#!/bin/sh\\nprintf git-version\\\\n\\n' > \"$PERITUS_TEST_BIN/git\"\n  /bin/chmod +x \"$PERITUS_TEST_BIN/git\"\nfi\n");
    assert_success(&fixture.run());
    let log = fs::read_to_string(&fixture.log).expect("package manager calls");
    assert!(log.contains("apt-get update"));
    assert!(log.contains("apt-get install --yes --no-install-recommends git bubblewrap dbus-user-session gnome-keyring ca-certificates"));
    assert!(fixture.installed().is_file());
}

#[cfg(target_os = "linux")]
#[test]
fn failed_or_incomplete_dependency_installation_never_publishes_the_package() {
    for body in ["exit 42\n", "exit 0\n"] {
        let fixture = Fixture::new();
        fs::remove_file(fixture.commands.join("git")).expect("missing Git");
        fixture.mock("apt-get", body);
        assert!(!fixture.run().status.success());
        assert!(!fixture.installed().exists());
    }
}

#[cfg(target_os = "linux")]
#[test]
fn dependency_opt_out_is_not_a_readiness_bypass() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.commands.join("git")).expect("missing Git");
    fixture.mock("apt-get", "exit 0\n");
    let output = fixture.command().env("PERITUS_INSTALL_DEPS", "0").output().expect("opt out");
    assert!(!output.status.success());
    assert!(!fixture.log.exists());
    assert!(!fixture.installed().exists());
}

#[cfg(target_os = "linux")]
#[test]
fn installed_bubblewrap_must_create_a_real_namespace_before_publication() {
    let fixture = Fixture::new();
    fixture.mock("bwrap", "[ \"$1\" = --version ]\n");
    let output = fixture.command().env("PERITUS_INSTALL_DEPS", "0").output().expect("probe");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("bubblewrap cannot create a sandbox"));
    assert!(!fixture.installed().exists());
}
