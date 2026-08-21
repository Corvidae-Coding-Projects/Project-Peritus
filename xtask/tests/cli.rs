#![doc = "Black-box tests for the xtask command contract."]

use std::process::Command;

#[test]
fn help_describes_the_stable_policy_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("help")
        .output()
        .expect("xtask binary must execute");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help must be UTF-8");
    assert!(stdout.contains("architecture-check"));
    assert!(stdout.contains("toolchain-check"));
    assert!(stdout.contains("verify-trust"));
}

#[test]
fn invalid_command_returns_typed_error_and_usage_guidance() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("not-a-command")
        .output()
        .expect("xtask binary must execute");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(stderr.contains("PERITUS-XTASK-CLI-001"));
    assert!(stderr.contains("cargo xtask help"));
}

#[test]
fn policy_commands_discover_the_workspace_from_a_member_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("all")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("xtask binary must execute from its package directory");
    assert!(
        output.status.success(),
        "xtask failed from a member directory: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
