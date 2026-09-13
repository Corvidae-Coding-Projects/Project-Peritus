//! Exact policy for the default-branch trusted-checker workflow.

use super::workflow_governance::{exact_keys, integer, mapping_value, string};
use crate::error::Diagnostic;
use crate::model::ToolchainPolicy;
use std::path::Path;
use yaml_rust2::yaml::Hash;
use yaml_rust2::{Yaml, YamlLoader};

pub(super) const PATH: &str = ".github/workflows/formal-authority.yml";

const CANONICAL: &str = include_str!("canonical/formal-authority.yml");
const CHECKOUT: &str = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";
const RUST_ACTION: &str = "dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772";
const REPOSITORY: &str = "${{ github.repository }}";
const CHECKER_SHA: &str = "${{ github.workflow_sha }}";
const BASE_SHA: &str = "${{ github.event.pull_request.base.sha }}";
const HEAD_SHA: &str = "${{ github.event.pull_request.head.sha }}";

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
        "trusted-base authority workflow differs from its reviewed definition",
        "restore the canonical file byte-for-byte; authority changes require independent review and a new default-branch checkpoint",
        diagnostics,
    );
    require(
        root_is_exact(workflow, tools),
        path,
        "trusted-base authority workflow weakens revision custody, candidate isolation, authority-input custody, or protected checker execution",
        "restore the exact pull-request-target trigger, read-only permissions, immutable checkouts, pre-metadata execution and authority-input guards, trusted build, and ordered all plus verify-trust checks",
        diagnostics,
    );
}

pub(super) fn reviewed_run_step(location: &str, script: &str) -> bool {
    let index = match location {
        "jobs.trusted-base-validation.steps[3]" => 3,
        "jobs.trusted-base-validation.steps[4]" => 4,
        "jobs.trusted-base-validation.steps[6]" => 6,
        "jobs.trusted-base-validation.steps[7]" => 7,
        "jobs.trusted-base-validation.steps[8]" => 8,
        _ => return false,
    };
    canonical_script(index).as_deref() == Some(script)
}

fn root_is_exact(workflow: &Hash, tools: &ToolchainPolicy) -> bool {
    let Some(triggers) = mapping_value(workflow, "on").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(target) = mapping_value(triggers, "pull_request_target").and_then(Yaml::as_hash)
    else {
        return false;
    };
    let Some(permissions) = mapping_value(workflow, "permissions").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(concurrency) = mapping_value(workflow, "concurrency").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(environment) = mapping_value(workflow, "env").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(jobs) = mapping_value(workflow, "jobs").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(job) = mapping_value(jobs, "trusted-base-validation") else {
        return false;
    };

    exact_keys(workflow, &["name", "on", "permissions", "concurrency", "env", "jobs"])
        && string(workflow, "name") == Some("Formal authority validation")
        && exact_keys(triggers, &["pull_request_target"])
        && exact_target_trigger(target)
        && exact_keys(permissions, &["contents"])
        && string(permissions, "contents") == Some("read")
        && exact_keys(concurrency, &["group", "cancel-in-progress"])
        && string(concurrency, "group")
            == Some(
                "formal-authority-${{ github.event.pull_request.number }}-${{ github.workflow_sha }}-${{ github.event.pull_request.base.sha }}-${{ github.event.pull_request.head.sha }}",
            )
        && mapping_value(concurrency, "cancel-in-progress").and_then(Yaml::as_bool) == Some(false)
        && exact_keys(environment, &["CARGO_BUILD_JOBS", "RUST_VERSION", "RUSTUP_TOOLCHAIN"])
        && string(environment, "CARGO_BUILD_JOBS") == Some("2")
        && string(environment, "RUST_VERSION") == Some(&tools.rust)
        && string(environment, "RUSTUP_TOOLCHAIN") == Some(&tools.rust)
        && exact_keys(jobs, &["trusted-base-validation"])
        && exact_job(job, tools)
}

