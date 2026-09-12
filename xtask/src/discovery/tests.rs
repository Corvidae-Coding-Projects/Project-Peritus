use super::{HARNESS, TARGETS, bundle, mutation, runner, validate_corpora};
use serde_json::json;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
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
fn accepted_failure_manifests_are_schema_versioned_and_replayable() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/testing/reproducers")
        .canonicalize()
        .expect("reproducer manifest directory");
    let mut manifests = 0;
    for entry in fs::read_dir(directory).expect("reproducer manifests") {
        let entry = entry.expect("manifest entry");
        if entry.path().extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        manifests += 1;
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(entry.path()).expect("manifest bytes"))
                .expect("valid manifest JSON");
        assert_eq!(manifest["schema_version"], 1, "{}", entry.path().display());
        assert_eq!(manifest["classification"], "product_defect");
        assert!(
            manifest["original"]["fresh_fixture_repetitions"]
                .as_u64()
                .is_some_and(|repetitions| repetitions >= 3),
            "{} needs at least three fresh-fixture reproductions",
            entry.path().display(),
        );
        for pointer in [
            "/id",
            "/invariant",
            "/original/revision",
            "/original/command",
            "/original/failure_signature",
            "/reproducer/path",
            "/reproducer/minimization",
            "/reproducer/oracle",
            "/fix/revision",
            "/fix/root_cause",
            "/fixed_replay/command",
            "/fixed_replay/expected",
            "/negative_control",
            "/cleanup",
        ] {
            assert!(
                manifest
                    .pointer(pointer)
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty()),
                "{} is missing {pointer}",
                entry.path().display(),
            );
        }
    }
    assert_eq!(manifests, 9, "accepted defect inventory changed without a reviewed manifest");
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

#[cfg(unix)]
#[test]
fn child_logs_are_drained_and_retain_bounded_prefixes_and_tails() {
    let fixture = Fixture::new();
    let mut command = Command::new("sh");
    command.args(["-c", "printf 0123456789; printf abcdefghij >&2"]);
    let outcome = runner::run_with_log_limit(
        &fixture.0,
        &fixture.0,
        "bounded-log",
        command,
        Duration::from_secs(1),
        8,
    )
    .expect("bounded command");
    assert!(outcome.status.success());
    assert_eq!(fs::read(fixture.0.join("bounded-log.stdout")).expect("stdout"), b"01236789");
    assert_eq!(fs::read(fixture.0.join("bounded-log.stderr")).expect("stderr"), b"abcdghij");
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.0.join("bounded-log.json")).expect("report"))
            .expect("JSON");
    for stream in ["stdout", "stderr"] {
        assert_eq!(report[stream]["limit_bytes"], 8);
        assert_eq!(report[stream]["observed_bytes"], 10);
        assert_eq!(report[stream]["retained_bytes"], 8);
        assert_eq!(report[stream]["truncated"], true);
        assert_eq!(report[stream]["retention"], "prefix_and_tail");
    }
}

mod evidence;

#[cfg(target_os = "linux")]
#[test]
fn detached_pipe_holder_cannot_block_campaign_completion() {
    struct EscapedProcess(u32);
    impl Drop for EscapedProcess {
        fn drop(&mut self) {
            let _ = Command::new("kill").args(["-KILL", &self.0.to_string()]).status();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while PathBuf::from(format!("/proc/{}", self.0)).exists()
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    let fixture = Fixture::new();
    let mut command = Command::new("sh");
    command.args(["-c", "setsid -f sh -c 'echo $$ > escaped-pid; sleep 1; printf late'"]);
    let started = std::time::Instant::now();
    let result = runner::run_with_limits(
        &fixture.0,
        &fixture.0,
        "detached-pipe",
        command,
        Duration::from_secs(1),
        64,
        Duration::from_millis(200),
    );
    let Err(error) = result else { panic!("escaped pipe must leave the campaign incomplete") };
    let pid_path = fixture.0.join("escaped-pid");
    let pid_deadline = std::time::Instant::now() + Duration::from_secs(1);
    let pid = loop {
        if let Some(pid) =
            fs::read_to_string(&pid_path).ok().and_then(|value| value.trim().parse::<u32>().ok())
        {
            break pid;
        }
        assert!(std::time::Instant::now() < pid_deadline, "escaped process ID was not published");
        std::thread::sleep(Duration::from_millis(10));
    };
    let _escaped = EscapedProcess(pid);

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(error.render().contains("pipe remained open after owned process cleanup"));
    let stdout = fixture.0.join("detached-pipe.stdout");
    let finalized_bytes = fs::read(&stdout).expect("bounded bytes at finalization");
    std::thread::sleep(Duration::from_millis(1_200));
    assert_eq!(
        fs::read(stdout).expect("capture after escaped process exit"),
        finalized_bytes,
        "a cancelled capture cannot mutate already-finalized evidence",
    );
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.0.join("detached-pipe.json")).expect("incomplete report"),
    )
    .expect("JSON");
    assert_eq!(report["status"], "incomplete");
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
    assert_eq!(mutation::test_timeout(0), "60");
    assert_eq!(mutation::test_timeout(1), "60");
    assert_eq!(mutation::test_timeout(2), "120");
}
