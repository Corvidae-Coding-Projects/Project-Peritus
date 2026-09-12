use super::{HARNESS, TARGETS, mutation, runner, validate_corpora};
use serde_json::json;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "peritus-discovery-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("unique fixture");
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove owned fixture");
    }
}

#[test]
fn missing_target_and_empty_corpus_cannot_pass() {
    let fixture = Fixture::new();
    let harness = fixture.0.join(HARNESS);
    fs::create_dir_all(&harness).expect("harness");
    let mut manifest = String::new();
    for name in TARGETS.into_iter().chain(["discovery-replay"]) {
        writeln!(manifest, "[[bin]]\nname = {name:?}").expect("write manifest");
    }
    fs::write(harness.join("Cargo.toml"), &manifest).expect("manifest");
    for target in TARGETS {
        let corpus = harness.join("corpus").join(target);
        fs::create_dir_all(&corpus).expect("corpus");
        fs::write(corpus.join("seed"), b"seed").expect("seed");
    }
    assert!(validate_corpora(&fixture.0).is_ok());
    fs::remove_file(harness.join("corpus/sse/seed")).expect("remove seed");
    assert!(validate_corpora(&fixture.0).is_err());
    fs::write(harness.join("corpus/sse/seed"), b"seed").expect("restore seed");
    fs::write(
        harness.join("Cargo.toml"),
        manifest.replace("name = \"ndjson\"", "name = \"missing\""),
    )
    .expect("mutated inventory");
    assert!(validate_corpora(&fixture.0).is_err());
}

#[test]
fn mutation_summary_distinguishes_behavior_build_failure_and_unexplored() {
    let inventory = json!([{"name":"caught","diff":"patch"}, {"name":"unviable"}, {"name":"missed"}, {"name":"timeout"}, {"name":"untested"}]);
    let report = json!({"outcomes":[
        {"scenario":"Baseline", "summary":"Success"},
        {"scenario":{"Mutant":{"name":"caught"}},"summary":"CaughtMutant"},
        {"scenario":{"Mutant":{"name":"unviable"}},"summary":"Unviable"},
        {"scenario":{"Mutant":{"name":"missed"}},"summary":"MissedMutant"},
        {"scenario":{"Mutant":{"name":"timeout"}},"summary":"Timeout"}
    ]});
    let result = mutation::summarize(&inventory, &report).expect("summary");
    for category in ["caught", "unviable", "missed", "timeout", "untested"] {
        assert_eq!(result[category], 1, "{category}");
    }
    assert_eq!(result["status"], "failed_or_incomplete");
    assert_eq!(result["baseline"], "passed");
    let baseline_only = json!({"outcomes":[{"scenario":"Baseline", "summary":"Success"}]});
    assert_eq!(
        mutation::summarize(&json!([]), &baseline_only).expect("empty summary")["status"],
        "failed_or_incomplete"
    );
    let unknown =
        json!({"outcomes":[{"scenario":{"Mutant":{"name":"foreign"}},"summary":"CaughtMutant"}]});
    assert!(mutation::summarize(&inventory, &unknown).is_err());
}

#[cfg(unix)]
#[test]
fn bounded_child_failure_is_not_reported_as_success() {
    let fixture = Fixture::new();
    let mut command = Command::new("sh");
    command.args(["-c", "exit 7"]);
    let result = runner::checked(&fixture.0, &fixture.0, "failure", command, 1);
    assert!(result.is_err());
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.0.join("failure.json")).expect("evidence"))
            .expect("JSON");
    assert_eq!(report["status"], "command_failed");
    assert_eq!(report["exit_code"], 7);
}

#[test]
#[ignore = "subprocess fixture; invoked by scratch_blocks_parent_git_discovery"]
fn git_ceiling_child_fixture() {
    let workspace = Fixture::new();
    fs::create_dir(workspace.0.join("src")).expect("fixture source directory");
    fs::write(
        workspace.0.join("Cargo.toml"),
        "[package]\nname = \"isolated-scratch\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .expect("fixture manifest");
    fs::write(workspace.0.join("src/lib.rs"), "pub const ISOLATED: bool = true;\n")
        .expect("fixture source");
    let status = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&workspace.0)
        .status()
        .expect("inspect ambient repository");
    assert!(!status.success(), "owned scratch inherited an ancestor repository");
    let cargo = Command::new("cargo")
        .args(["metadata", "--offline", "--no-deps", "--format-version", "1"])
        .current_dir(&workspace.0)
        .status()
        .expect("inspect ambient Cargo workspace");
    assert!(cargo.success(), "owned scratch inherited an ancestor Cargo workspace");
}

