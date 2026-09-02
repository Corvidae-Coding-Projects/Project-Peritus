//! Exact bounded-shard topology for the required Gate A workflow.

use super::workflow_commands::parse_script;
use super::workflow_governance::{
    candidate_checkout, config_step, exact_keys, integer, mapping_value, rust_step, string,
};
use crate::model::ToolchainPolicy;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

const PACKAGE_SHARDS: [&str; 6] =
    ["foundation-state", "runtime-tools", "model-orchestration", "app", "testing", "edge"];
const VERUS_SHARDS: [&str; 5] =
    ["foundation-state", "runtime-tools", "model-orchestration", "app", "edge"];
const RUST_OPERATIONS: [&str; 5] = ["build", "test", "doc-test", "clippy", "docs"];
const STATUS_OPERATIONS: [&str; 6] = ["fmt", "build", "test", "doc-test", "clippy", "docs"];
const VERUS_OPERATIONS: [&str; 4] =
    ["verus-verify", "verus-verify-strict", "verus-build", "verus-build-strict"];

pub(super) fn are_exact(jobs: &Hash, tools: &ToolchainPolicy) -> bool {
    mapping_value(jobs, "rust-format").is_some_and(rust_format_is_exact)
        && mapping_value(jobs, "rust-shards").is_some_and(rust_shards_are_exact)
        && mapping_value(jobs, "rust").is_some_and(rust_status_is_exact)
        && mapping_value(jobs, "verus-policy").is_some_and(|job| verus_policy_is_exact(job, tools))
        && mapping_value(jobs, "verus-shards").is_some_and(verus_shards_are_exact)
        && mapping_value(jobs, "verus").is_some_and(verus_status_is_exact)
}

fn rust_format_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_simple_job(job, "Rust format source", "policy", 10)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 4
                && candidate_checkout(&steps[0])
                && config_step(&steps[1])
                && rust_step(&steps[2], Some("rustfmt"))
                && cargo_at_candidate(
                    &steps[3],
                    &["run", "--locked", "--package", "xtask", "--", "format-check"],
                )
        })
}

fn rust_shards_are_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    let Some(strategy) = mapping_value(job, "strategy").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(matrix) = mapping_value(strategy, "matrix").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_matrix_job(
        job,
        "Rust shard ${{ matrix.operation }} ${{ matrix.shard }} (${{ matrix.os }})",
        "${{ matrix.os }}",
        10,
    ) && exact_keys(strategy, &["fail-fast", "matrix"])
        && mapping_value(strategy, "fail-fast").and_then(Yaml::as_bool) == Some(false)
        && exact_keys(matrix, &["os", "operation", "shard"])
        && sequence(matrix, "os", &["ubuntu-24.04", "macos-15", "windows-2025"])
        && sequence(matrix, "operation", &RUST_OPERATIONS)
        && sequence(matrix, "shard", &PACKAGE_SHARDS)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 4
                && candidate_checkout(&steps[0])
                && config_step(&steps[1])
                && rust_step(&steps[2], Some("clippy,rustfmt"))
                && ci_shard_step(
                    &steps[3],
                    "cargo run --locked --target-dir target/xtask-bootstrap --package xtask -- ci-shard ${{ matrix.operation }} ${{ matrix.shard }}",
                )
        })
}

fn rust_status_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    let Some(strategy) = mapping_value(job, "strategy").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(matrix) = mapping_value(strategy, "matrix").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_keys(job, &["name", "if", "needs", "strategy", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some("Rust ${{ matrix.operation }} (${{ matrix.os }})")
        && string(job, "if") == Some("always()")
        && needs(job, &["rust-format", "rust-shards"])
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(5)
        && exact_keys(strategy, &["fail-fast", "matrix"])
        && mapping_value(strategy, "fail-fast").and_then(Yaml::as_bool) == Some(false)
        && exact_keys(matrix, &["os", "operation"])
        && sequence(matrix, "os", &["ubuntu-24.04", "macos-15", "windows-2025"])
        && sequence(matrix, "operation", &STATUS_OPERATIONS)
        && mapping_value(job, "steps")
            .and_then(Yaml::as_vec)
            .is_some_and(|steps| steps.len() == 1 && rust_status_step(&steps[0]))
}

fn verus_policy_is_exact(job: &Yaml, tools: &ToolchainPolicy) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_simple_job(job, "Verus policy", "policy", 10)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 6
                && candidate_checkout(&steps[0])
                && config_step(&steps[1])
                && rust_step(&steps[2], None)
                && archive_step(&steps[3])
                && candidate_xtask(&steps[4], &tools.rust, "toolchain-check")
                && candidate_xtask(&steps[5], &tools.rust, "ordinary-api-check")
        })
}

