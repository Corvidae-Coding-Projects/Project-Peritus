use super::workflow_commands::{ParsedScript, parse_script};
use crate::error::Diagnostic;
use crate::model::ToolchainPolicy;
use std::path::Path;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

const CHECKOUT: &str = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";
const RUST_ACTION: &str = "dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772";
const RUST_REFERENCE: &str = "${{ env.RUST_VERSION }}";
const CONFIG_CHECK_LINE: &str =
    "a3add930639abf20b0b9ddf63453504be5394906ef61a8a38c276d5d9c762f79  .cargo/config.toml\\n";

pub(super) fn validate(
    workflow: &Hash,
    path: &Path,
    tools: &ToolchainPolicy,
    _policy: super::workflow_commands::CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let jobs = mapping_value(workflow, "jobs").and_then(Yaml::as_hash);
    require(
        root_controls_are_exact(workflow),
        path,
        "canonical CI root triggers, permissions, or concurrency differ from the reviewed gate",
        "retain push to main, pull_request, workflow_dispatch, contents:read, and failure-propagating canonical concurrency",
        diagnostics,
    );
    require(
        root_env_is_exact(workflow, tools),
        path,
        "canonical CI environment is not the exact reviewed toolchain-pin set",
        "define only RUST_VERSION, VERUS_VERSION, and VERUS_LINUX_SHA256 from toolchains.toml",
        diagnostics,
    );
    require(
        jobs.is_some_and(|jobs| {
            exact_keys(jobs, &["bootstrap", "rust", "supply-chain", "verus"])
                && mapping_value(jobs, "bootstrap").is_some_and(bootstrap_job_is_exact)
        }),
        path,
        "canonical CI does not retain the exact pre-Cargo configuration bootstrap",
        "restore the unconditional checkout and fixed .cargo/config.toml digest check before every Cargo-bearing job",
        diagnostics,
    );
    require(
        jobs.and_then(|jobs| mapping_value(jobs, "rust")).is_some_and(rust_job_is_exact),
        path,
        "canonical CI does not retain the exact unconditional Rust matrix gate",
        "restore the reviewed Linux, macOS, and Windows fmt/build/test/doc/Clippy/rustdoc/xtask job",
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
    let verus = jobs
        .and_then(|jobs| mapping_value(jobs, "verus"))
        .is_some_and(|job| verus_job_is_exact(job, tools));
    if !verus {
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
                "canonical CI does not run the full locked Verus workspace verification",
                "run exactly cargo verus verify --workspace --locked --check-toolchain",
            ),
            (
                "canonical CI does not run the locked verified release build",
                "run exactly cargo verus build --workspace --release --locked --check-toolchain",
            ),
        ] {
            diagnostics.push(Diagnostic::at(path, message, help));
        }
    }
}

fn bootstrap_job_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_keys(job, &["name", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some("Verify Cargo configuration before Cargo")
        && string(job, "runs-on") == Some("ubuntu-24.04")
        && integer(job, "timeout-minutes") == Some(5)
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 2 && checkout_step(&steps[0]) && config_digest_step(&steps[1])
        })
}

fn config_digest_step(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    exact_keys(step, &["name", "shell", "run"])
        && string(step, "name") == Some("Verify reviewed Cargo configuration digest")
        && string(step, "shell") == Some("bash")
        && string(step, "run").map(parse_script).is_some_and(|script| {
            script.is_reviewed_config_preflight()
                && script.commands()[0].is_exact_command(&["printf", CONFIG_CHECK_LINE])
        })
}

fn rust_job_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    exact_keys(job, &["name", "needs", "strategy", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some("Rust (${{ matrix.os }})")
        && string(job, "needs") == Some("bootstrap")
        && string(job, "runs-on") == Some("${{ matrix.os }}")
        && integer(job, "timeout-minutes") == Some(20)
        && rust_matrix(mapping_value(job, "strategy"))
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 9
                && checkout_step(&steps[0])
                && rust_step(&steps[1], Some("clippy,rustfmt"))
                && cargo_step(&steps[2], &["fmt", "--all", "--", "--check"])
                && cargo_step(
                    &steps[3],
                    &["build", "--workspace", "--all-targets", "--all-features", "--locked"],
                )
                && cargo_step(
                    &steps[4],
                    &["test", "--workspace", "--all-targets", "--all-features", "--locked"],
                )
                && cargo_step(
                    &steps[5],
                    &["test", "--doc", "--workspace", "--all-features", "--locked"],
                )
                && cargo_step(
                    &steps[6],
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
                && docs_step(&steps[7])
                && cargo_step(&steps[8], &["run", "--locked", "--package", "xtask", "--", "all"])
        })
}

