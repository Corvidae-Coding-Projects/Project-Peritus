use super::workflow_command_policy::validate as validate_command_policy;
use super::workflow_commands::CommandPolicy;
use crate::error::Diagnostic;
use std::path::Path;

#[test]
fn locked_after_cargo_boundary_does_not_protect_dependency_resolution() {
    let diagnostics = validate("cargo test --workspace -- --locked");

    assert_message(&diagnostics, "without --locked before Cargo's `--` boundary");
}

#[test]
fn multiline_recipe_commands_use_the_same_locked_and_solver_policy() {
    let recipe = r"
cargo \
      test \
      --workspace

cargo verus verify --workspace --locked \
      --check-toolchain --no-solver-version-check
";

    let diagnostics = validate(recipe);

    assert_message(&diagnostics, "without --locked before Cargo's `--` boundary");
    assert_message(&diagnostics, "disables solver-version enforcement");
}

#[test]
fn complete_recipe_command_forms_accept_locked_inputs_before_the_boundary() {
    for command in [
        "cargo --locked check --workspace",
        "cargo metadata --format-version 1 --locked",
        "cargo verus verify --workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20",
        "cargo verus verify --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-evidence --package peritus-git --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-migrations --package peritus-network --package peritus-patch --package peritus-policy --package peritus-process --package peritus-projection --package peritus-protocol --package peritus-quality-policy --package peritus-sandbox --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-secrets --package peritus-spec --package peritus-types --package peritus-workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20",
        "cargo verus build --workspace --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20",
        "cargo verus build --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-evidence --package peritus-git --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-migrations --package peritus-network --package peritus-patch --package peritus-policy --package peritus-process --package peritus-projection --package peritus-protocol --package peritus-quality-policy --package peritus-sandbox --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-secrets --package peritus-spec --package peritus-types --package peritus-workspace --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20",
        "cargo fmt --all -- --check",
        "cargo xtask reproducibility-check",
    ] {
        let diagnostics = validate(command);
        assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
    }
}

#[test]
fn fake_nested_and_background_cargo_are_not_accepted_as_direct_evidence() {
    let fake = validate("echo cargo test --workspace");
    assert_message(&fake, "unaudited executable");
    assert!(
        !fake.iter().any(|diagnostic| diagnostic.message().contains("without --locked")),
        "echo's data must not be parsed as Cargo: {fake:?}"
    );

    let nested = validate("sh -c 'cargo test --workspace --locked'");
    assert_message(&nested, "outside the failure-propagating command model");

    let background = validate("cargo test --workspace --locked &");
    assert_message(&background, "outside the failure-propagating command model");
}

#[test]
fn local_paths_script_suffixes_and_opaque_runners_fail_closed() {
    for command in [
        "./ci.sh",
        "../scripts/ci",
        "/opt/project/ci",
        r"C:\project\ci.ps1",
        "runner.cmd",
        "make verify",
        "just gate-a",
    ] {
        assert_message(&validate(command), "unaudited executable");
    }
}

#[test]
fn sequences_and_pipelines_cannot_mask_a_direct_cargo_failure() {
    for command in [
        "cargo test --workspace --locked; printf ok",
        "cargo test --workspace --locked | sha256sum",
    ] {
        assert_message(&validate(command), "outside the failure-propagating command model");
    }
    assert!(
        validate("cargo test --workspace --locked\n").is_empty(),
        "a trailing recipe newline must retain Cargo's status"
    );
}

#[test]
fn leading_environment_assignments_cannot_change_direct_cargo_execution() {
    for command in [
        "RUSTC_WRAPPER=./evil cargo test --workspace --locked",
        "PATH=./attacker cargo build --workspace --locked",
        "CARGO_HOME=./mutable cargo metadata --format-version 1 --locked",
        "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked",
    ] {
        assert_message(&validate(command), "forbidden leading environment assignments");
    }
}

#[test]
fn every_partial_or_forwarded_verus_form_is_rejected() {
    for command in [
        "cargo verus verify --workspace --locked --check-toolchain --exclude crate-a",
        "cargo verus verify --workspace --locked --check-toolchain module_name",
        "cargo verus verify --workspace --locked --check-toolchain -- --no-verify",
        "cargo verus verify --locked --workspace --check-toolchain",
        "cargo verus build --workspace --locked --check-toolchain --release --no-solver-version-check",
        "cargo verus verify --workspace --all-features --locked --check-toolchain --fwd-verus-args-to all -- --no-cheating -V check-api-safety --rlimit 20",
        "cargo verus verify --workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20",
        "cargo verus verify --workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating -V check-api-safety --rlimit 20",
        "cargo verus verify --workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 0",
    ] {
        assert_message(&validate(command), "non-canonical cargo-verus invocation");
    }
}

#[test]
fn xtask_requires_the_exact_locked_alias_policy() {
    let mut diagnostics = Vec::new();
    validate_command_policy(
        "cargo xtask all",
        Path::new("justfile"),
        "recipe command",
        CommandPolicy::new(false),
        &mut diagnostics,
    );
    assert_message(&diagnostics, "without the exact locked alias");
}

fn validate(script: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    validate_command_policy(
        script,
        Path::new("justfile"),
        "recipe command",
        CommandPolicy::new(true),
        &mut diagnostics,
    );
    diagnostics
}

fn assert_message(diagnostics: &[Diagnostic], expected: &str) {
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message().contains(expected)),
        "expected a diagnostic containing `{expected}`, got {diagnostics:?}"
    );
}
