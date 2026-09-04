use super::reproducibility_workflow_fixture::canonical_ci;
use super::reproducibility_workflow_tests::{assert_message, validate};
use super::workflow_files::DocumentKind;

#[test]
fn rust_and_supply_chain_jobs_cannot_be_skipped_or_gutted() {
    for (altered, expected) in [
        (
            canonical_ci().replace("  rust:\n", "  rust:\n    if: false\n"),
            "exact unconditional Rust shard matrix",
        ),
        (
            canonical_ci().replace("  supply-chain:\n", "  supply-chain:\n    if: false\n"),
            "exact unconditional full supply-chain gate",
        ),
        (
            canonical_ci().replace(
                "        run: cargo deny --locked check",
                "        run: cargo deny --locked check licenses",
            ),
            "exact unconditional full supply-chain gate",
        ),
    ] {
        let (_, diagnostics) =
            validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, expected);
    }
}

#[test]
fn rust_shards_retain_the_ten_minute_ceiling() {
    let altered = canonical_ci().replacen(
        "    name: Foundation Rust ${{ matrix.operation }} ${{ matrix.shard }} (${{ matrix.os }})\n    needs: bootstrap\n    strategy:\n      fail-fast: false\n      matrix:\n        os: [ubuntu-24.04, macos-15, windows-2025]\n        operation: [build, test, doc-test, clippy, docs]\n        shard: [foundation-state, runtime-tools, model-orchestration, app-runner, app-shell, testing, edge]\n    runs-on: ${{ matrix.os }}\n    timeout-minutes: 10",
        "    name: Foundation Rust ${{ matrix.operation }} ${{ matrix.shard }} (${{ matrix.os }})\n    needs: bootstrap\n    strategy:\n      fail-fast: false\n      matrix:\n        os: [ubuntu-24.04, macos-15, windows-2025]\n        operation: [build, test, doc-test, clippy, docs]\n        shard: [foundation-state, runtime-tools, model-orchestration, app-runner, app-shell, testing, edge]\n    runs-on: ${{ matrix.os }}\n    timeout-minutes: 11",
        1,
    );

    assert_ne!(altered, canonical_ci(), "fixture mutation must change canonical CI");
    let (_, diagnostics) = validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
    assert_message(&diagnostics, "exact unconditional Rust shard matrix");
}

#[test]
fn verus_jobs_reject_needs_runner_shell_env_and_intervening_steps() {
    for altered in [
        canonical_ci().replace(
            "  verus-policy:\n    name: Foundation Verus policy\n    needs: bootstrap",
            "  verus-policy:\n    name: Foundation Verus policy\n    needs: rust",
        ),
        canonical_ci().replace(
            "  verus-policy:\n    name: Foundation Verus policy\n    needs: bootstrap\n    runs-on: ubuntu-24.04",
            "  verus-policy:\n    name: Foundation Verus policy\n    needs: bootstrap\n    runs-on: ubuntu-latest",
        ),
        canonical_ci().replace(
            "      - name: Probe every pinned tool component\n        run:",
            "      - name: Probe every pinned tool component\n        shell: ./ci.sh {0}\n        run:",
        ),
        canonical_ci().replace(
            "      - name: Probe every pinned tool component",
            "      - name: Intervening action\n        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n      - name: Probe every pinned tool component",
        ),
    ] {
        assert_ne!(altered, canonical_ci(), "fixture mutation must change canonical CI");
        let (_, diagnostics) =
            validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, "does not install RUST_VERSION");
    }

    let altered = canonical_ci()
        .replace("  VERUS_LINUX_SHA256:", "  PATH: ./attacker\n  VERUS_LINUX_SHA256:");
    let (_, diagnostics) = validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
    assert_message(&diagnostics, "environment is not the exact reviewed toolchain-pin set");
}

