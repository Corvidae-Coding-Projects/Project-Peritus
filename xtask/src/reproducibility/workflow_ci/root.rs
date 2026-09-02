//! Exact root controls for the canonical Foundation workflow.

use crate::model::ToolchainPolicy;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

use super::PROOF_IMPACT_BASE_REFERENCE;
use super::yaml::{exact_keys, mapping_value, string};

pub(super) fn env_is_exact(workflow: &Hash, tools: &ToolchainPolicy) -> bool {
    let Some(env) = mapping_value(workflow, "env").and_then(Yaml::as_hash) else { return false };
    exact_keys(
        env,
        &[
            "CARGO_BUILD_JOBS",
            "RUST_VERSION",
            "VERUS_VERSION",
            "VERUS_LINUX_SHA256",
            "PERITUS_PROOF_IMPACT_BASE",
        ],
    ) && string(env, "CARGO_BUILD_JOBS") == Some("2")
        && string(env, "RUST_VERSION") == Some(&tools.rust)
        && string(env, "VERUS_VERSION") == Some(&tools.verus)
        && string(env, "VERUS_LINUX_SHA256") == Some(&tools.archives.linux_x86_64.sha256)
        && string(env, "PERITUS_PROOF_IMPACT_BASE") == Some(PROOF_IMPACT_BASE_REFERENCE)
}

pub(super) fn controls_are_exact(workflow: &Hash) -> bool {
    let Some(triggers) = mapping_value(workflow, "on").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(push) = mapping_value(triggers, "push").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(permissions) = mapping_value(workflow, "permissions").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(concurrency) = mapping_value(workflow, "concurrency").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_keys(workflow, &["name", "on", "permissions", "concurrency", "env", "jobs"])
        && string(workflow, "name") == Some("Foundation verification")
        && exact_keys(triggers, &["push", "pull_request", "workflow_dispatch"])
        && exact_keys(push, &["branches"])
        && mapping_value(push, "branches")
            .and_then(Yaml::as_vec)
            .is_some_and(|branches| branches.len() == 1 && branches[0].as_str() == Some("main"))
        && mapping_value(triggers, "pull_request") == Some(&Yaml::Null)
        && workflow_dispatch_is_exact(mapping_value(triggers, "workflow_dispatch"))
        && exact_keys(permissions, &["contents"])
        && string(permissions, "contents") == Some("read")
        && exact_keys(concurrency, &["group", "cancel-in-progress"])
        && string(concurrency, "group")
            == Some("foundation-${{ github.workflow }}-${{ github.ref }}")
        && mapping_value(concurrency, "cancel-in-progress").and_then(Yaml::as_bool) == Some(true)
}

fn workflow_dispatch_is_exact(dispatch: Option<&Yaml>) -> bool {
    let Some(dispatch) = dispatch.and_then(Yaml::as_hash) else { return false };
    let Some(inputs) = mapping_value(dispatch, "inputs").and_then(Yaml::as_hash) else {
        return false;
    };
    let Some(base) = mapping_value(inputs, "proof_impact_base").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_keys(dispatch, &["inputs"])
        && exact_keys(inputs, &["proof_impact_base"])
        && exact_keys(base, &["description", "required", "type"])
        && string(base, "description") == Some("Immutable base commit for proof-impact comparison")
        && mapping_value(base, "required").and_then(Yaml::as_bool) == Some(true)
        && string(base, "type") == Some("string")
}
