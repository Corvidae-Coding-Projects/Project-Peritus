use super::workflow_commands::parse_script;
use super::workflow_governance_jobs;
use crate::error::Diagnostic;
use crate::model::ToolchainPolicy;
use std::path::Path;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

pub(super) const PATH: &str = ".github/workflows/formal-governance.yml";

const CANONICAL: &str = include_str!("../../../.github/workflows/formal-governance.yml");
const CHECKOUT: &str = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";
const RUST_ACTION: &str = "dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772";
const RUST_REFERENCE: &str = "${{ env.RUST_VERSION }}";
const CANDIDATE_REPOSITORY: &str = "${{ github.repository }}";
const CANDIDATE_REFERENCE: &str = "${{ github.sha }}";
const PROOF_IMPACT_BASE: &str = "${{ github.event.pull_request.base.sha || github.event.merge_group.base_sha || github.event.before }}";
pub(super) const CANDIDATE_CONFIG_LINE: &str = "a3add930639abf20b0b9ddf63453504be5394906ef61a8a38c276d5d9c762f79  candidate/.cargo/config.toml\\n";

pub(super) fn validate(
    workflow: &Hash,
    contents: &str,
    path: &Path,
    tools: &ToolchainPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    require(
        contents == CANONICAL,
        path,
        "required Gate A workflow differs from its reviewed definition",
        "restore the canonical file byte-for-byte; changes require independent review",
        diagnostics,
    );
    require(
        root_is_exact(workflow, tools),
        path,
        "required Gate A workflow has mutable triggers, permissions, environment, or job topology",
        "retain main push, pull-request, and merge-group triggers; contents:read; exact pins; and all reviewed jobs without cancellation",
        diagnostics,
    );
    let jobs = mapping_value(workflow, "jobs").and_then(Yaml::as_hash);
    require(
        jobs.and_then(|jobs| mapping_value(jobs, "bootstrap")).is_some_and(bootstrap_is_exact),
        path,
        "required Gate A workflow lacks the exact pre-Cargo candidate bootstrap",
        "restore the candidate checkout and reviewed Cargo-config digest check",
        diagnostics,
    );
    require(
        jobs.and_then(|jobs| mapping_value(jobs, "policy"))
            .is_some_and(|job| policy_is_exact(job, tools)),
        path,
        "required Gate A workflow does not evaluate the candidate policy exactly",
        "restore the locked candidate xtask evaluation before the downstream jobs",
        diagnostics,
    );
    require(
        jobs.is_some_and(|jobs| workflow_governance_jobs::are_exact(jobs, tools)),
        path,
        "required Gate A workflow does not retain every hardcoded job and final status",
        "restore the Rust matrix, supply-chain audit, Verus/no-cheating operations, and always-running Gate A aggregator",
        diagnostics,
    );
}

fn root_is_exact(workflow: &Hash, tools: &ToolchainPolicy) -> bool {
    let Some(triggers) = mapping_value(workflow, "on").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(permissions) = mapping_value(workflow, "permissions").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(env) = mapping_value(workflow, "env").and_then(Yaml::as_hash) else { return false };
    let Some(jobs) = mapping_value(workflow, "jobs").and_then(Yaml::as_hash) else { return false };
    exact_keys(workflow, &["name", "on", "permissions", "env", "jobs"])
        && string(workflow, "name") == Some("Gate A")
        && exact_keys(triggers, &["push", "pull_request", "merge_group"])
        && mapping_value(triggers, "push").is_some_and(push_trigger_is_exact)
        && mapping_value(triggers, "pull_request") == Some(&Yaml::Null)
        && mapping_value(triggers, "merge_group") == Some(&Yaml::Null)
        && exact_keys(permissions, &["contents"])
        && string(permissions, "contents") == Some("read")
        && exact_keys(
            env,
            &[
                "RUST_VERSION",
                "RUSTUP_TOOLCHAIN",
                "ACTIONLINT_VERSION",
                "ACTIONLINT_LINUX_SHA256",
                "VERUS_VERSION",
                "VERUS_LINUX_SHA256",
                "PERITUS_PROOF_IMPACT_BASE",
            ],
        )
        && string(env, "RUST_VERSION") == Some(&tools.rust)
        && string(env, "RUSTUP_TOOLCHAIN") == Some(&tools.rust)
        && string(env, "ACTIONLINT_VERSION") == Some("1.7.12")
        && string(env, "ACTIONLINT_LINUX_SHA256")
            == Some("8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8")
        && string(env, "VERUS_VERSION") == Some(&tools.verus)
        && string(env, "VERUS_LINUX_SHA256") == Some(&tools.archives.linux_x86_64.sha256)
        && string(env, "PERITUS_PROOF_IMPACT_BASE") == Some(PROOF_IMPACT_BASE)
        && exact_keys(
            jobs,
            &["bootstrap", "policy", "workflow-lint", "rust", "supply-chain", "verus", "gate-a"],
        )
}