fn exact_target_trigger(trigger: &Hash) -> bool {
    let Some(branches) = mapping_value(trigger, "branches").and_then(Yaml::as_vec) else {
        return false;
    };
    let Some(types) = mapping_value(trigger, "types").and_then(Yaml::as_vec) else {
        return false;
    };
    exact_keys(trigger, &["branches", "types"])
        && exact_strings(branches, &["main", "develop"])
        && exact_strings(
            types,
            &["opened", "edited", "reopened", "synchronize", "ready_for_review"],
        )
}

fn exact_job(job: &Yaml, tools: &ToolchainPolicy) -> bool {
    let Some(job) = job.as_hash() else { return false };
    let Some(environment) = mapping_value(job, "env").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(steps) = mapping_value(job, "steps").and_then(Yaml::as_vec) else {
        return false;
    };
    exact_keys(job, &["name", "runs-on", "timeout-minutes", "env", "steps"])
        && string(job, "name") == Some("Trusted-base validation")
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(10)
        && exact_keys(
            environment,
            &[
                "CHECKER_SHA",
                "BASE_SHA",
                "CANDIDATE_SHA",
                "EVENT_SHA",
                "EVENT_REF",
                "WORKFLOW_REF",
                "DEFAULT_BRANCH",
                "BASE_REF",
                "BASE_REPOSITORY",
                "BASE_REPOSITORY_ID",
                "REPOSITORY_ID",
                "EVENT_NAME",
            ],
        )
        && string(environment, "CHECKER_SHA") == Some(CHECKER_SHA)
        && string(environment, "BASE_SHA") == Some(BASE_SHA)
        && string(environment, "CANDIDATE_SHA") == Some(HEAD_SHA)
        && string(environment, "EVENT_SHA") == Some("${{ github.sha }}")
        && string(environment, "EVENT_REF") == Some("${{ github.ref }}")
        && string(environment, "WORKFLOW_REF") == Some("${{ github.workflow_ref }}")
        && string(environment, "DEFAULT_BRANCH")
            == Some("${{ github.event.repository.default_branch }}")
        && string(environment, "BASE_REF") == Some("${{ github.event.pull_request.base.ref }}")
        && string(environment, "BASE_REPOSITORY")
            == Some("${{ github.event.pull_request.base.repo.full_name }}")
        && string(environment, "BASE_REPOSITORY_ID")
            == Some("${{ github.event.pull_request.base.repo.id }}")
        && string(environment, "REPOSITORY_ID") == Some("${{ github.repository_id }}")
        && string(environment, "EVENT_NAME") == Some("${{ github.event_name }}")
        && steps.len() == 9
        && exact_checkout(&steps[0], "Check out exact checker revision", CHECKER_SHA, "authority")
        && exact_checkout(&steps[1], "Check out exact comparison base", BASE_SHA, "base")
        && exact_checkout(&steps[2], "Check out exact candidate revision", HEAD_SHA, "candidate")
        && exact_run(&steps[3], 3, "Bind event and checkout identities", None, &[])
        && exact_run(&steps[4], 4, "Reject execution inputs before Cargo metadata", None, &[])
        && exact_rust(&steps[5], &tools.rust)
        && exact_run(
            &steps[6],
            6,
            "Build trusted checker and prime locked metadata",
            None,
            &[("CARGO_HOME", "${{ runner.temp }}/formal-authority-cargo-home")],
        )
        && exact_run(
            &steps[7],
            7,
            "Evaluate candidate with trusted checker",
            Some("candidate"),
            &[
                ("CARGO_HOME", "${{ runner.temp }}/formal-authority-cargo-home"),
                ("CARGO_NET_OFFLINE", "true"),
                ("GIT_CONFIG_GLOBAL", "/dev/null"),
                ("GIT_CONFIG_NOSYSTEM", "1"),
            ],
        )
        && exact_run(
            &steps[8],
            8,
            "Evaluate protected proof-impact trust",
            Some("candidate"),
            &[
                ("CARGO_HOME", "${{ runner.temp }}/formal-authority-cargo-home"),
                ("CARGO_NET_OFFLINE", "true"),
                ("GITHUB_ACTIONS", "true"),
                ("GIT_CONFIG_GLOBAL", "/dev/null"),
                ("GIT_CONFIG_NOSYSTEM", "1"),
                ("PERITUS_PROOF_IMPACT_BASE", BASE_SHA),
            ],
        )
}

