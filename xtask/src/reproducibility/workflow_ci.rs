mod root;
mod yaml;

use super::workflow_commands::{ParsedScript, parse_script};
use crate::error::Diagnostic;
use crate::model::ToolchainPolicy;
use std::path::Path;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

use self::yaml::{exact_keys, integer, mapping_value, string};

const CHECKOUT: &str = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";
const RUST_ACTION: &str = "dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772";
const RUST_REFERENCE: &str = "${{ env.RUST_VERSION }}";
const RUSTUP_MAX_RETRIES: &str = "10";
const PROOF_IMPACT_BASE_REFERENCE: &str =
    "${{ github.event.pull_request.base.sha || github.event.before || inputs.proof_impact_base }}";

pub(super) fn validate(
    workflow: &Hash,
    path: &Path,
    tools: &ToolchainPolicy,
    _policy: super::workflow_command_policy::CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let jobs = mapping_value(workflow, "jobs").and_then(Yaml::as_hash);
    require(
        root::controls_are_exact(workflow),
        path,
        "canonical CI root triggers, permissions, or concurrency differ from the reviewed gate",
        "retain push to main, pull_request, workflow_dispatch, contents:read, and failure-propagating canonical concurrency",
        diagnostics,
    );
    require(
        root::env_is_exact(workflow, tools),
        path,
        "canonical CI environment is not the exact reviewed toolchain-pin set",
        "define only the three toolchain pins and immutable PERITUS_PROOF_IMPACT_BASE event binding",
        diagnostics,
    );
    require(
        jobs.is_some_and(|jobs| {
            exact_keys(
                jobs,
                &[
                    "bootstrap",
                    "policy",
                    "rust-format",
                    "rust",
                    "supply-chain",
                    "verus-policy",
                    "verus",
                ],
            ) && mapping_value(jobs, "bootstrap").is_some_and(bootstrap_job_is_exact)
        }),
        path,
        "canonical CI does not retain the exact pre-Cargo configuration bootstrap",
        "restore the unconditional checkout and authority-pinned .cargo/config.toml/.gitattributes comparison before every Cargo-bearing job",
        diagnostics,
    );
    require(
        jobs.and_then(|jobs| mapping_value(jobs, "policy")).is_some_and(policy_job_is_exact),
        path,
        "canonical CI does not retain the exact workspace policy gate",
        "restore the reviewed locked xtask policy job",
        diagnostics,
    );
    require(
        jobs.and_then(|jobs| mapping_value(jobs, "rust-format")).is_some_and(format_job_is_exact),
        path,
        "canonical CI does not retain the exact Rust format gate",
        "restore the reviewed single-host format job",
        diagnostics,
    );
    require(
        jobs.and_then(|jobs| mapping_value(jobs, "rust")).is_some_and(rust_job_is_exact),
        path,
        "canonical CI does not retain the exact unconditional Rust shard matrix",
        "restore the reviewed Linux, macOS, and Windows package shards",
        diagnostics,
    );
    require(
        jobs.and_then(|jobs| mapping_value(jobs, "supply-chain"))
            .is_some_and(supply_chain_job_is_exact),
        path,
        "canonical CI does not retain the exact unconditional full supply-chain gate",
        "restore the ubuntu-24.04 cargo-deny install and full cargo deny --locked check job",
        diagnostics,
    );
    let verus_policy = jobs
        .and_then(|jobs| mapping_value(jobs, "verus-policy"))
        .is_some_and(|job| verus_policy_job_is_exact(job, tools));
    if !verus_policy {
        for (message, help) in [
            (
                "canonical CI does not install RUST_VERSION with the official pinned Rust action in its required Verus gate",
                "restore the reviewed unconditional ubuntu-24.04 Verus job",
            ),
            (
                "canonical CI does not download and verify the exact pinned Verus archive in its required Verus gate",
                "restore the exact canonical archive download, digest check, extraction, and PATH setup step",
            ),
            (
                "canonical CI does not run the direct locked xtask toolchain check after the verified install",
                "restore cargo run --locked --package xtask -- toolchain-check",
            ),
            (
                "canonical CI does not run the direct ordinary-Rust formal API contract check",
                "restore cargo run --locked --package xtask -- ordinary-api-check before Verus verification",
            ),
        ] {
            diagnostics.push(Diagnostic::at(path, message, help));
        }
    }
    require(
        jobs.and_then(|jobs| mapping_value(jobs, "verus"))
            .is_some_and(|job| verus_job_is_exact(job, tools)),
        path,
        "canonical CI does not retain the exact Verus shard matrix",
        "restore all reviewed Verus verification and release-build package shards",
        diagnostics,
    );
}

fn bootstrap_job_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_keys(job, &["name", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some("Verify pre-Cargo policy")
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(5)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 2 && checkout_step(&steps[0]) && pre_cargo_policy_step(&steps[1])
        })
}