fn push_trigger_is_exact(trigger: &Yaml) -> bool {
    let Some(trigger) = trigger.as_hash() else { return false };
    let Some(branches) = mapping_value(trigger, "branches").and_then(Yaml::as_vec) else {
        return false;
    };
    exact_keys(trigger, &["branches"])
        && branches.len() == 1
        && branches[0].as_str() == Some("main")
}

fn bootstrap_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_keys(job, &["name", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some("Candidate bootstrap")
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(5)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 2
                && candidate_checkout(&steps[0])
                && config_step(&steps[1], CANDIDATE_CONFIG_LINE)
        })
}

fn policy_is_exact(job: &Yaml, tools: &ToolchainPolicy) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_job(job, "Candidate policy", "bootstrap")
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 4
                && candidate_checkout(&steps[0])
                && config_step(&steps[1], CANDIDATE_CONFIG_LINE)
                && rust_step(&steps[2], None)
                && candidate_policy_step(&steps[3], &tools.rust)
        })
}

fn exact_job(job: &Hash, name: &str, needs: &str) -> bool {
    exact_keys(job, &["name", "needs", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some(name)
        && string(job, "needs") == Some(needs)
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(20)
}

pub(super) fn candidate_checkout(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(inputs) = mapping_value(step, "with").and_then(Yaml::as_hash) else { return false };
    exact_keys(step, &["name", "uses", "with"])
        && string(step, "name") == Some("Check out candidate revision")
        && string(step, "uses") == Some(CHECKOUT)
        && exact_keys(inputs, &["repository", "ref", "path", "fetch-depth", "persist-credentials"])
        && string(inputs, "repository") == Some(CANDIDATE_REPOSITORY)
        && string(inputs, "ref") == Some(CANDIDATE_REFERENCE)
        && string(inputs, "path") == Some("candidate")
        && integer(inputs, "fetch-depth") == Some(0)
        && mapping_value(inputs, "persist-credentials").and_then(Yaml::as_bool) == Some(false)
}

pub(super) fn config_step(step: &Yaml, line: &str) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "shell", "run"])
        && string(step, "name") == Some("Verify candidate Cargo configuration before Cargo")
        && string(step, "shell") == Some("bash")
        && string(step, "run").map(parse_script).is_some_and(|script| {
            let commands = script.commands();
            script.has_no_shell_issues()
                && commands.len() == 2
                && commands[0].pipes_to_next()
                && commands[0].is_exact_command(&["printf", line])
                && commands[1].is_exact_command(&["sha256sum", "--check", "--strict"])
        })
}

pub(super) fn rust_step(step: &Yaml, components: Option<&str>) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(inputs) = mapping_value(step, "with").and_then(Yaml::as_hash) else { return false };
    let keys =
        if components.is_some() { &["toolchain", "components"][..] } else { &["toolchain"][..] };
    exact_keys(step, &["name", "uses", "with"])
        && string(step, "uses") == Some(RUST_ACTION)
        && exact_keys(inputs, keys)
        && string(inputs, "toolchain") == Some(RUST_REFERENCE)
        && components.is_none_or(|expected| string(inputs, "components") == Some(expected))
}

fn candidate_policy_step(step: &Yaml, rust: &str) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let toolchain = format!("+{rust}");
    exact_keys(step, &["name", "working-directory", "run"])
        && string(step, "name") == Some("Evaluate candidate policy")
        && string(step, "working-directory") == Some("candidate")
        && string(step, "run").map(parse_script).is_some_and(|script| {
            script.exact_cargo_command(&[
                &toolchain,
                "run",
                "--locked",
                "--package",
                "xtask",
                "--",
                "all",
            ])
        })
}

pub(super) fn exact_keys(mapping: &Hash, expected: &[&str]) -> bool {
    mapping.len() == expected.len()
        && mapping.keys().all(|key| key.as_str().is_some_and(|key| expected.contains(&key)))
}

pub(super) fn string<'a>(mapping: &'a Hash, key: &str) -> Option<&'a str> {
    mapping_value(mapping, key).and_then(Yaml::as_str)
}

pub(super) fn integer(mapping: &Hash, key: &str) -> Option<i64> {
    mapping_value(mapping, key).and_then(Yaml::as_i64)
}

fn require(valid: bool, path: &Path, message: &str, help: &str, diagnostics: &mut Vec<Diagnostic>) {
    if !valid {
        diagnostics.push(Diagnostic::at(path, message, help));
    }
}

pub(super) fn mapping_value<'a>(mapping: &'a Hash, key: &str) -> Option<&'a Yaml> {
    mapping.get(&Yaml::String(key.to_owned()))
}