fn supply_chain_job_is_exact(job: &Yaml) -> bool {
    let Some(job) = job.as_hash() else { return false };
    common_job(job, "Licenses and dependency policy", "ubuntu-24.04")
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

fn verus_job_is_exact(job: &Yaml, tools: &ToolchainPolicy) -> bool {
    let Some(job) = job.as_hash() else { return false };
    common_job(job, "Locked Verus workspace", "ubuntu-24.04")
        && mapping_value(job, "steps").and_then(Yaml::as_vec).is_some_and(|steps| {
            steps.len() == 6
                && checkout_step(&steps[0])
                && rust_step(&steps[1], None)
                && archive_step(&steps[2], tools)
                && cargo_step(
                    &steps[3],
                    &["run", "--locked", "--package", "xtask", "--", "toolchain-check"],
                )
                && cargo_step(
                    &steps[4],
                    &["verus", "verify", "--workspace", "--locked", "--check-toolchain"],
                )
                && cargo_step(
                    &steps[5],
                    &[
                        "verus",
                        "build",
                        "--workspace",
                        "--release",
                        "--locked",
                        "--check-toolchain",
                    ],
                )
        })
}

fn common_job(job: &Hash, name: &str, runner: &str) -> bool {
    exact_keys(job, &["name", "needs", "runs-on", "timeout-minutes", "steps"])
        && string(job, "name") == Some(name)
        && string(job, "needs") == Some("bootstrap")
        && string(job, "runs-on") == Some(runner)
        && integer(job, "timeout-minutes") == Some(20)
}

fn rust_matrix(strategy: Option<&Yaml>) -> bool {
    let Some(strategy) = strategy.and_then(Yaml::as_hash) else { return false };
    let Some(matrix) = mapping_value(strategy, "matrix").and_then(Yaml::as_hash) else {
        return false;
    };
    exact_keys(strategy, &["fail-fast", "matrix"])
        && mapping_value(strategy, "fail-fast").and_then(Yaml::as_bool) == Some(false)
        && exact_keys(matrix, &["os"])
        && mapping_value(matrix, "os").and_then(Yaml::as_vec).is_some_and(|values| {
            values.iter().filter_map(Yaml::as_str).eq(["ubuntu-24.04", "macos-15", "windows-2025"])
        })
}

fn root_env_is_exact(workflow: &Hash, tools: &ToolchainPolicy) -> bool {
    let Some(env) = mapping_value(workflow, "env").and_then(Yaml::as_hash) else { return false };
    exact_keys(env, &["RUST_VERSION", "VERUS_VERSION", "VERUS_LINUX_SHA256"])
        && string(env, "RUST_VERSION") == Some(&tools.rust)
        && string(env, "VERUS_VERSION") == Some(&tools.verus)
        && string(env, "VERUS_LINUX_SHA256") == Some(&tools.archives.linux_x86_64.sha256)
}

fn root_controls_are_exact(workflow: &Hash) -> bool {
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
        && mapping_value(triggers, "workflow_dispatch") == Some(&Yaml::Null)
        && exact_keys(permissions, &["contents"])
        && string(permissions, "contents") == Some("read")
        && exact_keys(concurrency, &["group", "cancel-in-progress"])
        && string(concurrency, "group")
            == Some("foundation-${{ github.workflow }}-${{ github.ref }}")
        && mapping_value(concurrency, "cancel-in-progress").and_then(Yaml::as_bool) == Some(true)
}

fn checkout_step(step: &Yaml) -> bool {
    step.as_hash().is_some_and(|step| {
        exact_keys(step, &["name", "uses"]) && string(step, "uses") == Some(CHECKOUT)
    })
}

fn rust_step(step: &Yaml, components: Option<&str>) -> bool {
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

fn cargo_step(step: &Yaml, expected: &[&str]) -> bool {
    step.as_hash().is_some_and(|step| {
        exact_keys(step, &["name", "run"])
            && string(step, "run")
                .map(parse_script)
                .is_some_and(|script: ParsedScript| script.exact_cargo_command(expected))
    })
}

fn docs_step(step: &Yaml) -> bool {
    let Some(step) = step.as_hash() else { return false };
    let Some(env) = mapping_value(step, "env").and_then(Yaml::as_hash) else { return false };
    exact_keys(step, &["name", "run", "env"])
        && exact_keys(env, &["RUSTDOCFLAGS"])
        && string(env, "RUSTDOCFLAGS") == Some("-D warnings")
        && cargo_script(
            string(step, "run"),
            &["doc", "--workspace", "--all-features", "--no-deps", "--locked"],
        )
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

fn cargo_script(script: Option<&str>, expected: &[&str]) -> bool {
    script.map(parse_script).is_some_and(|parsed| parsed.exact_cargo_command(expected))
}

fn exact_keys(mapping: &Hash, expected: &[&str]) -> bool {
    mapping.len() == expected.len()
        && mapping.keys().all(|key| key.as_str().is_some_and(|key| expected.contains(&key)))
}

fn string<'a>(mapping: &'a Hash, key: &str) -> Option<&'a str> {
    mapping_value(mapping, key).and_then(Yaml::as_str)
}

fn integer(mapping: &Hash, key: &str) -> Option<i64> {
    mapping_value(mapping, key).and_then(Yaml::as_i64)
}

fn require(valid: bool, path: &Path, message: &str, help: &str, diagnostics: &mut Vec<Diagnostic>) {
    if !valid {
        diagnostics.push(Diagnostic::at(path, message, help));
    }
}

fn mapping_value<'a>(mapping: &'a Hash, key: &str) -> Option<&'a Yaml> {
    mapping.get(&Yaml::String(key.to_owned()))
}
