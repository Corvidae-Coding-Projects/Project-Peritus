use super::reproducibility_workflow_tests::{assert_message, validate};
use super::workflow_authority::PATH;
use super::workflow_files::DocumentKind;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const AUTHORITY_INPUTS: &[&str] = &[
    ".cargo/config.toml",
    ".gitattributes",
    "rust-toolchain.toml",
    "Cargo.toml",
    "Cargo.lock",
    "xtask/Cargo.toml",
    "xtask/build.rs",
    "xtask/src",
    ".github/workflows/formal-authority.yml",
    ".github/workflows/formal-governance.yml",
    "docs/formal-governance-ruleset.template.json",
];

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

fn canonical() -> String {
    include_str!("canonical/formal-authority.yml").to_owned()
}

#[test]
fn canonical_authority_workflow_is_exact_and_pinned() {
    let (actions, diagnostics) = validate(PATH, DocumentKind::Workflow, &canonical());

    assert_eq!(actions, 6);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
}

#[test]
fn candidate_cannot_select_or_build_its_own_checker() {
    for (before, after) in [
        ("--manifest-path authority/Cargo.toml", "--manifest-path candidate/Cargo.toml"),
        (
            "checker=\"$RUNNER_TEMP/formal-authority-target/release/xtask\"",
            "checker=\"$GITHUB_WORKSPACE/candidate/target/release/xtask\"",
        ),
        ("PATH=\"$trusted_path\" \"$checker\" all", "cargo run --locked --package xtask -- all"),
    ] {
        assert_rejected(&canonical().replace(before, after));
    }
}

#[test]
fn both_policy_passes_are_mandatory_and_ordered() {
    for altered in [
        canonical().replace("\"$checker\" all", "\"$checker\" verify-trust"),
        canonical().replace("\"$checker\" verify-trust", "\"$checker\" all"),
        canonical().replace(
            "          PERITUS_PROOF_IMPACT_BASE: ${{ github.event.pull_request.base.sha }}\n",
            "",
        ),
        canonical().replace("          GITHUB_ACTIONS: \"true\"\n", ""),
    ] {
        assert_rejected(&altered);
    }
}

#[test]
fn revision_custody_and_pre_metadata_guards_cannot_be_relaxed() {
    for (before, after) in [
        ("ref: ${{ github.workflow_sha }}", "ref: ${{ github.event.pull_request.base.sha }}"),
        ("ref: ${{ github.event.pull_request.base.sha }}", "ref: ${{ github.base_ref }}"),
        ("ref: ${{ github.event.pull_request.head.sha }}", "ref: ${{ github.sha }}"),
        ("fetch-depth: 0", "fetch-depth: 1"),
        ("persist-credentials: false", "persist-credentials: true"),
        ("require_sha CHECKER_SHA \"$CHECKER_SHA\"", "true"),
        ("test \"$EVENT_NAME\" = \"pull_request_target\"", "true"),
        ("test \"$DEFAULT_BRANCH\" = \"main\"", "true"),
        ("test \"$EVENT_SHA\" = \"$CHECKER_SHA\"", "true"),
        ("test \"$EVENT_REF\" = \"refs/heads/main\"", "true"),
        (
            "test \"$WORKFLOW_REF\" = \"$GITHUB_REPOSITORY/.github/workflows/formal-authority.yml@refs/heads/main\"",
            "true",
        ),
        ("test \"$BASE_REPOSITORY\" = \"$GITHUB_REPOSITORY\"", "true"),
        ("test \"$BASE_REPOSITORY_ID\" = \"$REPOSITORY_ID\"", "true"),
        ("main) test \"$BASE_SHA\" = \"$CHECKER_SHA\" ;;", "main) ;;"),
        ("git -C candidate merge-base --is-ancestor \"$BASE_SHA\" \"$CANDIDATE_SHA\"", "true"),
        ("check_tree authority \"$CHECKER_SHA\" checker", "true"),
        ("check_tree base \"$BASE_SHA\" base", "true"),
        ("check_tree candidate \"$CANDIDATE_SHA\" candidate", "true"),
        ("allow-unsafe-pr-checkout: true", "allow-unsafe-pr-checkout: false"),
        ("100644:blob|100755:blob", "*:blob"),
        (".cargo/config|rust-toolchain", "rust-toolchain"),
        ("CARGO_NET_OFFLINE: \"true\"", "CARGO_NET_OFFLINE: \"false\""),
        (
            "${{ runner.temp }}/formal-authority-cargo-home",
            "${{ github.workspace }}/candidate/.cargo-home",
        ),
    ] {
        assert_rejected(&canonical().replace(before, after));
    }
}

