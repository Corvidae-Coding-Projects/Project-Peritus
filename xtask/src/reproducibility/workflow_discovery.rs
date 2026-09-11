//! Discovery has fixed inventories, hermetic PR replay, and no successful skipped campaign.

use crate::error::Diagnostic;
use std::path::Path;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

pub(super) const PATH: &str = ".github/workflows/bug-discovery.yml";
const PREFLIGHT: &str = "git diff --no-ext-diff --no-textconv --exit-code 6ca5f56d2ab12e93f155d684b33f4a86c2f877b8 -- .cargo/config.toml .gitattributes";
const SCHEDULE_ONLY: &str = "${{ github.event_name != 'pull_request' }}";

pub(super) fn validate(workflow: &Hash, diagnostics: &mut Vec<Diagnostic>) {
    let jobs = get(workflow, "jobs");
    let root = get(workflow, "on");
    let controls = root["pull_request"].is_null()
        && root["workflow_dispatch"].is_null()
        && root["schedule"][0]["cron"].as_str() == Some("17 3 * * 1")
        && keys(root.as_hash(), &["pull_request", "workflow_dispatch", "schedule"])
        && get(workflow, "permissions")["contents"].as_str() == Some("read")
        && keys(get(workflow, "permissions").as_hash(), &["contents"])
        && get(workflow, "env")["CARGO_BUILD_JOBS"].as_str() == Some("2")
        && get(workflow, "env")["RUST_VERSION"].as_str() == Some("1.97.1")
        && keys(get(workflow, "env").as_hash(), &["CARGO_BUILD_JOBS", "RUST_VERSION"])
        && get(workflow, "concurrency")["cancel-in-progress"].as_bool() == Some(true)
        && get(workflow, "concurrency")["group"].as_str()
            == Some("discovery-${{ github.workflow }}-${{ github.ref }}")
        && keys(jobs.as_hash(), &["replay", "context-canary", "fuzz", "mutation"]);
    if !controls {
        violation("discovery root controls or complete job inventory changed", diagnostics);
    }
    for name in ["replay", "context-canary", "fuzz", "mutation"] {
        if !valid_job(&jobs[name], name) {
            violation(
                &format!(
                    "discovery {name} must retain exact bounded commands, target inventory, conditions and evidence upload"
                ),
                diagnostics,
            );
        }
    }
}

fn valid_job(job: &Yaml, name: &str) -> bool {
    let key_names: &[&str] = match name {
        "replay" => &["runs-on", "timeout-minutes", "steps"],
        "context-canary" => &["if", "runs-on", "timeout-minutes", "steps"],
        _ => &["if", "runs-on", "timeout-minutes", "strategy", "steps"],
    };
    if !keys(job.as_hash(), key_names)
        || job["runs-on"].as_str() != Some("ubuntu-24.04")
        || job["timeout-minutes"].as_i64() != Some(10)
    {
        return false;
    }
    let commands = match name {
        "fuzz" => vec![
            PREFLIGHT,
            "cargo fetch --locked",
            "cargo xtask discovery-setup-fuzz",
            "cargo xtask discovery-fuzz-${{ matrix.target }}",
        ],
        "mutation" => vec![
            PREFLIGHT,
            "cargo fetch --locked",
            "cargo xtask discovery-setup-mutation",
            "cargo xtask discovery-mutation-${{ matrix.slice }}-${{ matrix.shard }}",
        ],
        "context-canary" => {
            vec![PREFLIGHT, "cargo fetch --locked", "cargo xtask discovery-mutation-context-canary"]
        }
        _ => vec![
            PREFLIGHT,
            "cargo fetch --locked",
            "cargo xtask discovery-posix-lifecycle",
            "cargo xtask discovery-replay",
        ],
    };
    if matches!(name, "fuzz" | "mutation") {
        let (dimension, expected) = if name == "fuzz" {
            ("target", &["sse", "ndjson", "working-state", "provider-sequence"][..])
        } else {
            ("slice", &["receipt", "cancellation"][..])
        };
        let actual = job["strategy"]["matrix"][dimension]
            .as_vec()
            .map(|values| values.iter().filter_map(Yaml::as_str).collect::<Vec<_>>());
        if job["if"].as_str() != Some(SCHEDULE_ONLY)
            || job["strategy"]["fail-fast"].as_bool() != Some(false)
            || actual.as_deref() != Some(expected)
            || !valid_strategy(job, name, dimension)
        {
            return false;
        }
    } else if name == "context-canary" && job["if"].as_str() != Some(SCHEDULE_ONLY) {
        return false;
    }
    let Some(steps) = job["steps"].as_vec() else { return false };
    if steps.len() != commands.len() + 3 {
        return false;
    }
    let runs: Vec<_> = steps.iter().filter_map(|step| step["run"].as_str()).collect();
    if runs != commands {
        return false;
    }
    for step in steps {
        let Some(map) = step.as_hash() else { return false };
        if step["run"].as_str().is_some()
            && !map.keys().all(|key| matches!(key.as_str(), Some("run" | "name")))
        {
            return false;
        }
    }
    valid_checkout(&steps[0])
        && steps[1]["run"].as_str() == Some(PREFLIGHT)
        && valid_toolchain(&steps[2])
        && valid_upload(&steps[steps.len() - 1], name)
}

