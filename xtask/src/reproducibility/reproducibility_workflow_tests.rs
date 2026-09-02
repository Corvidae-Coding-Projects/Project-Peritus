use super::reproducibility_workflow_fixture::canonical_ci;
use super::workflow_command_policy::CommandPolicy;
use super::workflow_files::DocumentKind;
use super::workflow_policy::{validate as validate_repository, validate_document};
use crate::error::Diagnostic;
use crate::model::{ToolchainArchive, ToolchainArchives, ToolchainPolicy};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const RUST: &str = "1.97.1";
const VERUS: &str = "0.2026.08.09.92f466f";
const DIGEST: &str = "2f5a41c553f424aacdd732339e9d125563716a0b003c27730f75d6f81a282cef";
const ACTION_SHA: &str = "3d3c42e5aac5ba805825da76410c181273ba90b1";

#[test]
fn stable_rust_and_zero_archive_digest_do_not_match_toolchains_policy() {
    let yaml = format!(
        r"
name: altered pins
env:
  RUST_VERSION: stable
  VERUS_VERSION: {VERUS}
  VERUS_LINUX_SHA256: {zero_digest}
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: dtolnay/rust-toolchain@{ACTION_SHA}
        with:
          toolchain: stable
",
        zero_digest = "0".repeat(64),
    );

    let (_, diagnostics) = validate(".github/workflows/ci.yml", DocumentKind::Workflow, &yaml);

    assert_message(&diagnostics, "CI `RUST_VERSION` does not match toolchains.toml");
    assert_message(&diagnostics, "CI `VERUS_LINUX_SHA256` does not match toolchains.toml");
    assert_message(&diagnostics, "CI `toolchain` does not select toolchains.toml Rust");
}

#[test]
fn gutted_canonical_ci_rejects_unused_pins_and_missing_required_operations() {
    let yaml = format!(
        r"
name: gutted
env:
  RUST_VERSION: {RUST}
  VERUS_VERSION: {VERUS}
  VERUS_LINUX_SHA256: {DIGEST}
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: echo policy removed
",
    );

    let (_, diagnostics) = validate(".github/workflows/ci.yml", DocumentKind::Workflow, &yaml);

    assert_message(&diagnostics, "does not install RUST_VERSION");
    assert_message(&diagnostics, "does not download and verify the exact pinned Verus archive");
    assert_message(&diagnostics, "does not run the direct locked xtask toolchain check");
    assert_message(&diagnostics, "does not run the direct ordinary-Rust formal API contract check");
    assert_message(&diagnostics, "does not retain the exact Verus shard matrix");
}

#[test]
fn canonical_ci_retains_pin_consumption_and_all_required_operations() {
    let yaml = canonical_ci();

    let (count, diagnostics) = validate(".github/workflows/ci.yml", DocumentKind::Workflow, &yaml);

    assert_eq!(count, 13);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

#[test]
fn conditional_fake_and_failure_masked_steps_cannot_supply_ci_evidence() {
    let yaml = canonical_ci()
        .replace("  verus:\n", "  verus:\n    if: false\n")
        .replace("sha256sum --check --strict", "sha256sum --check --strict || true")
        .replace(
            "run: cargo run --locked --package xtask -- toolchain-check",
            "run: echo cargo run --locked --package xtask -- toolchain-check",
        )
        .replace("run: cargo verus", "run: echo cargo verus");
    let (_, diagnostics) = validate(".github/workflows/ci.yml", DocumentKind::Workflow, &yaml);
    assert_message(&diagnostics, "does not install RUST_VERSION");
    assert_message(&diagnostics, "outside the failure-propagating command model");
}

#[test]
fn archive_evidence_binds_exact_url_digest_operand_and_order() {
    for altered in [
        canonical_ci().replace("github.com/verus-lang", "attacker.invalid/verus-lang"),
        canonical_ci()
            .replace("\"$VERUS_LINUX_SHA256\" \"$archive\"", "\"$VERUS_LINUX_SHA256\" \"$other\""),
        canonical_ci().replace("          unzip -q \"$archive\" -d \"$install_root\"\n", ""),
    ] {
        assert_ne!(altered, canonical_ci(), "adversarial fixture mutation must change CI");
        let (_, diagnostics) =
            validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, "does not download and verify the exact pinned Verus archive");
    }
}