fn exact_checkout(step: &Yaml, name: &str, reference: &str, checkout_path: &str) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(inputs) = mapping_value(step, "with").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_keys(step, &["name", "uses", "with"])
        && string(step, "name") == Some(name)
        && string(step, "uses") == Some(CHECKOUT)
        && exact_keys(inputs, &["repository", "ref", "path", "fetch-depth", "persist-credentials"])
        && string(inputs, "repository") == Some(REPOSITORY)
        && string(inputs, "ref") == Some(reference)
        && string(inputs, "path") == Some(checkout_path)
        && integer(inputs, "fetch-depth") == Some(0)
        && mapping_value(inputs, "persist-credentials").and_then(Yaml::as_bool) == Some(false)
}

fn exact_rust(step: &Yaml, rust: &str) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(environment) = mapping_value(step, "env").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(inputs) = mapping_value(step, "with").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_keys(step, &["name", "uses", "env", "with"])
        && string(step, "name") == Some("Install pinned Rust")
        && string(step, "uses") == Some(RUST_ACTION)
        && exact_keys(environment, &["RUSTUP_MAX_RETRIES"])
        && string(environment, "RUSTUP_MAX_RETRIES") == Some("10")
        && exact_keys(inputs, &["toolchain"])
        && string(inputs, "toolchain") == Some("${{ env.RUST_VERSION }}")
        && rust == "1.97.1"
}

fn exact_run(
    step: &Yaml,
    index: usize,
    name: &str,
    working_directory: Option<&str>,
    expected_environment: &[(&str, &str)],
) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let expected_keys = match (working_directory, expected_environment.is_empty()) {
        (None, true) => &["name", "shell", "run"][..],
        (None, false) => &["name", "shell", "env", "run"][..],
        (Some(_), false) => &["name", "working-directory", "shell", "env", "run"][..],
        _ => return false,
    };
    exact_keys(step, expected_keys)
        && string(step, "name") == Some(name)
        && string(step, "shell") == Some("bash")
        && working_directory
            .is_none_or(|expected| string(step, "working-directory") == Some(expected))
        && exact_environment(step, expected_environment)
        && canonical_script(index).as_deref() == string(step, "run")
}

fn exact_environment(step: &Hash, expected: &[(&str, &str)]) -> bool {
    if expected.is_empty() {
        return mapping_value(step, "env").is_none();
    }
    let Some(environment) = mapping_value(step, "env").and_then(Yaml::as_hash) else {
        return false;
    };
    environment.len() == expected.len()
        && expected.iter().all(|(key, value)| string(environment, key) == Some(value))
}

fn canonical_script(index: usize) -> Option<String> {
    let documents = YamlLoader::load_from_str(CANONICAL).ok()?;
    let root = documents.first()?.as_hash()?;
    let jobs = mapping_value(root, "jobs")?.as_hash()?;
    let job = mapping_value(jobs, "trusted-base-validation")?.as_hash()?;
    let steps = mapping_value(job, "steps")?.as_vec()?;
    let step = steps.get(index)?.as_hash()?;
    string(step, "run").map(ToOwned::to_owned)
}

fn exact_strings(values: &[Yaml], expected: &[&str]) -> bool {
    values.len() == expected.len()
        && values.iter().zip(expected).all(|(value, expected)| value.as_str() == Some(*expected))
}

fn require(
    condition: bool,
    path: &Path,
    message: &str,
    help: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !condition {
        diagnostics.push(Diagnostic::at(path, message, help));
    }
}
