use super::just_policy::validate_contents;
use super::workflow_commands::CommandPolicy;
use crate::error::Diagnostic;

const CANONICAL: &str = r"
default: check

fmt:
    cargo fmt --all -- --check
build:
    cargo build --workspace --all-targets --all-features --locked
test:
    cargo test --workspace --all-targets --all-features --locked
doc-test:
    cargo test --doc --workspace --all-features --locked
clippy:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
docs:
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked
check: fmt build test doc-test clippy docs
    @cargo run --locked --package xtask -- all
architecture:
    cargo run --locked --package xtask -- architecture-check
source-layout:
    cargo run --locked --package xtask -- source-layout-check
reproducibility:
    cargo run --locked --package xtask -- reproducibility-check
trust:
    cargo run --locked --package xtask -- verify-trust
ordinary-api:
    cargo run --locked --package xtask -- ordinary-api-check
licenses:
    cargo deny --locked check bans licenses sources
deny:
    cargo deny --locked check
toolchain:
    cargo run --locked --package xtask -- toolchain-check
verus-verify:
    cargo verus verify --workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20
    cargo verus verify --package peritus-agent --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-context --package peritus-evidence --package peritus-gates --package peritus-git --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-memory --package peritus-migrations --package peritus-model-protocol --package peritus-network --package peritus-patch --package peritus-policy --package peritus-process --package peritus-projection --package peritus-protocol --package peritus-provider-anthropic --package peritus-provider-compatible --package peritus-provider-core --package peritus-provider-google --package peritus-provider-openai --package peritus-quality-policy --package peritus-role --package peritus-sandbox --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-secrets --package peritus-spec --package peritus-telemetry --package peritus-tool-protocol --package peritus-tool-router --package peritus-tools-fs --package peritus-tools-git --package peritus-tools-quality --package peritus-tools-shell --package peritus-trace --package peritus-types --package peritus-workspace --all-features --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
verus-build:
    cargo verus build --workspace --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --rlimit 20
    cargo verus build --package peritus-agent --package peritus-approval --package peritus-artifact-store --package peritus-budget --package peritus-codec --package peritus-context --package peritus-evidence --package peritus-gates --package peritus-git --package peritus-journal --package peritus-kernel --package peritus-leases --package peritus-memory --package peritus-migrations --package peritus-model-protocol --package peritus-network --package peritus-patch --package peritus-policy --package peritus-process --package peritus-projection --package peritus-protocol --package peritus-provider-anthropic --package peritus-provider-compatible --package peritus-provider-core --package peritus-provider-google --package peritus-provider-openai --package peritus-quality-policy --package peritus-role --package peritus-sandbox --package peritus-sandbox-linux --package peritus-sandbox-macos --package peritus-sandbox-windows --package peritus-secrets --package peritus-spec --package peritus-telemetry --package peritus-tool-protocol --package peritus-tool-router --package peritus-tools-fs --package peritus-tools-git --package peritus-tools-quality --package peritus-tools-shell --package peritus-trace --package peritus-types --package peritus-workspace --all-features --release --locked --check-toolchain --fwd-verus-args-to roots -- --no-cheating --rlimit 20
gate-a: check ordinary-api deny toolchain verus-verify verus-build
";

#[test]
fn canonical_gate_accepts_quiet_sigil_and_complete_closure() {
    assert!(validate(CANONICAL).is_empty());
}

#[test]
fn gutted_gate_with_opaque_shell_is_rejected() {
    let diagnostics =
        validate("gate-a:\n    sh -c 'cargo metadata --format-version 1 >/dev/null'\n");
    assert_message(&diagnostics, "exact reviewed gate dependencies");
    assert_message(&diagnostics, "outside the failure-propagating command model");
}

#[test]
fn ignored_failure_sigil_and_partial_deny_are_rejected() {
    let altered = CANONICAL
        .replace("cargo deny --locked check", "-cargo deny --locked check licenses")
        .replace("gate-a: check ordinary-api deny", "gate-a: check");
    let diagnostics = validate(&altered);
    assert_message(&diagnostics, "failure-ignoring sigil");
    assert_message(&diagnostics, "exact canonical Cargo operation");
    assert_message(&diagnostics, "exact reviewed gate dependencies");
}