#[test]
fn required_steps_must_be_ordered_unconditional_and_failure_propagating() {
    for altered in [
        canonical_ci().replace(
            "      - name: Run reviewed Verus package shard\n        run:",
            "      - name: Run reviewed Verus package shard\n        continue-on-error: true\n        run:",
        ),
        canonical_ci().replace(
            "      - name: Run reviewed Verus package shard\n        run:",
            "      - name: Run reviewed Verus package shard\n        background: true\n        id: verify\n        run:",
        ),
        canonical_ci().replace(
            "cargo run --locked --package xtask -- ci-shard ${{ matrix.operation }} ${{ matrix.shard }}",
            "cargo run --locked --package xtask -- ci-shard ${{ matrix.operation }} ${{ matrix.shard }} &",
        ),
    ] {
        assert_ne!(altered, canonical_ci(), "adversarial fixture mutation must change CI");
        let (_, diagnostics) =
            validate(".github/workflows/ci.yml", DocumentKind::Workflow, &altered);
        assert_message(&diagnostics, "does not retain the exact Verus shard matrix");
    }
}

#[test]
fn rust_pin_must_be_bound_to_the_official_action_in_the_same_chain() {
    let yaml = canonical_ci().replace("dtolnay/rust-toolchain@", "attacker/toolchain@");
    let (_, diagnostics) = validate(".github/workflows/ci.yml", DocumentKind::Workflow, &yaml);
    assert_message(&diagnostics, "does not install RUST_VERSION");
}

#[test]
fn quoted_action_tag_is_not_mistaken_for_an_immutable_revision() {
    let yaml = r#"
name: quoted tag
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - { name: checkout, uses: "actions/checkout@v7" }
"#;

    let (count, diagnostics) =
        validate(".github/workflows/extra.yml", DocumentKind::Workflow, yaml);

    assert_eq!(count, 1);
    assert_message(&diagnostics, "without an immutable commit SHA");
}

#[test]
fn unlocked_cargo_in_an_extra_workflow_is_rejected() {
    let yaml = r"
name: bypass attempt
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: >-
          cargo test
          --workspace
";

    let (_, diagnostics) = validate(".github/workflows/extra.yaml", DocumentKind::Workflow, yaml);

    assert_message(&diagnostics, "without --locked");
}

#[test]
fn every_hosted_job_requires_a_ten_minute_or_shorter_timeout() {
    let yaml = r"
name: slow runner
jobs:
  check:
    runs-on: ubuntu-latest
    timeout-minutes: 11
    steps:
      - run: cargo test --workspace --locked
";

    let (_, diagnostics) = validate(".github/workflows/extra.yaml", DocumentKind::Workflow, yaml);

    assert_message(&diagnostics, "timeout from 1 through 10 minutes");
}

#[test]
fn composite_action_cannot_hide_tagged_uses_or_unlocked_cargo() {
    let yaml = r"
name: composite bypass
runs:
  using: composite
  steps:
    - { uses: 'vendor/action@stable' }
    - shell: bash
      run: cargo build --workspace
";

    let (count, diagnostics) =
        validate(".github/actions/bypass/action.yml", DocumentKind::Action, yaml);

    assert_eq!(count, 1);
    assert_message(&diagnostics, "without an immutable commit SHA");
    assert_message(&diagnostics, "without --locked");
}

