//! Fixed, bounded discovery operations; production inputs and toolchains stay unchanged.

use crate::error::XtaskError;
use base64::Engine;
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

mod mutation;
mod runner;
#[cfg(test)]
mod tests;

pub(crate) const HARNESS: &str = "crates/app/testing/peritus-bug-discovery";
pub(crate) const TARGETS: [&str; 4] = ["sse", "ndjson", "working_state", "provider_sequence"];
pub(crate) const NIGHTLY: &str = "nightly-2026-08-09";
const FUZZ_VERSION: &str = "cargo-fuzz 0.13.2";
const MUTANTS_VERSION: &str = "cargo-mutants 27.1.0";
const POSIX_LIFECYCLE_IMAGE: &str = "docker.io/library/alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Replay,
    PosixLifecycle,
    SetupFuzz,
    SetupMutation,
    Fuzz(usize),
    ContextCanary,
    Mutation { index: usize, shard: Option<usize> },
}

impl Operation {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        if let Some((base, suffix)) = name.rsplit_once('-')
            && let Ok(shard) = suffix.parse::<usize>()
            && shard < 8
        {
            let index = match base {
                "discovery-mutation-context" => Some(0),
                "discovery-mutation-receipt" => Some(1),
                "discovery-mutation-cancellation" => Some(2),
                _ => None,
            };
            if let Some(index) = index {
                return Some(Self::Mutation { index, shard: Some(shard) });
            }
        }
        match name {
            "discovery-replay" => Some(Self::Replay),
            "discovery-posix-lifecycle" => Some(Self::PosixLifecycle),
            "discovery-setup-fuzz" => Some(Self::SetupFuzz),
            "discovery-setup-mutation" => Some(Self::SetupMutation),
            "discovery-fuzz-sse" => Some(Self::Fuzz(0)),
            "discovery-fuzz-ndjson" => Some(Self::Fuzz(1)),
            "discovery-fuzz-working-state" => Some(Self::Fuzz(2)),
            "discovery-fuzz-provider-sequence" => Some(Self::Fuzz(3)),
            "discovery-mutation-context-canary" => Some(Self::ContextCanary),
            "discovery-mutation-context" => Some(Self::Mutation { index: 0, shard: None }),
            "discovery-mutation-receipt" => Some(Self::Mutation { index: 1, shard: None }),
            "discovery-mutation-cancellation" => Some(Self::Mutation { index: 2, shard: None }),
            _ => None,
        }
    }
}

pub(crate) fn run(root: &Path, operation: Operation) -> Result<(), XtaskError> {
    let evidence = new_evidence(root, operation)?;
    let result = execute(root, operation, &evidence);
    let status = json!({
        "schema_version": 1,
        "status": if result.is_ok() { "completed" } else { "failed_or_incomplete" },
        "error": result.as_ref().err().map(XtaskError::render),
    });
    write_json(&evidence.join("completion.json"), &status)?;
    result
}

fn new_evidence(root: &Path, operation: Operation) -> Result<PathBuf, XtaskError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| XtaskError::metadata(format!("discovery clock: {error}")))?
        .as_nanos();
    let label: String = format!("{operation:?}")
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '-' })
        .collect();
    let evidence = root.join("target/discovery").join(format!("{label}-{timestamp}"));
    fs::create_dir_all(&evidence).map_err(|error| XtaskError::io("create", &evidence, error))?;
    write_json(
        &evidence.join("completion.json"),
        &json!({
            "schema_version": 1, "status": "incomplete", "operation": format!("{operation:?}"),
        }),
    )?;
    let sha = capture(root, "git", &["rev-parse", "HEAD"])?;
    let dirty = capture(root, "git", &["status", "--porcelain", "--untracked-files=normal"])?;
    let rust = capture(root, "rustc", &["--version", "--verbose"])?;
    let source_changes =
        capture(root, "git", &["diff", "--binary", "--no-ext-diff", "--no-textconv", "HEAD"])?;
    fs::write(evidence.join("working-tree.patch"), source_changes)
        .map_err(|error| XtaskError::io("write source patch", &evidence, error))?;
    let untracked = capture(root, "git", &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut hashes = Vec::new();
    for name in untracked.split('\0').filter(|name| !name.is_empty()) {
        use sha2::{Digest, Sha256};
        let path = root.join(name);
        let bytes = fs::read(&path)
            .map_err(|error| XtaskError::io("read untracked source", &path, error))?;
        hashes.push(json!({"path": name, "sha256_base64": base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))}));
    }
    write_json(&evidence.join("untracked-source.json"), &json!(hashes))?;
    write_json(
        &evidence.join("source.json"),
        &json!({
            "schema_version": 1, "source_sha": sha.trim(), "working_tree_changes": dirty,
            "rustc": rust, "os": env::consts::OS, "architecture": env::consts::ARCH,
            "build_jobs": 2, "operation": format!("{operation:?}"),
            "evidence_directory": evidence,
        }),
    )?;
    println!("discovery evidence: {}", evidence.display());
    Ok(evidence)
}