fn pre_cargo_policy_step(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "shell", "run"])
        && string(step, "name") == Some("Verify reviewed pre-Cargo policy")
        && string(step, "shell") == Some("bash")
        && string(step, "run")
            .is_some_and(|script| parse_script(script).is_reviewed_root_config_preflight())
}

fn policy_job_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    common_job(job, "Foundation policy", "ubuntu-24.04", 10)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 3
                && checkout_step(&steps[0])
                && rust_step(&steps[1], None)
                && cargo_step(&steps[2], &["run", "--locked", "--package", "xtask", "--", "all"])
        })
}

fn format_job_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    common_job(job, "Rust format source", "ubuntu-24.04", 10)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 3
                && checkout_step(&steps[0])
                && rust_step(&steps[1], Some("rustfmt"))
                && cargo_step(
                    &steps[2],
                    &["run", "--locked", "--package", "xtask", "--", "format-check"],
                )
        })
}

fn rust_job_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_keys(job, &["name", "needs", "strategy", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name")
            == Some(
                "Foundation Rust ${{ matrix.operation }} ${{ matrix.shard }} (${{ matrix.os }})",
            )
        && string(job, "needs") == Some("bootstrap")
        && string(job, "runs-on") == Some("${{ matrix.os }}")
        && integer(job, "timeout-minutes") == Some(10)
        && rust_matrix(mapping_value(job, "strategy"))
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 3
                && checkout_step(&steps[0])
                && rust_step(&steps[1], Some("clippy,rustfmt"))
                && ci_shard_step(
                    &steps[2],
                    "cargo run --locked --target-dir target/xtask-bootstrap --package xtask -- ci-shard ${{ matrix.operation }} ${{ matrix.shard }}",
                )
        })
}

fn supply_chain_job_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    common_job(job, "Licenses and dependency policy", "ubuntu-24.04", 10)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 4
                && checkout_step(&steps[0])
                && rust_step(&steps[1], None)
                && cargo_step(
                    &steps[2],
                    &["install", "cargo-deny", "--version", "0.20.2", "--locked"],
                )
                && cargo_step(&steps[3], &["deny", "--locked", "check"])
        })
}

fn verus_policy_job_is_exact(job: &Yaml, tools: &ToolchainPolicy) -> bool {
    let Some(job) = job.as_hash() else { return false };
    common_job(job, "Foundation Verus policy", "ubuntu-24.04", 10)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 5
                && checkout_step(&steps[0])
                && rust_step(&steps[1], None)
                && archive_step(&steps[2], tools)
                && cargo_step(
                    &steps[3],
                    &["run", "--locked", "--package", "xtask", "--", "toolchain-check"],
                )
                && cargo_step(
                    &steps[4],
                    &["run", "--locked", "--package", "xtask", "--", "ordinary-api-check"],
                )
        })
}

fn verus_job_is_exact(job: &Yaml, tools: &ToolchainPolicy) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_keys(job, &["name", "needs", "strategy", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name")
            == Some("Foundation Verus ${{ matrix.operation }} ${{ matrix.shard }}")
        && string(job, "needs") == Some("bootstrap")
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(10)
        && verus_matrix(mapping_value(job, "strategy"))
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 4
                && checkout_step(&steps[0])
                && rust_step(&steps[1], None)
                && archive_step(&steps[2], tools)
                && ci_shard_step(
                    &steps[3],
                    "cargo run --locked --package xtask -- ci-shard ${{ matrix.operation }} ${{ matrix.shard }}",
                )
        })
}