#[test]
fn verus_shards_retain_the_ten_minute_ceiling() {
    let altered = canonical_ci().replacen(
        "    name: Foundation Verus ${{ matrix.operation }} ${{ matrix.shard }}\n    needs: bootstrap\n    strategy:\n      fail-fast: false\n      matrix:\n        operation: [verus-verify, verus-verify-strict, verus-build, verus-build-strict]\n        shard: [foundation-state, runtime-tools, model-orchestration, app-runner, app-shell, edge]\n    runs-on: ubuntu-24.04\n    timeout-minutes: 10",
        "    name: Foundation Verus ${{ matrix.operation }} ${{ matrix.shard }}\n    needs: bootstrap\n    strategy:\n      fail-fast: false\n      matrix:\n        operation: [verus-verify, verus-verify-strict, verus-build, verus-build-strict]\n        shard: [foundation-state, runtime-tools, model-orchestration, app-runner, app-shell, edge]\n    runs-on: ubuntu-24.04\n    timeout-minutes: 11",
        1,
    );

    assert_ne!(altered, canonical_ci(), "fixture mutation must change canonical CI");
    let (_, diagnostics) = validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
    assert_message(&diagnostics, "exact Verus shard matrix");
}

#[test]
fn pre_cargo_bootstrap_and_needs_graph_are_exact() {
    for altered in [
        canonical_ci().replace("  bootstrap:\n", "  bootstrap:\n    if: false\n"),
        canonical_ci().replace(
            "6ca5f56d2ab12e93f155d684b33f4a86c2f877b8",
            "0000000000000000000000000000000000000000",
        ),
        canonical_ci().replace("--no-textconv", "--textconv"),
        canonical_ci().replace(
            "      - name: Verify reviewed pre-Cargo policy",
            "      - name: Unreviewed bootstrap step\n        run: printf ignored\n      - name: Verify reviewed pre-Cargo policy",
        ),
        canonical_ci().replace(
            "  rust:\n    name: Foundation Rust ${{ matrix.operation }} ${{ matrix.shard }} (${{ matrix.os }})\n    needs: bootstrap",
            "  rust:\n    name: Foundation Rust ${{ matrix.operation }} ${{ matrix.shard }} (${{ matrix.os }})",
        ),
    ] {
        assert_ne!(altered, canonical_ci(), "fixture mutation must change canonical CI");
        let (_, diagnostics) =
            validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.message().contains("pre-Cargo configuration bootstrap")
                    || diagnostic.message().contains("exact unconditional Rust shard matrix")
            }),
            "expected bootstrap/needs diagnostic, got {diagnostics:?}"
        );
    }
}

#[test]
fn workflow_root_controls_are_exact_and_automatic() {
    for altered in [
        canonical_ci().replace(
            "on:\n  push:\n    branches: [main]\n  pull_request:\n  workflow_dispatch:",
            "on:\n  workflow_dispatch:",
        ),
        canonical_ci()
            .replace("    branches: [main]", "    branches: [main]\n    paths: [docs/**]"),
        canonical_ci().replace("  contents: read", "  contents: write"),
        canonical_ci().replace("  cancel-in-progress: true", "  cancel-in-progress: false"),
        canonical_ci().replace(
            "permissions:\n  contents: read",
            "defaults:\n  run:\n    shell: bash {0} || true\n\npermissions:\n  contents: read",
        ),
    ] {
        let (_, diagnostics) =
            validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, "root triggers, permissions, or concurrency");
    }
}

#[test]
fn ci_cargo_steps_reject_wrapper_and_path_assignments() {
    for (needle, replacement) in [
        (
            "run: cargo run --locked --target-dir target/xtask-bootstrap --package xtask -- ci-shard ${{ matrix.operation }} ${{ matrix.shard }}",
            "run: RUSTC_WRAPPER=./evil cargo run --locked --target-dir target/xtask-bootstrap --package xtask -- ci-shard ${{ matrix.operation }} ${{ matrix.shard }}",
        ),
        ("run: cargo deny --locked check", "run: PATH=./attacker cargo deny --locked check"),
    ] {
        let altered = canonical_ci().replacen(needle, replacement, 1);
        assert_ne!(altered, canonical_ci(), "fixture mutation `{needle}` must apply");
        let (_, diagnostics) =
            validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, "forbidden leading environment assignments");
    }
}