fn execute(root: &Path, operation: Operation, evidence: &Path) -> Result<(), XtaskError> {
    match operation {
        Operation::Replay => {
            validate_corpora(root)?;
            for target in TARGETS {
                let corpus = format!("{HARNESS}/corpus/{target}");
                let mut command = Command::new("cargo");
                command.args([
                    "run",
                    "--locked",
                    "--offline",
                    "--package",
                    "peritus-bug-discovery",
                    "--bin",
                    "discovery-replay",
                    "--",
                    target,
                    &corpus,
                ]);
                runner::checked(root, evidence, target, command, 110)?;
            }
            Ok(())
        }
        Operation::PosixLifecycle => posix_lifecycle(root, evidence),
        Operation::SetupFuzz => {
            let mut nightly = Command::new("rustup");
            nightly.args([
                "toolchain",
                "install",
                NIGHTLY,
                "--profile",
                "minimal",
                "--component",
                "rust-src",
            ]);
            runner::checked(root, evidence, "nightly-install", nightly, 240)?;
            install(root, evidence, "cargo-fuzz", "0.13.2")
        }
        Operation::SetupMutation => install(root, evidence, "cargo-mutants", "27.1.0"),
        Operation::Fuzz(index) => fuzz(root, evidence, index),
        Operation::ContextCanary => mutation::context_canary(root, evidence),
        Operation::Mutation { index, shard } => {
            require_version(root, evidence, "mutants", MUTANTS_VERSION)?;
            mutation::run(root, evidence, index, shard)
        }
    }
}

fn posix_lifecycle(root: &Path, evidence: &Path) -> Result<(), XtaskError> {
    let engine = env::var("PERITUS_CONTAINER_ENGINE").unwrap_or_else(|_| "docker".to_owned());
    if !matches!(engine.as_str(), "docker" | "podman") {
        return Err(XtaskError::invocation(
            "PERITUS_CONTAINER_ENGINE must be exactly docker or podman",
        ));
    }
    let mut pull = Command::new(&engine);
    pull.args(["pull", POSIX_LIFECYCLE_IMAGE]);
    runner::checked(root, evidence, "posix-image", pull, 180)?;

    let mut command = Command::new("python3");
    command
        .arg("packaging/test_posix_lifecycle.py")
        .env("PERITUS_CONTAINER_ENGINE", &engine)
        .env("PERITUS_REQUIRE_POSIX_LIFECYCLE", "1")
        .env("PERITUS_POSIX_LIFECYCLE_IMAGE", POSIX_LIFECYCLE_IMAGE);
    runner::checked(root, evidence, "posix-lifecycle", command, 420)?;
    write_json(
        &evidence.join("posix-lifecycle-summary.json"),
        &json!({
            "container_engine": engine,
            "container_image": POSIX_LIFECYCLE_IMAGE,
            "network_during_scenarios": "none",
            "status": "completed",
        }),
    )
}

fn install(root: &Path, evidence: &Path, tool: &str, version: &str) -> Result<(), XtaskError> {
    let mut command = Command::new("cargo");
    command.args(["install", "--locked", "--version", version, tool]);
    runner::checked(root, evidence, tool, command, 300)
}

fn fuzz(root: &Path, evidence: &Path, index: usize) -> Result<(), XtaskError> {
    validate_corpora(root)?;
    require_version(root, evidence, "fuzz", FUZZ_VERSION)?;
    let nightly = capture(root, "rustc", &[&format!("+{NIGHTLY}"), "--version", "--verbose"])?;
    fs::write(evidence.join("nightly.txt"), nightly)
        .map_err(|error| XtaskError::io("write", evidence, error))?;
    let target = TARGETS.get(index).ok_or_else(|| XtaskError::invocation("unknown fuzz target"))?;
    let lock_path = root.join("Cargo.lock");
    let lock_before =
        fs::read(&lock_path).map_err(|error| XtaskError::io("read lockfile", &lock_path, error))?;
    let _ =
        capture(root, "cargo", &["metadata", "--locked", "--offline", "--format-version", "1"])?;
    let mut command = fuzz_command(target);
    command.env("CARGO_NET_OFFLINE", "true");
    // Discovery may extend only a private corpus copy, never the checked-in regression corpus.
    let corpus = evidence.join("corpus");
    fs::create_dir(&corpus).map_err(|error| XtaskError::io("create", &corpus, error))?;
    for entry in fs::read_dir(root.join(HARNESS).join("corpus").join(target))
        .map_err(|error| XtaskError::io("read corpus", &corpus, error))?
    {
        let entry = entry.map_err(|error| XtaskError::io("read corpus entry", &corpus, error))?;
        fs::copy(entry.path(), corpus.join(entry.file_name()))
            .map_err(|error| XtaskError::io("copy corpus", &corpus, error))?;
    }
    command.arg(&corpus).args([
        "--features",
        "fuzzing",
        "--no-cfg-fuzzing",
        "--fuzz-dir",
        HARNESS,
        "--",
        "-max_total_time=120",
        "-max_len=8192",
        "-rss_limit_mb=2048",
        "-timeout=30",
        "-seed=881",
    ]);
    command.arg(format!("-artifact_prefix={}/", evidence.display()));
    let result = runner::checked(root, evidence, target, command, 480);
    let lock_after =
        fs::read(&lock_path).map_err(|error| XtaskError::io("read lockfile", &lock_path, error))?;
    if lock_before != lock_after {
        return Err(XtaskError::metadata("cargo-fuzz changed Cargo.lock; campaign invalid"));
    }
    result?;
    let log = fs::read_to_string(evidence.join(format!("{target}.stderr")))
        .map_err(|error| XtaskError::io("read fuzz log", evidence, error))?;
    let (runs, seconds) = fuzz_completion(&log).ok_or_else(|| {
        XtaskError::metadata(
            "fuzz engine did not demonstrate nonzero executions and complete allotted budget",
        )
    })?;
    write_json(
        &evidence.join("fuzz-summary.json"),
        &json!({"target": target, "runs": runs, "engine_seconds": seconds, "seed": 881, "corpus": corpus, "max_len": 8192, "rss_limit_mb": 2048, "input_timeout_seconds": 30, "address_sanitizer": "enabled", "leak_detection": "disabled_for_traced_child_compatibility"}),
    )
}