#[test]
fn multiline_commands_are_validated_after_joining() {
    let altered = CANONICAL.replace(
        "cargo test --workspace --all-targets --all-features --locked",
        "cargo test --workspace \\\n+      --all-targets --all-features",
    );
    let diagnostics = validate(&altered);
    assert_message(&diagnostics, "without --locked");
}

#[test]
fn local_script_cannot_hide_unlocked_cargo_from_the_just_gate() {
    let altered = CANONICAL.replace("@cargo run --locked --package xtask -- all", "@./ci.sh");
    let diagnostics = validate(&altered);
    assert_message(&diagnostics, "unaudited executable");
    assert_message(&diagnostics, "exact canonical Cargo operation");
}

#[test]
fn custom_just_shell_setting_cannot_hijack_canonical_recipes() {
    let altered = format!("set shell := [\"./ci.sh\"]\n{CANONICAL}");
    let diagnostics = validate(&altered);
    assert_message(&diagnostics, "unsupported top-level Just directive");
}

#[test]
fn recipe_shebang_cannot_swallow_a_canonical_cargo_line() {
    let altered =
        CANONICAL.replace("build:\n    cargo build", "build:\n    #!/bin/true\n    cargo build");
    let diagnostics = validate(&altered);
    assert_message(&diagnostics, "forbidden Just shebang/script mode");
}

#[test]
fn gate_dependencies_cannot_mask_gutted_canonical_operations() {
    let altered = CANONICAL.replace(
        "cargo build --workspace --all-targets --all-features --locked",
        "printf build-skipped",
    );
    let diagnostics = validate(&altered);
    assert_message(&diagnostics, "does not contain its exact canonical Cargo operation");
}

#[test]
fn just_rejects_wrapper_and_path_assignments_but_retains_exact_docs_flags() {
    assert!(validate(CANONICAL).is_empty(), "canonical docs assignment must remain valid");
    for altered in [
        CANONICAL.replace(
            "cargo build --workspace --all-targets --all-features --locked",
            "RUSTC_WRAPPER=./evil cargo build --workspace --all-targets --all-features --locked",
        ),
        CANONICAL.replace(
            "cargo test --workspace --all-targets --all-features --locked",
            "PATH=./attacker cargo test --workspace --all-targets --all-features --locked",
        ),
    ] {
        assert_ne!(altered, CANONICAL, "fixture mutation must apply");
        assert_message(&validate(&altered), "forbidden leading environment assignments");
    }
}

#[test]
fn canonical_gate_invokes_xtask_directly_without_the_cargo_alias() {
    assert!(
        validate_contents_with_policy(CANONICAL, CommandPolicy::new(false)).is_empty(),
        "gate-bearing recipes must not rely on the repo-controlled cargo xtask alias"
    );
}

#[test]
fn gate_and_check_dependencies_reject_reordering_and_duplicates() {
    for altered in [
        CANONICAL.replace(
            "check: fmt build test doc-test clippy docs",
            "check: build fmt test doc-test clippy docs",
        ),
        CANONICAL.replace(
            "gate-a: check ordinary-api deny toolchain verus-verify verus-build",
            "gate-a: check ordinary-api deny deny toolchain verus-verify verus-build",
        ),
    ] {
        assert_ne!(altered, CANONICAL, "fixture mutation must apply");
        assert_message(&validate(&altered), "exact reviewed gate dependencies");
    }
}

fn validate(contents: &str) -> Vec<Diagnostic> {
    validate_contents_with_policy(contents, CommandPolicy::new(true))
}

fn validate_contents_with_policy(contents: &str, policy: CommandPolicy) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    validate_contents(contents, policy, &mut diagnostics);
    diagnostics
}

fn assert_message(diagnostics: &[Diagnostic], expected: &str) {
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message().contains(expected)),
        "expected a diagnostic containing `{expected}`, got {diagnostics:?}"
    );
}
