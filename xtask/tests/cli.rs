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
    assert!(stdout.contains("docs-check"));
    assert!(stdout.contains("format-check"));
    assert!(stdout.contains("ordinary-api-check"));
    assert!(stdout.contains("toolchain-check"));
    assert!(stdout.contains("verify-trust"));
    assert!(stdout.contains("distro-compile"));
    assert!(stdout.contains("distro-package-compiled"));
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
fn proof_impact_inventory_reports_current_bytes_without_approving_them() {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("proof-impact-inventory")
        .current_dir(root)
        .output()
        .expect("inventory command executes");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let inventory: serde_json::Value = serde_json::from_slice(&output.stdout).expect("audit JSON");
    assert_eq!(inventory["status"], "audit-only");
    let sources = inventory["sources"].as_object().expect("current inputs");
    let lock = std::fs::read(root.join("Cargo.lock")).expect("current lockfile");
    let hash = Sha256::digest(lock).iter().fold(String::new(), |mut output, byte| {
        write!(output, "{byte:02x}").expect("write hexadecimal");
        output
    });
    assert_eq!(sources["Cargo.lock"]["sha256"], hash);
    assert!(sources.values().all(|source| source.get("change_id").is_none()));
    assert!(!sources.contains_key("verification/proof-impact.toml"));
    assert!(inventory.get("changes").is_none());
}

#[test]
fn policy_commands_discover_the_workspace_from_a_member_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("architecture-check")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("xtask binary must execute from its package directory");
    assert!(
        output.status.success(),
        "xtask failed from a member directory: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("architecture-check passed"),
        "xtask did not run the requested root-dependent policy command"
    );
}