#[test]
fn flow_mapping_with_pinned_action_and_locked_command_is_valid() {
    let yaml = format!(
        r#"
name: valid composite
runs:
  using: composite
  steps:
    - {{ uses: "actions/checkout@{ACTION_SHA}" }}
    - {{ shell: bash, run: "cargo test --workspace --locked" }}
"#,
    );

    let (count, diagnostics) =
        validate(".github/actions/valid/action.yaml", DocumentKind::Action, &yaml);

    assert_eq!(count, 1);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

#[test]
fn local_actions_cannot_escape_the_checked_action_directory() {
    let yaml = r"
name: local escape
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: ./../unreviewed-action
";

    let (_, diagnostics) = validate(".github/workflows/extra.yml", DocumentKind::Workflow, yaml);

    assert_message(&diagnostics, "outside checked-in policy");
}

#[test]
fn extra_workflow_and_nested_composite_action_are_both_discovered() {
    let fixture = Fixture::new();
    fixture.write(".github/workflows/ci.yml", &canonical_ci());
    fixture.write(
        ".github/workflows/extra.yaml",
        "name: extra\njobs: { check: { runs-on: ubuntu-latest, steps: [ { run: 'cargo test' } ] } }\n",
    );
    fixture.write(
        ".github/actions/nested/tagged/action.yml",
        "name: tagged\nruns: { using: composite, steps: [ { uses: 'vendor/action@stable' } ] }\n",
    );
    let mut diagnostics = Vec::new();

    fixture.write(".cargo/config.toml", "[alias]\nxtask = \"run --locked --package xtask --\"\n");
    let count =
        validate_repository(fixture.path(), &policy(), CommandPolicy::new(true), &mut diagnostics)
            .expect("fixture files must be readable");

    assert_eq!(count, 14);
    assert_message(&diagnostics, "without --locked");
    assert_message(&diagnostics, "without an immutable commit SHA");
}

#[test]
fn referenced_local_node_action_is_rejected_as_an_uninspected_payload() {
    let fixture = Fixture::new();
    fixture.write(".github/workflows/ci.yml", &canonical_ci());
    fixture.write(
        ".github/workflows/node.yml",
        "name: node\njobs: { check: { runs-on: ubuntu-latest, steps: [ { uses: './.github/actions/node' } ] } }\n",
    );
    fixture.write(
        ".github/actions/node/action.yml",
        "name: node\nruns: { using: node20, main: index.js }\n",
    );
    fixture.write(".github/actions/node/index.js", "// could spawn unlocked Cargo\n");
    fixture.write(".cargo/config.toml", "[alias]\nxtask = \"run --locked --package xtask --\"\n");
    let mut diagnostics = Vec::new();
    validate_repository(fixture.path(), &policy(), CommandPolicy::new(true), &mut diagnostics)
        .expect("fixture files must be readable");
    assert_message(&diagnostics, "outside checked-in policy");
}

pub(super) fn validate(path: &str, kind: DocumentKind, yaml: &str) -> (usize, Vec<Diagnostic>) {
    let root = Path::new("/tmp/peritus-reproducibility-workflow-test");
    let policy = policy();
    let mut diagnostics = Vec::new();
    let count = validate_document(
        root,
        &root.join(path),
        kind,
        yaml,
        &policy,
        CommandPolicy::new(true),
        &mut diagnostics,
    );
    (count, diagnostics)
}

fn policy() -> ToolchainPolicy {
    ToolchainPolicy {
        schema: 1,
        rust: RUST.to_owned(),
        verus: VERUS.to_owned(),
        vstd_revision: "92f466f247f45128c630d1c843fd6e27d2115587".to_owned(),
        z3: "4.16.0".to_owned(),
        cargo_verus_advertised_z3: "4.12.5".to_owned(),
        archives: ToolchainArchives {
            linux_x86_64: ToolchainArchive {
                url: format!(
                    "https://github.com/verus-lang/verus/releases/download/release/{VERUS}/verus-{VERUS}-x86-linux.zip"
                ),
                sha256: DIGEST.to_owned(),
            },
        },
    }
}

pub(super) fn assert_message(diagnostics: &[Diagnostic], expected: &str) {
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message().contains(expected)),
        "expected a diagnostic containing `{expected}`, got {diagnostics:?}"
    );
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("peritus-reproducibility-workflow-{}-{id}", std::process::id()));
        fs::create_dir(&root).expect("fixture root must be creatable");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture path must have a parent"))
            .expect("fixture directory must be creatable");
        fs::write(path, contents).expect("fixture file must be writable");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("fixture root must be removable");
    }
}
