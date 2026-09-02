use super::reproducibility_workflow_fixture::canonical_governance;
use super::reproducibility_workflow_tests::{assert_message, validate};
use super::workflow_files::DocumentKind;

const PATH: &str = ".github/workflows/formal-governance.yml";

#[test]
fn canonical_team_workflow_retains_every_gate_and_stable_status() {
    let (action_count, diagnostics) =
        validate(PATH, DocumentKind::Workflow, &canonical_governance());

    assert_eq!(action_count, 14);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

#[test]
fn required_workflow_rejects_trigger_permission_and_environment_drift() {
    for altered in [
        canonical_governance().replace("  pull_request:\n", "  pull_request_target:\n"),
        canonical_governance().replace("    branches: [main]", "    branches: [feature]"),
        canonical_governance().replace("  contents: read", "  contents: write"),
        canonical_governance().replace(
            "permissions:\n  contents: read",
            "concurrency:\n  cancel-in-progress: true\n\npermissions:\n  contents: read",
        ),
        canonical_governance().replace("  RUSTUP_TOOLCHAIN: 1.97.1\n", ""),
        canonical_governance().replace(
            "github.event.pull_request.base.sha || github.event.merge_group.base_sha || github.event.before",
            "github.sha",
        ),
    ] {
        assert_ne!(altered, canonical_governance());
        let (_, diagnostics) = validate(PATH, DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, "mutable triggers, permissions, environment, or job topology");
    }
}

#[test]
fn required_workflow_rejects_candidate_policy_weakening() {
    for altered in [
        canonical_governance().replace("--package xtask -- all", "--package impostor -- all"),
        canonical_governance()
            .replace("Evaluate candidate policy", "Pretend to evaluate candidate policy"),
        canonical_governance().replace(
            "  policy:\n    name: Candidate policy",
            "  policy:\n    if: false\n    name: Candidate policy",
        ),
    ] {
        assert_ne!(altered, canonical_governance());
        let (_, diagnostics) = validate(PATH, DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, "does not evaluate the candidate policy exactly");
    }
}

#[test]
fn required_workflow_rejects_every_candidate_checkout_identity_drift() {
    let checkout = "          repository: ${{ github.repository }}\n          ref: ${{ github.sha }}\n          path: candidate\n          fetch-depth: 0\n          persist-credentials: false";
    for altered_checkout in [
        "          ref: ${{ github.sha }}\n          path: candidate\n          fetch-depth: 0\n          persist-credentials: false",
        "          repository: attacker/example\n          ref: ${{ github.sha }}\n          path: candidate\n          fetch-depth: 0\n          persist-credentials: false",
        "          repository: ${{ github.repository }}\n          ref: refs/heads/main\n          path: candidate\n          fetch-depth: 0\n          persist-credentials: false",
        "          repository: ${{ github.repository }}\n          ref: ${{ github.sha }}\n          path: workspace\n          fetch-depth: 0\n          persist-credentials: false",
        "          repository: ${{ github.repository }}\n          ref: ${{ github.sha }}\n          path: candidate\n          fetch-depth: 1\n          persist-credentials: false",
        "          repository: ${{ github.repository }}\n          ref: ${{ github.sha }}\n          path: candidate\n          fetch-depth: 0\n          persist-credentials: false\n          submodules: true",
    ] {
        let altered = canonical_governance().replacen(checkout, altered_checkout, 1);
        assert_ne!(altered, canonical_governance());
        let (_, diagnostics) = validate(PATH, DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, "lacks the exact pre-Cargo candidate bootstrap");
    }
}

#[test]
fn required_workflow_rejects_bootstrap_and_gate_weakening() {
    for (altered, expected) in [
        (
            canonical_governance().replace(
                "6ca5f56d2ab12e93f155d684b33f4a86c2f877b8",
                "0000000000000000000000000000000000000000",
            ),
            "lacks the exact pre-Cargo candidate bootstrap",
        ),
        (
            canonical_governance().replace("--no-textconv", "--textconv"),
            "lacks the exact pre-Cargo candidate bootstrap",
        ),
        (
            canonical_governance().replace("  rust-shards:\n", "  rust-shards:\n    if: false\n"),
            "does not retain every hardcoded job and final status",
        ),
        (
            canonical_governance()
                .replace("cargo deny --locked check", "cargo deny --locked check licenses"),
            "does not retain every hardcoded job and final status",
        ),
        (
            canonical_governance().replace("verus-build-strict]", "verus-build]"),
            "does not retain every hardcoded job and final status",
        ),
        (
            canonical_governance().replace("    if: always()", "    if: success()"),
            "does not retain every hardcoded job and final status",
        ),
        (
            canonical_governance().replace(
                "needs: [policy, workflow-lint, rust, supply-chain, verus]",
                "needs: [policy, workflow-lint, rust, supply-chain]",
            ),
            "does not retain every hardcoded job and final status",
        ),
        (
            canonical_governance()
                .replace("test \"$VERUS_RESULT\" = success", "test \"$VERUS_RESULT\" != success"),
            "does not retain every hardcoded job and final status",
        ),
    ] {
        assert_ne!(altered, canonical_governance());
        let (_, diagnostics) = validate(PATH, DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, expected);
    }
}

#[test]
fn required_workflow_retains_the_measured_verus_timeout() {
    let altered = canonical_governance().replacen(
        "    name: Verus shard ${{ matrix.operation }} ${{ matrix.shard }}\n    needs: policy\n    strategy:\n      fail-fast: false\n      matrix:\n        operation: [verus-verify, verus-verify-strict, verus-build, verus-build-strict]\n        shard: [foundation-state, runtime-tools, model-orchestration, app, edge]\n    runs-on: ubuntu-24.04\n    timeout-minutes: 10",
        "    name: Verus shard ${{ matrix.operation }} ${{ matrix.shard }}\n    needs: policy\n    strategy:\n      fail-fast: false\n      matrix:\n        operation: [verus-verify, verus-verify-strict, verus-build, verus-build-strict]\n        shard: [foundation-state, runtime-tools, model-orchestration, app, edge]\n    runs-on: ubuntu-24.04\n    timeout-minutes: 9",
        1,
    );

    assert_ne!(altered, canonical_governance());
    let (_, diagnostics) = validate(PATH, DocumentKind::Workflow, &altered);
    assert_message(&diagnostics, "does not retain every hardcoded job and final status");
}

#[test]
fn required_workflow_retains_the_measured_rust_matrix_timeout() {
    let altered = canonical_governance().replacen(
        "    name: Rust shard ${{ matrix.operation }} ${{ matrix.shard }} (${{ matrix.os }})\n    needs: policy\n    strategy:\n      fail-fast: false\n      matrix:\n        os: [ubuntu-24.04, macos-15, windows-2025]\n        operation: [build, test, doc-test, clippy, docs]\n        shard: [foundation-state, runtime-tools, model-orchestration, app, testing, edge]\n    runs-on: ${{ matrix.os }}\n    timeout-minutes: 10",
        "    name: Rust shard ${{ matrix.operation }} ${{ matrix.shard }} (${{ matrix.os }})\n    needs: policy\n    strategy:\n      fail-fast: false\n      matrix:\n        os: [ubuntu-24.04, macos-15, windows-2025]\n        operation: [build, test, doc-test, clippy, docs]\n        shard: [foundation-state, runtime-tools, model-orchestration, app, testing, edge]\n    runs-on: ${{ matrix.os }}\n    timeout-minutes: 9",
        1,
    );

    assert_ne!(altered, canonical_governance());
    let (_, diagnostics) = validate(PATH, DocumentKind::Workflow, &altered);
    assert_message(&diagnostics, "does not retain every hardcoded job and final status");
}

#[test]
fn required_workflow_is_an_exact_reviewed_definition() {
    let altered = canonical_governance().replace("name: Gate A", "name: Locally renamed gate");

    let (_, diagnostics) = validate(PATH, DocumentKind::Workflow, &altered);

    assert_message(&diagnostics, "differs from its reviewed definition");
}
