use crate::error::{Diagnostic, ErrorCode, XtaskError};
use crate::metadata;
use crate::model::ToolchainPolicy;
use serde::Deserialize;
use std::fs;
use std::path::Path;

mod actionlint_policy;
mod deny_policy;
mod evidence_command;
mod github_ruleset_policy;
mod just_policy;
mod line_endings_policy;
mod policy_file;
#[cfg(test)]
mod reproducibility_ci_control_tests;
#[cfg(test)]
mod reproducibility_command_tests;
#[cfg(test)]
mod reproducibility_config_tests;
mod reproducibility_dependencies;
#[cfg(test)]
mod reproducibility_dependency_tests;
#[cfg(test)]
mod reproducibility_executable_tests;
#[cfg(test)]
mod reproducibility_governance_tests;
#[cfg(test)]
mod reproducibility_just_tests;
#[cfg(test)]
mod reproducibility_manifest_tests;
mod reproducibility_manifests;
#[cfg(test)]
mod reproducibility_workflow_fixture;
#[cfg(test)]
mod reproducibility_workflow_tests;
mod reproducibility_workspace;
#[cfg(test)]
mod reproducibility_workspace_tests;
mod verification_commands;
mod verus_commands;
mod workflow_actionlint;
mod workflow_ci;
mod workflow_command_policy;
mod workflow_command_syntax;
mod workflow_commands;
mod workflow_files;
mod workflow_governance;
mod workflow_governance_jobs;
mod workflow_local;
mod workflow_pins;
mod workflow_policy;
mod workflow_run;

pub(crate) use evidence_command::is_exact_package_gate as is_exact_evidence_command;

const EXPECTED_RUST: &str = "1.97.1";
const EXPECTED_VERUS: &str = "0.2026.08.09.92f466f";
const EXPECTED_VSTD_REVISION: &str = "92f466f247f45128c630d1c843fd6e27d2115587";
const EXPECTED_Z3: &str = "4.16.0";
const EXPECTED_ADVERTISED_Z3: &str = "4.12.5";
const EXPECTED_VERUS_SHA256: &str =
    "2f5a41c553f424aacdd732339e9d125563716a0b003c27730f75d6f81a282cef";

#[derive(Deserialize)]
struct RustToolchain {
    toolchain: RustToolchainSpec,
}

#[derive(Deserialize)]
struct RustToolchainSpec {
    channel: String,
    profile: String,
    components: Vec<String>,
}

pub(crate) fn check(root: &Path, tools: &ToolchainPolicy) -> Result<usize, XtaskError> {
    let mut diagnostics = Vec::new();
    validate_toolchain_policy(tools, &mut diagnostics);
    validate_rust_toolchain(root, &mut diagnostics)?;
    actionlint_policy::validate(root, &mut diagnostics);
    deny_policy::validate(root, &mut diagnostics)?;
    github_ruleset_policy::validate(root, &mut diagnostics);
    line_endings_policy::validate(root, &mut diagnostics);
    reproducibility_workspace::validate(root, &mut diagnostics)?;
    let cargo = metadata::cargo_metadata(root)?;
    let architecture = metadata::architecture_policy(root)?;
    verification_commands::validate(&architecture, &mut diagnostics);
    let architecture = metadata::architecture_policy(root)?;
    diagnostics.extend(crate::architecture::validate_verus_opt_ins(root, &architecture, &cargo));
    reproducibility_manifests::validate(root, &cargo, &mut diagnostics)?;
    reproducibility_dependencies::validate(root, &cargo, &mut diagnostics);
    validate_lockfile(root, &mut diagnostics)?;
    let command_policy = workflow_command_policy::load(root, &mut diagnostics)?;
    let action_count = workflow_policy::validate(root, tools, command_policy, &mut diagnostics)?;
    just_policy::validate(root, command_policy, &mut diagnostics)?;

    if diagnostics.is_empty() {
        Ok(action_count)
    } else {
        Err(XtaskError::violations(
            ErrorCode::Reproducibility,
            "reproducibility-check",
            diagnostics,
        ))
    }
}