#[test]
fn checker_build_inputs_cannot_drift_on_either_custody_edge() {
    for (before, after) in [
        (".cargo/config.toml .gitattributes rust-toolchain.toml", ".gitattributes"),
        ("Cargo.toml Cargo.lock", "Cargo.toml"),
        (
            "xtask/Cargo.toml xtask/build.rs xtask/src",
            "xtask/Cargo.toml xtask/build.rs xtask/tests",
        ),
        (".github/workflows/formal-authority.yml", ".github/workflows/formal-authority.yaml"),
        (".github/workflows/formal-governance.yml", ".github/workflows/formal-governance.yaml"),
        ("docs/formal-governance-ruleset.template.json", "docs/github-governance.md"),
        (
            "\"$CHECKER_SHA\" \"$BASE_SHA\" -- \"${authority_inputs[@]}\"",
            "\"$CHECKER_SHA\" \"$CHECKER_SHA\" -- \"${authority_inputs[@]}\"",
        ),
        (
            "\"$BASE_SHA\" \"$CANDIDATE_SHA\" -- \"${authority_inputs[@]}\"",
            "\"$BASE_SHA\" \"$BASE_SHA\" -- \"${authority_inputs[@]}\"",
        ),
    ] {
        assert_rejected(&canonical().replace(before, after));
    }
}

#[test]
fn protected_input_transition_classification_cannot_be_bypassed_or_misreported() {
    for altered in [
        canonical().replace(
            "changed: ${{ steps.classify.outputs.changed }}",
            "changed: false",
        ),
        canonical().replace(
            "if: needs.protected-input-classification.outputs.changed == 'false'",
            "if: always()",
        ),
        canonical().replace("        id: classify\n", ""),
        canonical().replace(
            "git -C candidate diff --no-ext-diff --no-textconv --quiet",
            "git -C candidate diff --quiet",
        ),
        canonical().replace(
            "printf 'changed=true\\n' >> \"$GITHUB_OUTPUT\"",
            "printf 'changed=false\\n' >> \"$GITHUB_OUTPUT\"",
        ),
        canonical().replace(
            "*)\n              printf 'failed to classify protected authority inputs",
            "*)\n              printf 'changed=false\\n' >> \"$GITHUB_OUTPUT\"\n              printf 'failed to classify protected authority inputs",
        ),
    ] {
        assert_rejected(&altered);
    }
}

#[test]
fn unchanged_develop_base_and_candidate_preserve_both_custody_edges() {
    let unchanged = GitFixture::new();
    unchanged.write("xtask/src/lib.rs", "pub fn authority() -> bool { true }\n");
    unchanged.commit("checker");
    let checker = unchanged.head();
    unchanged.write("docs/base-unrelated.md", "# Develop base\n");
    unchanged.commit("develop base");
    let base = unchanged.head();
    unchanged.write("docs/candidate-unrelated.md", "# Candidate\n");
    unchanged.commit("candidate");
    let candidate = unchanged.head();

    assert!(unchanged.diff(&checker, &base).success());
    assert!(unchanged.diff(&base, &candidate).success());
}

#[test]
fn checker_source_only_drift_in_develop_base_fails_closed() {
    let drift = GitFixture::new();
    drift.write("xtask/src/lib.rs", "pub fn authority() -> bool { true }\n");
    drift.commit("checker");
    let checker = drift.head();
    drift.write("xtask/src/lib.rs", "pub fn authority() -> bool { false }\n");
    drift.commit("develop checker drift");
    let base = drift.head();
    drift.write("docs/unrelated.md", "# Candidate\n");
    drift.commit("candidate");
    let candidate = drift.head();

    assert!(!drift.diff(&checker, &base).success());
    assert!(drift.diff(&base, &candidate).success());
}