fn valid_strategy(job: &Yaml, name: &str, dimension: &str) -> bool {
    if name == "fuzz" {
        return keys(job["strategy"].as_hash(), &["fail-fast", "matrix"])
            && keys(job["strategy"]["matrix"].as_hash(), &[dimension]);
    }
    let shards = job["strategy"]["matrix"]["shard"]
        .as_vec()
        .map(|items| items.iter().filter_map(Yaml::as_i64).collect::<Vec<_>>());
    keys(job["strategy"].as_hash(), &["fail-fast", "max-parallel", "matrix"])
        && keys(job["strategy"]["matrix"].as_hash(), &[dimension, "shard"])
        && job["strategy"]["max-parallel"].as_i64() == Some(2)
        && shards == Some((0..8).collect())
}

fn valid_checkout(step: &Yaml) -> bool {
    keys(step.as_hash(), &["uses", "with"])
        && step["uses"].as_str()
            == Some("actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1")
        && keys(step["with"].as_hash(), &["fetch-depth"])
        && step["with"]["fetch-depth"].as_i64() == Some(0)
}

fn valid_toolchain(step: &Yaml) -> bool {
    keys(step.as_hash(), &["uses", "with"])
        && step["uses"].as_str()
            == Some("dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772")
        && keys(step["with"].as_hash(), &["toolchain"])
        && step["with"]["toolchain"].as_str() == Some("${{ env.RUST_VERSION }}")
}

fn valid_upload(step: &Yaml, name: &str) -> bool {
    let artifact = match name {
        "fuzz" => "discovery-fuzz-${{ matrix.target }}",
        "mutation" => "discovery-mutation-${{ matrix.slice }}-${{ matrix.shard }}",
        "context-canary" => "discovery-context-canary",
        _ => "discovery-replay",
    };
    keys(step.as_hash(), &["name", "if", "uses", "with"])
        && step["uses"].as_str()
            == Some("actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a")
        && step["if"].as_str() == Some("${{ always() }}")
        && keys(step["with"].as_hash(), &["name", "path", "if-no-files-found", "retention-days"])
        && step["with"]["name"].as_str() == Some(artifact)
        && step["with"]["path"].as_str() == Some("target/discovery/**")
        && step["with"]["if-no-files-found"].as_str() == Some("error")
        && step["with"]["retention-days"].as_i64() == Some(14)
}

fn keys(mapping: Option<&Hash>, expected: &[&str]) -> bool {
    mapping.is_some_and(|mapping| {
        mapping.len() == expected.len()
            && expected.iter().all(|key| mapping.contains_key(&Yaml::String((*key).to_owned())))
    })
}

fn get<'a>(mapping: &'a Hash, key: &str) -> &'a Yaml {
    mapping.get(&Yaml::String(key.to_owned())).unwrap_or(&Yaml::BadValue)
}

fn violation(message: &str, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.push(Diagnostic::at(Path::new(PATH), message,
        "restore the reviewed discovery workflow; missing targets, disabled replay, masked errors and incomplete evidence cannot pass"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_rust2::YamlLoader;

    fn check(text: &str) -> Vec<Diagnostic> {
        let document = YamlLoader::load_from_str(text).expect("fixture YAML");
        let mut diagnostics = Vec::new();
        validate(document[0].as_hash().expect("mapping"), &mut diagnostics);
        diagnostics
    }

    #[test]
    fn checked_workflow_has_exact_bounded_inventory() {
        assert!(check(include_str!("../../../.github/workflows/bug-discovery.yml")).is_empty());
    }

    #[test]
    fn incomplete_or_weakened_workflow_is_rejected() {
        let source = include_str!("../../../.github/workflows/bug-discovery.yml");
        for (before, after) in [
            ("[sse, ndjson, working-state, provider-sequence]", "[sse, ndjson]"),
            ("timeout-minutes: 10", "timeout-minutes: 20"),
            ("cargo xtask discovery-replay", "cargo xtask help"),
            ("if-no-files-found: error", "if-no-files-found: warn"),
            ("${{ always() }}", "${{ success() }}"),
            ("github.event_name != 'pull_request'", "github.event_name == 'pull_request'"),
            ("[receipt, cancellation]", "[receipt]"),
            ("[0, 1, 2, 3, 4, 5, 6, 7]", "[0, 1, 2]"),
            ("cargo fetch --locked", "cargo fetch --locked\n        continue-on-error: true"),
            (
                "cargo xtask discovery-mutation-context-canary",
                "cargo xtask discovery-mutation-context",
            ),
            ("cargo xtask discovery-posix-lifecycle", "cargo xtask discovery-replay"),
        ] {
            assert!(!check(&source.replace(before, after)).is_empty(), "accepted {after}");
        }
    }
}