fn validate_toolchain_policy(tools: &ToolchainPolicy, diagnostics: &mut Vec<Diagnostic>) {
    let checks = [
        (tools.schema == 1, "toolchain schema must be 1"),
        (tools.rust == EXPECTED_RUST, "Rust pin differs from the reviewed A0 pin"),
        (tools.verus == EXPECTED_VERUS, "Verus pin differs from the reviewed A0 pin"),
        (
            tools.vstd_revision == EXPECTED_VSTD_REVISION,
            "vstd revision differs from the reviewed A0 pin",
        ),
        (
            tools.z3 == EXPECTED_Z3,
            "bundled Z3 pin must match the Verus executable's 4.16.0 requirement",
        ),
        (
            tools.cargo_verus_advertised_z3 == EXPECTED_ADVERTISED_Z3,
            "cargo-verus advertised-Z3 observation has changed; investigate upstream metadata",
        ),
        (
            tools.archives.linux_x86_64.sha256 == EXPECTED_VERUS_SHA256,
            "Verus Linux archive digest differs from the reviewed artifact",
        ),
    ];
    for (valid, message) in checks {
        if !valid {
            diagnostics.push(Diagnostic::at(
                "toolchains.toml",
                message,
                "review the upstream release and update every pin atomically",
            ));
        }
    }
    let expected_url = format!(
        "https://github.com/verus-lang/verus/releases/download/release/{EXPECTED_VERUS}/verus-{EXPECTED_VERUS}-x86-linux.zip"
    );
    if tools.archives.linux_x86_64.url != expected_url {
        diagnostics.push(Diagnostic::at(
            "toolchains.toml",
            "Verus archive URL does not identify the pinned release",
            "use the immutable release URL matching the reviewed version and digest",
        ));
    }
}

fn validate_rust_toolchain(
    root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    if root.join("rust-toolchain").symlink_metadata().is_ok() {
        diagnostics.push(Diagnostic::at(
            "rust-toolchain",
            "legacy rust-toolchain file can shadow the reviewed rust-toolchain.toml selection",
            "remove rust-toolchain; retain only the exact regular rust-toolchain.toml pin and explicit CI RUSTUP_TOOLCHAIN",
        ));
    }
    let relative = Path::new("rust-toolchain.toml");
    let Some(contents) = policy_file::read_regular(
        root,
        relative,
        "Rust toolchain policy is missing, non-regular, or symbolic",
        "Rust toolchain policy",
        "restore the reviewed regular rust-toolchain.toml file",
        diagnostics,
    ) else {
        return Ok(());
    };
    let path = root.join(relative);
    let manifest: RustToolchain =
        toml::from_str(&contents).map_err(|error| XtaskError::parse_policy(&path, error))?;
    if manifest.toolchain.channel != EXPECTED_RUST
        || manifest.toolchain.profile != "minimal"
        || !manifest.toolchain.components.iter().any(|component| component == "clippy")
        || !manifest.toolchain.components.iter().any(|component| component == "rustfmt")
    {
        diagnostics.push(Diagnostic::at(
            "rust-toolchain.toml",
            "Rust toolchain must pin 1.97.1 with minimal profile, Clippy, and rustfmt",
            "restore the reviewed toolchain specification",
        ));
    }
    Ok(())
}

fn validate_lockfile(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Result<(), XtaskError> {
    let path = root.join("Cargo.lock");
    let contents =
        fs::read_to_string(&path).map_err(|error| XtaskError::io("read", &path, error))?;
    if !contents.lines().any(|line| line.trim() == "version = 4") {
        diagnostics.push(Diagnostic::at(
            "Cargo.lock",
            "lockfile is absent or does not use Cargo lock format 4",
            "regenerate it with the pinned Rust 1.97.1 Cargo and review the diff",
        ));
    }
    for line in contents.lines().map(str::trim) {
        if let Some(source) =
            line.strip_prefix("source = \"").and_then(|value| value.strip_suffix('"'))
            && source.starts_with("git+")
            && !has_full_git_revision(source)
        {
            diagnostics.push(Diagnostic::at(
                "Cargo.lock",
                format!("Git lock source lacks a full requested and resolved revision: {source}"),
                "pin the dependency's rev to a 40-character commit and regenerate the lockfile",
            ));
        }
    }
    Ok(())
}

fn has_full_git_revision(source: &str) -> bool {
    let requested = source.split("?rev=").nth(1).and_then(|value| value.split(['&', '#']).next());
    let resolved = source.rsplit_once('#').map(|(_, value)| value);
    requested.is_some_and(is_commit) && resolved.is_none_or(is_commit)
}

fn is_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::{has_full_git_revision, is_commit};

    #[test]
    fn full_git_revision_requires_a_commit_sized_hex_value() {
        let commit = "92f466f247f45128c630d1c843fd6e27d2115587";
        assert!(is_commit(commit));
        assert!(has_full_git_revision(&format!(
            "git+https://example.invalid/repo?rev={commit}#{commit}"
        )));
        assert!(!has_full_git_revision("git+https://example.invalid/repo?branch=main#1234"));
    }
}