fn common_job(job: &Hash, name: &str, runner: &str, timeout_minutes: i64) -> bool {
    exact_keys(job, &["name", "needs", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some(name)
        && string(job, "needs") == Some("bootstrap")
        && string(job, "runs-on") == Some(runner)
        && integer(job, "timeout-minutes") == Some(timeout_minutes)
}

fn rust_matrix(strategy: Option<&Yaml>) -> bool {
    let Some(strategy) = strategy.and_then(Yaml::as_hash) else { return false };
    let Some(matrix) = mapping_value(strategy, "matrix").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_keys(strategy, &["fail-fast", "matrix"])
        && mapping_value(strategy, "fail-fast").and_then(Yaml::as_bool) == Some(false)
        && exact_keys(matrix, &["os", "operation", "shard", "include"])
        && mapping_value(matrix, "os").and_then(Yaml::as_vec).is_some_and(|values| {
            values.iter().filter_map(Yaml::as_str).eq(["ubuntu-24.04", "macos-15", "windows-2025"])
        })
        && string_sequence(
            mapping_value(matrix, "operation"),
            &["build", "test", "doc-test", "clippy", "docs"],
        )
        && string_sequence(mapping_value(matrix, "shard"), &crate::ci_shard::SHARD_NAMES)
        && super::workflow_rust_matrix::has_platform_terminal_includes(mapping_value(
            matrix, "include",
        ))
}

fn verus_matrix(strategy: Option<&Yaml>) -> bool {
    let Some(strategy) = strategy.and_then(Yaml::as_hash) else { return false };
    let Some(matrix) = mapping_value(strategy, "matrix").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_keys(strategy, &["fail-fast", "matrix"])
        && mapping_value(strategy, "fail-fast").and_then(Yaml::as_bool) == Some(false)
        && exact_keys(matrix, &["operation", "shard"])
        && string_sequence(
            mapping_value(matrix, "operation"),
            &["verus-verify", "verus-verify-strict", "verus-build", "verus-build-strict"],
        )
        && string_sequence(
            mapping_value(matrix, "shard"),
            &[
                "foundation-state",
                "runtime-tools",
                "model-orchestration",
                "app-runner",
                "app-shell",
                "edge",
            ],
        )
}

fn string_sequence(value: Option<&Yaml>, expected: &[&str]) -> bool {
    value
        .and_then(Yaml::as_vec)
        .is_some_and(|values| values.iter().filter_map(Yaml::as_str).eq(expected.iter().copied()))
}

fn checkout_step(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(inputs) = mapping_value(step, "with").and_then(Yaml::as_hash) else { return false };
    exact_keys(step, &["name", "uses", "with"])
        && string(step, "uses") == Some(CHECKOUT)
        && exact_keys(inputs, &["fetch-depth"])
        && integer(inputs, "fetch-depth") == Some(0)
}

fn rust_step(step: &Yaml, components: Option<&str>) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(environment) = mapping_value(step, "env").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(inputs) = mapping_value(step, "with").and_then(Yaml::as_hash) else { return false };
    let keys =
        if components.is_some() { &["toolchain", "components"][..] } else { &["toolchain"][..] };
    exact_keys(step, &["name", "uses", "env", "with"])
        && string(step, "uses") == Some(RUST_ACTION)
        && exact_keys(environment, &["RUSTUP_MAX_RETRIES"])
        && string(environment, "RUSTUP_MAX_RETRIES") == Some(RUSTUP_MAX_RETRIES)
        && exact_keys(inputs, keys)
        && string(inputs, "toolchain") == Some(RUST_REFERENCE)
        && components.is_none_or(|expected| string(inputs, "components") == Some(expected))
}

fn cargo_step(step: &Yaml, expected: &[&str]) -> bool {
    step.as_hash().is_some_and(|step| {
        exact_keys(step, &["name", "run"])
            && string(step, "run")
                .map(parse_script)
                .is_some_and(|script: ParsedScript| script.exact_cargo_command(expected))
    })
}

fn ci_shard_step(step: &Yaml, expected: &str) -> bool {
    step.as_hash().is_some_and(|step| {
        exact_keys(step, &["name", "run"])
            && string(step, "name").is_some_and(|name| name.contains("reviewed"))
            && string(step, "run") == Some(expected)
    })
}

fn archive_step(step: &Yaml, tools: &ToolchainPolicy) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "shell", "run"])
        && string(step, "shell") == Some("bash")
        && string(step, "run").is_some_and(|script| archive_script(script, tools))
}

fn archive_script(script: &str, tools: &ToolchainPolicy) -> bool {
    let parsed = parse_script(script);
    if !parsed.has_no_shell_issues() {
        return false;
    }
    let url = tools.archives.linux_x86_64.url.replace(&tools.verus, "$VERUS_VERSION");
    let commands = parsed.commands();
    commands.len() == 9
        && commands[0].is_exact_command(&["set", "-euo", "pipefail"])
        && commands[1].is_exact_words(&["archive=$RUNNER_TEMP/verus.zip"])
        && commands[2].is_exact_words(&["install_root=$RUNNER_TEMP/peritus-verus"])
        && commands[3].is_exact_command(&[
            "curl",
            "--fail",
            "--location",
            "--retry",
            "3",
            "--output",
            "$archive",
            &url,
        ])
        && commands[4].pipes_to_next()
        && commands[4].is_exact_command(&["printf", "%s  %s\\n", "$VERUS_LINUX_SHA256", "$archive"])
        && commands[5].is_exact_command(&["sha256sum", "--check", "--strict"])
        && commands[6].is_exact_command(&["mkdir", "-p", "$install_root"])
        && commands[7].is_exact_command(&["unzip", "-q", "$archive", "-d", "$install_root"])
        && commands[8].is_exact_command(&[
            "printf",
            "%s\\n",
            "$install_root/verus-x86-linux",
            ">>",
            "$GITHUB_PATH",
        ])
}

fn require(valid: bool, path: &Path, message: &str, help: &str, diagnostics: &mut Vec<Diagnostic>) {
    if !valid {
        diagnostics.push(Diagnostic::at(path, message, help));
    }
}
