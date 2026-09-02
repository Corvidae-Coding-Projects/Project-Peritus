use super::workflow_actionlint;
use super::workflow_commands::{ParsedScript, parse_script};
use super::workflow_governance::{
    candidate_checkout, config_step, exact_keys, integer, mapping_value, rust_step, string,
};
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

const GATE_STATUS_SCRIPT: &str = "test \"$POLICY_RESULT\" = success\n\
test \"$WORKFLOW_LINT_RESULT\" = success\n\
test \"$RUST_RESULT\" = success\n\
test \"$SUPPLY_CHAIN_RESULT\" = success\n\
test \"$VERUS_RESULT\" = success\n";

pub(super) fn are_exact(jobs: &Hash) -> bool {
    mapping_value(jobs, "workflow-lint").is_some_and(workflow_lint_is_exact)
        && mapping_value(jobs, "supply-chain").is_some_and(supply_gate_is_exact)
        && mapping_value(jobs, "gate-a").is_some_and(gate_status_is_exact)
}

fn workflow_lint_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_job(job, "Workflow lint", "policy", 10)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 3
                && candidate_checkout(&steps[0])
                && actionlint_archive(&steps[1])
                && actionlint_step(&steps[2])
        })
}

fn supply_gate_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_job(job, "Supply chain", "policy", 10)
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

fn exact_job(job: &Hash, name: &str, needs: &str, timeout: i64) -> bool {
    exact_keys(job, &["name", "needs", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some(name)
        && string(job, "needs") == Some(needs)
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(timeout)
}

fn cargo_at_candidate(step: &Yaml, expected: &[&str]) -> bool {
    step.as_hash().is_some_and(|step| {
        exact_keys(step, &["name", "working-directory", "run"])
            && string(step, "working-directory") == Some("candidate")
            && cargo_script(string(step, "run"), expected)
    })
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
        && string(step, "run") == Some(GATE_STATUS_SCRIPT)
}

pub(super) fn gate_status_script_is_exact(script: &str) -> bool {
    script == GATE_STATUS_SCRIPT
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
