use super::verus_commands::{
    VERUS_STRICT_BUILD_ARGS, VERUS_STRICT_VERIFY_ARGS, VERUS_WORKSPACE_BUILD_ARGS,
    VERUS_WORKSPACE_VERIFY_ARGS,
};
use super::workflow_actionlint;
use super::workflow_command_contracts::WORKSPACE_TEST_ARGS;
use super::workflow_commands::{ParsedScript, parse_script};
use super::workflow_governance::{
    candidate_checkout, config_step, exact_keys, integer, mapping_value, rust_step, string,
};
use crate::model::ToolchainPolicy;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

const GATE_STATUS_SCRIPT: &str = "test \"$POLICY_RESULT\" = success\n\
test \"$WORKFLOW_LINT_RESULT\" = success\n\
test \"$RUST_RESULT\" = success\n\
test \"$SUPPLY_CHAIN_RESULT\" = success\n\
test \"$VERUS_RESULT\" = success\n";

pub(super) fn are_exact(jobs: &Hash, tools: &ToolchainPolicy) -> bool {
    mapping_value(jobs, "workflow-lint").is_some_and(workflow_lint_is_exact)
        && mapping_value(jobs, "rust").is_some_and(rust_gate_is_exact)
        && mapping_value(jobs, "supply-chain").is_some_and(supply_gate_is_exact)
        && mapping_value(jobs, "verus").is_some_and(|job| verus_gate_is_exact(job, tools))
        && mapping_value(jobs, "gate-a").is_some_and(gate_status_is_exact)
}

fn workflow_lint_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_keys(job, &["name", "needs", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some("Workflow lint")
        && string(job, "needs") == Some("policy")
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(10)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 3
                && candidate_checkout(&steps[0])
                && actionlint_archive(&steps[1])
                && actionlint_step(&steps[2])
        })
}

fn rust_gate_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    let Some(strategy) = mapping_value(job, "strategy").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(matrix) = mapping_value(strategy, "matrix").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_keys(job, &["name", "needs", "strategy", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some("Rust ${{ matrix.operation }} (${{ matrix.os }})")
        && string(job, "needs") == Some("policy")
        && string(job, "runs-on") == Some("${{ matrix.os }}")
        && integer(job, "timeout-minutes") == Some(45)
        && exact_keys(strategy, &["fail-fast", "matrix"])
        && mapping_value(strategy, "fail-fast").and_then(Yaml::as_bool) == Some(false)
        && exact_matrix(matrix)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 9
                && candidate_checkout(&steps[0])
                && config_step(&steps[1])
                && rust_step(&steps[2], Some("clippy,rustfmt"))
                && conditional_cargo(
                    &steps[3],
                    "matrix.operation == 'fmt'",
                    &["run", "--locked", "--package", "xtask", "--", "format-check"],
                )
                && conditional_cargo(
                    &steps[4],
                    "matrix.operation == 'build'",
                    &["build", "--workspace", "--all-targets", "--all-features", "--locked"],
                )
                && conditional_cargo(&steps[5], "matrix.operation == 'test'", WORKSPACE_TEST_ARGS)
                && conditional_cargo(
                    &steps[6],
                    "matrix.operation == 'doc-test'",
                    &["test", "--doc", "--workspace", "--all-features", "--locked"],
                )
                && conditional_cargo(
                    &steps[7],
                    "matrix.operation == 'clippy'",
                    &[
                        "clippy",
                        "--workspace",
                        "--all-targets",
                        "--all-features",
                        "--locked",
                        "--",
                        "-D",
                        "warnings",
                    ],
                )
                && docs_step(&steps[8])
        })
}

fn exact_matrix(matrix: &Hash) -> bool {
    exact_keys(matrix, &["os", "operation"])
        && mapping_value(matrix, "os").and_then(Yaml::as_vec).is_some_and(|oses| {
            oses.iter().filter_map(Yaml::as_str).eq(["ubuntu-24.04", "macos-15", "windows-2025"])
        })
        && mapping_value(matrix, "operation").and_then(Yaml::as_vec).is_some_and(|operations| {
            operations
                .iter()
                .filter_map(Yaml::as_str)
                .eq(["fmt", "build", "test", "doc-test", "clippy", "docs"])
        })
}

fn supply_gate_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_job(job, "Supply chain", 20)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 5
                && candidate_checkout(&steps[0])
                && config_step(&steps[1])
                && rust_step(&steps[2], None)
                && cargo_at_candidate(
                    &steps[3],
                    &["install", "cargo-deny", "--version", "0.20.2", "--locked"],
                )
                && cargo_at_candidate(&steps[4], &["deny", "--locked", "check"])
        })
}

fn verus_gate_is_exact(job: &Yaml, tools: &ToolchainPolicy) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_job(job, "Verus", 40)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 10
                && candidate_checkout(&steps[0])
                && config_step(&steps[1])
                && rust_step(&steps[2], None)
                && archive_step(&steps[3])
                && candidate_xtask(&steps[4], &tools.rust, "toolchain-check")
                && candidate_xtask(&steps[5], &tools.rust, "ordinary-api-check")
                && cargo_at_candidate(&steps[6], VERUS_WORKSPACE_VERIFY_ARGS)
                && cargo_at_candidate(&steps[7], VERUS_STRICT_VERIFY_ARGS)
                && cargo_at_candidate(&steps[8], VERUS_WORKSPACE_BUILD_ARGS)
                && cargo_at_candidate(&steps[9], VERUS_STRICT_BUILD_ARGS)
        })
}