#[test]
fn checker_source_only_drift_in_candidate_is_detected_for_bootstrap() {
    let drift = GitFixture::new();
    drift.write("xtask/src/lib.rs", "pub fn authority() -> bool { true }\n");
    drift.commit("checker");
    let checker = drift.head();
    drift.write("docs/unrelated.md", "# Develop base\n");
    drift.commit("develop base");
    let base = drift.head();
    drift.write("xtask/src/lib.rs", "pub fn authority() -> bool { false }\n");
    drift.commit("candidate checker drift");
    let candidate = drift.head();

    assert!(drift.diff(&checker, &base).success());
    assert!(!drift.diff(&base, &candidate).success());
}

#[test]
fn externally_installed_checker_is_a_later_unchanged_authority_baseline() {
    let installed = GitFixture::new();
    installed.write("xtask/src/lib.rs", "pub fn authority() -> bool { false }\n");
    installed.commit("externally installed checker authority");
    let checker = installed.head();
    installed.write("docs/base-unrelated.md", "# Develop base\n");
    installed.commit("develop base after bootstrap");
    let base = installed.head();
    installed.write("docs/candidate-unrelated.md", "# Candidate\n");
    installed.commit("candidate");
    let candidate = installed.head();

    assert!(installed.diff(&checker, &base).success());
    assert!(installed.diff(&base, &candidate).success());
}

#[test]
fn target_identity_permissions_and_failure_masking_are_rejected() {
    for altered in [
        canonical().replace("contents: read", "contents: write"),
        canonical().replace(
            "uses: dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772",
            "uses: ./candidate/.github/actions/setup-rust",
        ),
        canonical().replace(
            "      - name: Evaluate candidate with trusted checker\n",
            "      - name: Evaluate candidate with trusted checker\n        continue-on-error: true\n",
        ),
        canonical().replace("pull_request_target:", "pull_request:"),
        canonical().replace("branches: [main, develop]", "branches: [main]"),
        canonical().replace("branches: [main, develop]", "branches: [develop]"),
        canonical().replace("branches: [main, develop]", "branches: [main, develop, staging]"),
        canonical().replace(
            "${{ github.workflow_sha }}-${{ github.event.pull_request.base.sha }}",
            "${{ github.event.pull_request.base.sha }}-${{ github.event.pull_request.base.sha }}",
        ),
        canonical().replace("CHECKER_SHA: ${{ github.workflow_sha }}", "CHECKER_SHA: ${{ github.sha }}"),
        canonical().replace(
            "PERITUS_PROOF_IMPACT_BASE: ${{ github.event.pull_request.base.sha }}",
            "PERITUS_PROOF_IMPACT_BASE: ${{ github.workflow_sha }}",
        ),
    ] {
        assert_rejected(&altered);
    }
}

fn assert_rejected(altered: &str) {
    assert_ne!(altered, canonical(), "fixture mutation must change the workflow");
    let (_, diagnostics) = validate(PATH, DocumentKind::Workflow, altered);
    assert_message(&diagnostics, "trusted-base authority workflow");
}

struct GitFixture {
    root: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
        let serial = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("peritus-authority-custody-{}-{serial}", std::process::id()));
        fs::create_dir(&root).expect("create authority custody fixture");
        run_git(&root, &["init", "--quiet"]);
        run_git(&root, &["config", "user.name", "Peritus test"]);
        run_git(&root, &["config", "user.email", "peritus@example.invalid"]);
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture file parent"))
            .expect("create fixture file parent");
        fs::write(path, contents).expect("write fixture file");
    }

    fn commit(&self, message: &str) {
        run_git(&self.root, &["add", "."]);
        run_git(&self.root, &["commit", "--quiet", "-m", message]);
    }

    fn head(&self) -> String {
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("read fixture HEAD");
        assert!(output.status.success());
        String::from_utf8(output.stdout).expect("fixture HEAD is UTF-8").trim().to_owned()
    }

    fn diff(&self, before: &str, after: &str) -> ExitStatus {
        Command::new("git")
            .current_dir(&self.root)
            .args(["diff", "--no-ext-diff", "--no-textconv", "--exit-code", before, after, "--"])
            .args(AUTHORITY_INPUTS)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("compare authority custody inputs")
    }
}

impl Drop for GitFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run_git(root: &Path, arguments: &[&str]) {
    assert!(
        Command::new("git")
            .current_dir(root)
            .args(arguments)
            .status()
            .expect("run fixture Git")
            .success(),
        "fixture Git command failed: {arguments:?}"
    );
}