fn fuzz_completion(log: &str) -> Option<(u64, u64)> {
    log.lines().rev().find_map(|line| {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.len() != 6
            || words[0] != "Done"
            || words[2] != "runs"
            || words[3] != "in"
            || words[5] != "second(s)"
        {
            return None;
        }
        let runs = words[1].parse::<u64>().ok()?;
        let seconds = words[4].parse::<u64>().ok()?;
        (runs > 0 && seconds >= 120).then_some((runs, seconds))
    })
}

fn fuzz_command(target: &str) -> Command {
    let mut command = Command::new("cargo");
    command.args([&format!("+{NIGHTLY}"), "fuzz", "run", target]);
    command.env_remove("CUSTOM_LIBFUZZER_PATH");
    command.env_remove("CUSTOM_LIBFUZZER_STD_CXX");
    command.env("ASAN_OPTIONS", "detect_leaks=0");
    command
}

fn require_version(
    root: &Path,
    evidence: &Path,
    tool: &str,
    expected: &str,
) -> Result<(), XtaskError> {
    let version = capture(root, "cargo", &[tool, "--version"])?;
    fs::write(evidence.join(format!("{tool}-version.txt")), &version)
        .map_err(|error| XtaskError::io("write tool version", evidence, error))?;
    if version.trim() != expected {
        return Err(XtaskError::metadata(format!(
            "expected {expected}, observed {}",
            version.trim()
        )));
    }
    Ok(())
}

fn validate_corpora(root: &Path) -> Result<(), XtaskError> {
    let path = root.join(HARNESS).join("Cargo.toml");
    let contents =
        fs::read_to_string(&path).map_err(|error| XtaskError::io("read", &path, error))?;
    let manifest: toml::Value =
        toml::from_str(&contents).map_err(|error| XtaskError::parse_policy(&path, error))?;
    let bins = manifest.get("bin").and_then(toml::Value::as_array).ok_or_else(|| {
        XtaskError::metadata("discovery harness has no explicit binary inventory")
    })?;
    for target in TARGETS.into_iter().chain(["discovery-replay"]) {
        if !bins.iter().any(|bin| bin.get("name").and_then(toml::Value::as_str) == Some(target)) {
            return Err(XtaskError::metadata(format!("discovery target {target} is missing")));
        }
    }
    for target in TARGETS {
        let corpus = root.join(HARNESS).join("corpus").join(target);
        let entries =
            fs::read_dir(&corpus).map_err(|error| XtaskError::io("read corpus", &corpus, error))?;
        let mut count = 0;
        for entry in entries {
            let entry =
                entry.map_err(|error| XtaskError::io("read corpus entry", &corpus, error))?;
            let metadata = entry
                .metadata()
                .map_err(|error| XtaskError::io("inspect corpus", &entry.path(), error))?;
            if entry.file_type().is_ok_and(|kind| kind.is_symlink())
                || !metadata.is_file()
                || metadata.len() > 8192
            {
                return Err(XtaskError::metadata(format!(
                    "corpus must contain regular files <=8192 bytes: {}",
                    entry.path().display()
                )));
            }
            count += 1;
        }
        if count == 0 {
            return Err(XtaskError::metadata(format!("discovery corpus {target} is empty")));
        }
    }
    Ok(())
}

fn capture(root: &Path, program: &str, args: &[&str]) -> Result<String, XtaskError> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| XtaskError::io("probe discovery tool", root, error))?;
    if !output.status.success() {
        return Err(XtaskError::metadata(format!(
            "discovery probe {program} {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| XtaskError::metadata(format!("tool output is not UTF-8: {error}")))
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), XtaskError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(XtaskError::metadata_decode)?;
    fs::write(path, bytes).map_err(|error| XtaskError::io("write discovery evidence", path, error))
}