fn exact_job(job: &Hash, name: &str, timeout_minutes: i64) -> bool {
    exact_keys(job, &["name", "needs", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some(name)
        && string(job, "needs") == Some("policy")
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(timeout_minutes)
}

fn conditional_cargo(step: &Yaml, condition: &str, expected: &[&str]) -> bool {
    step.as_hash().is_some_and(|step| {
        exact_keys(step, &["name", "if", "working-directory", "run"])
            && string(step, "if") == Some(condition)
            && string(step, "working-directory") == Some("candidate")
            && cargo_script(string(step, "run"), expected)
    })
}

fn cargo_at_candidate(step: &Yaml, expected: &[&str]) -> bool {
    step.as_hash().is_some_and(|step| {
        exact_keys(step, &["name", "working-directory", "run"])
            && string(step, "working-directory") == Some("candidate")
            && cargo_script(string(step, "run"), expected)
    })
}

fn candidate_xtask(step: &Yaml, rust: &str, subcommand: &str) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let toolchain = format!("+{rust}");
    exact_keys(step, &["name", "working-directory", "run"])
        && string(step, "working-directory") == Some("candidate")
        && cargo_script(
            string(step, "run"),
            &[&toolchain, "run", "--locked", "--package", "xtask", "--", subcommand],
        )
}

fn gate_status_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    let Some(needs) = mapping_value(job, "needs").and_then(Yaml::as_vec) else { return false };
    exact_keys(job, &["name", "if", "needs", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some("Gate A")
        && string(job, "if") == Some("always()")
        && needs.iter().filter_map(Yaml::as_str).eq([
            "policy",
            "workflow-lint",
            "rust",
            "supply-chain",
            "verus",
        ])
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(5)
        && mapping_value(job, "steps")
            .and_then(Yaml::as_vec)
            .is_some_and(|steps| steps.len() == 1 && gate_step_is_exact(&steps[0]))
}

fn gate_step_is_exact(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(env) = mapping_value(step, "env").and_then(Yaml::as_hash) else { return false };
    exact_keys(step, &["name", "shell", "env", "run"])
        && string(step, "name") == Some("Require every Gate A job")
        && string(step, "shell") == Some("bash")
        && exact_keys(
            env,
            &[
                "POLICY_RESULT",
                "WORKFLOW_LINT_RESULT",
                "RUST_RESULT",
                "SUPPLY_CHAIN_RESULT",
                "VERUS_RESULT",
            ],
        )
        && string(env, "POLICY_RESULT") == Some("${{ needs.policy.result }}")
        && string(env, "WORKFLOW_LINT_RESULT") == Some("${{ needs.workflow-lint.result }}")
        && string(env, "RUST_RESULT") == Some("${{ needs.rust.result }}")
        && string(env, "SUPPLY_CHAIN_RESULT") == Some("${{ needs.supply-chain.result }}")
        && string(env, "VERUS_RESULT") == Some("${{ needs.verus.result }}")
        && string(step, "run").is_some_and(gate_status_script_is_exact)
}

pub(super) fn gate_status_script_is_exact(script: &str) -> bool {
    script == GATE_STATUS_SCRIPT
}

fn docs_step(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(env) = mapping_value(step, "env").and_then(Yaml::as_hash) else { return false };
    exact_keys(step, &["name", "if", "working-directory", "run", "env"])
        && string(step, "if") == Some("matrix.operation == 'docs'")
        && string(step, "working-directory") == Some("candidate")
        && exact_keys(env, &["RUSTDOCFLAGS"])
        && string(env, "RUSTDOCFLAGS") == Some("-D warnings")
        && cargo_script(
            string(step, "run"),
            &["doc", "--workspace", "--all-features", "--no-deps", "--locked"],
        )
}

fn archive_step(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "shell", "run"])
        && string(step, "shell") == Some("bash")
        && string(step, "run")
            .map(parse_script)
            .is_some_and(|script| script.is_reviewed_archive_install())
}

fn actionlint_archive(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "shell", "run"])
        && string(step, "name") == Some("Install digest-checked actionlint archive")
        && string(step, "shell") == Some("bash")
        && string(step, "run")
            .map(parse_script)
            .is_some_and(|script| workflow_actionlint::is_reviewed_install(&script))
}

fn actionlint_step(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "working-directory", "run"])
        && string(step, "name") == Some("Lint every workflow")
        && string(step, "working-directory") == Some("candidate")
        && string(step, "run").map(parse_script).is_some_and(|script| {
            script.is_failure_propagating()
                && script.commands().len() == 1
                && script.commands()[0].is_exact_command(&[
                    "actionlint",
                    "-config-file",
                    ".github/actionlint.yaml",
                ])
        })
}

fn cargo_script(script: Option<&str>, expected: &[&str]) -> bool {
    script
        .map(parse_script)
        .is_some_and(|script: ParsedScript| script.exact_cargo_command(expected))
}