#[test]
fn scratch_blocks_parent_git_discovery() {
    let repository = Fixture::new();
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&repository.0)
        .status()
        .expect("git init");
    assert!(status.success());
    fs::write(repository.0.join(".gitignore"), "/target/\n").expect("ignore target");
    fs::write(repository.0.join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("parent workspace manifest");
    let evidence = repository.0.join("target/discovery");
    fs::create_dir_all(&evidence).expect("evidence directory");
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command.args([
        "--exact",
        "discovery::tests::git_ceiling_child_fixture",
        "--ignored",
        "--nocapture",
    ]);

    runner::checked(&repository.0, &evidence, "git-ceiling", command, 10)
        .expect("scratch isolation");
}

#[cfg(target_os = "linux")]
#[test]
fn timeout_contains_owned_descendant_and_preserves_sibling() {
    let fixture = Fixture::new();
    let mut sibling = Command::new("sleep").arg("30").spawn().expect("sibling canary");
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 30 & echo $! > reached; wait"]);
    let outcome =
        runner::run(&fixture.0, &fixture.0, "timeout", command, Duration::from_millis(250))
            .expect("bounded run");
    let sibling_alive = sibling.try_wait().expect("sibling census").is_none();
    sibling.kill().expect("terminate owned sibling");
    sibling.wait().expect("reap sibling");
    assert!(outcome.timed_out);
    assert!(sibling_alive);
    let pid = fs::read_to_string(fixture.0.join("reached")).expect("descendant boundary reached");
    let state = fs::read_to_string(format!("/proc/{}/stat", pid.trim()));
    assert!(
        state.is_err() || state.is_ok_and(|line| line.split_whitespace().nth(2) == Some("Z")),
        "owned descendant still executing"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.0.join("timeout.json")).expect("evidence"))
            .expect("JSON");
    assert_eq!(report["status"], "timeout");
}

#[test]
fn engine_must_reach_nonzero_runs_and_finish_its_budget() {
    assert_eq!(super::fuzz_completion("Done 80 runs in 120 second(s)\n"), Some((80, 120)));
    for log in ["", "Done 0 runs in 120 second(s)", "Done 80 runs in 1 second(s)"] {
        assert_eq!(super::fuzz_completion(log), None);
    }
}

#[test]
fn fuzz_command_excludes_ambient_custom_libfuzzer_inputs() {
    let command = super::fuzz_command("sse");
    for name in ["CUSTOM_LIBFUZZER_PATH", "CUSTOM_LIBFUZZER_STD_CXX"] {
        assert!(
            command.get_envs().any(|(candidate, value)| candidate == name && value.is_none()),
            "{name} must be removed from the fuzz build environment",
        );
    }
    assert!(command.get_envs().any(|(name, value)| {
        name == "ASAN_OPTIONS"
            && value.is_some_and(|value| value == std::ffi::OsStr::new("detect_leaks=0"))
    }));
}

#[test]
fn mutation_shards_are_explicit_and_bounded() {
    assert_eq!(
        super::Operation::parse("discovery-posix-lifecycle"),
        Some(super::Operation::PosixLifecycle)
    );
    assert_eq!(
        super::Operation::parse("discovery-mutation-context-canary"),
        Some(super::Operation::ContextCanary)
    );
    for shard in 0..8 {
        assert_eq!(
            super::Operation::parse(&format!("discovery-mutation-receipt-{shard}")),
            Some(super::Operation::Mutation { index: 1, shard: Some(shard) })
        );
    }
    for invalid in
        ["discovery-mutation-receipt-8", "discovery-mutation-receipt-0-1", "discovery-fuzz-sse-0"]
    {
        assert_eq!(super::Operation::parse(invalid), None);
    }
}

#[test]
fn socket_capability_filter_is_scoped_to_cancellation_mutants() {
    assert!(!mutation::needs_lifecycle_test_filter(0));
    assert!(!mutation::needs_lifecycle_test_filter(1));
    assert!(mutation::needs_lifecycle_test_filter(2));
}