fn verus_shards_are_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    let Some(strategy) = mapping_value(job, "strategy").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(matrix) = mapping_value(strategy, "matrix").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_matrix_job(
        job,
        "Verus shard ${{ matrix.operation }} ${{ matrix.shard }}",
        "ubuntu-24.04",
        10,
    ) && exact_keys(strategy, &["fail-fast", "matrix"])
        && mapping_value(strategy, "fail-fast").and_then(Yaml::as_bool) == Some(false)
        && exact_keys(matrix, &["operation", "shard"])
        && sequence(matrix, "operation", &VERUS_OPERATIONS)
        && sequence(matrix, "shard", &VERUS_SHARDS)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 5
                && candidate_checkout(&steps[0])
                && config_step(&steps[1])
                && rust_step(&steps[2], None)
                && archive_step(&steps[3])
                && ci_shard_step(
                    &steps[4],
                    "cargo +1.97.1 run --locked --package xtask -- ci-shard ${{ matrix.operation }} ${{ matrix.shard }}",
                )
        })
}

fn verus_status_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_keys(job, &["name", "if", "needs", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some("Verus")
        && string(job, "if") == Some("always()")
        && needs(job, &["verus-policy", "verus-shards"])
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(5)
        && mapping_value(job, "steps")
            .and_then(Yaml::as_vec)
            .is_some_and(|steps| steps.len() == 1 && verus_status_step(&steps[0]))
}

fn exact_simple_job(job: &Hash, name: &str, dependency: &str, timeout: i64) -> bool {
    exact_keys(job, &["name", "needs", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some(name)
        && string(job, "needs") == Some(dependency)
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(timeout)
}

fn exact_matrix_job(job: &Hash, name: &str, runner: &str, timeout: i64) -> bool {
    exact_keys(job, &["name", "needs", "strategy", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some(name)
        && string(job, "needs") == Some("policy")
        && string(job, "runs-on") == Some(runner)
        && integer(job, "timeout-minutes") == Some(timeout)
}

fn sequence(mapping: &Hash, key: &str, expected: &[&str]) -> bool {
    mapping_value(mapping, key)
        .and_then(Yaml::as_vec)
        .is_some_and(|values| values.iter().filter_map(Yaml::as_str).eq(expected.iter().copied()))
}

fn needs(job: &Hash, expected: &[&str]) -> bool {
    mapping_value(job, "needs")
        .and_then(Yaml::as_vec)
        .is_some_and(|values| values.iter().filter_map(Yaml::as_str).eq(expected.iter().copied()))
}

fn cargo_at_candidate(step: &Yaml, expected: &[&str]) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "working-directory", "run"])
        && string(step, "working-directory") == Some("candidate")
        && string(step, "run")
            .map(parse_script)
            .is_some_and(|script| script.exact_cargo_command(expected))
}

fn ci_shard_step(step: &Yaml, command: &str) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "working-directory", "run"])
        && string(step, "working-directory") == Some("candidate")
        && string(step, "run") == Some(command)
}

fn candidate_xtask(step: &Yaml, rust: &str, subcommand: &str) -> bool {
    let toolchain = format!("+{rust}");
    cargo_at_candidate(
        step,
        &[&toolchain, "run", "--locked", "--package", "xtask", "--", subcommand],
    )
}

fn archive_step(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "shell", "run"])
        && string(step, "name") == Some("Install digest-checked Verus archive")
        && string(step, "shell") == Some("bash")
        && string(step, "run")
            .map(parse_script)
            .is_some_and(|script| script.is_reviewed_archive_install())
}

fn rust_status_step(step: &Yaml) -> bool {
    status_step(
        step,
        "Require every Rust shard",
        &[
            ("FORMAT_RESULT", "${{ needs.rust-format.result }}"),
            ("SHARD_RESULT", "${{ needs.rust-shards.result }}"),
        ],
        "test \"$FORMAT_RESULT\" = success\ntest \"$SHARD_RESULT\" = success\n",
    )
}

fn verus_status_step(step: &Yaml) -> bool {
    status_step(
        step,
        "Require every Verus shard",
        &[
            ("POLICY_RESULT", "${{ needs.verus-policy.result }}"),
            ("SHARD_RESULT", "${{ needs.verus-shards.result }}"),
        ],
        "test \"$POLICY_RESULT\" = success\ntest \"$SHARD_RESULT\" = success\n",
    )
}

pub(super) fn status_script_is_exact(location: &str, script: &str) -> bool {
    matches!(
        (location, script),
        (
            "jobs.rust.steps[0]",
            "test \"$FORMAT_RESULT\" = success\ntest \"$SHARD_RESULT\" = success\n"
        ) | (
            "jobs.verus.steps[0]",
            "test \"$POLICY_RESULT\" = success\ntest \"$SHARD_RESULT\" = success\n"
        )
    )
}

fn status_step(step: &Yaml, name: &str, expected_env: &[(&str, &str)], script: &str) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(env) = mapping_value(step, "env").and_then(Yaml::as_hash) else { return false };
    exact_keys(step, &["name", "shell", "env", "run"])
        && string(step, "name") == Some(name)
        && string(step, "shell") == Some("bash")
        && env.len() == expected_env.len()
        && expected_env.iter().all(|(key, value)| string(env, key) == Some(*value))
        && string(step, "run") == Some(script)
}
