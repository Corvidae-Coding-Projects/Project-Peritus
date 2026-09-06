//! Reviewed package shards for bounded hosted Rust and Verus jobs.

mod runner;

use crate::error::XtaskError;
use crate::metadata;
use crate::model::{ArchitecturePolicy, CargoMetadata, CargoPackage};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

const PLATFORM_PACKAGE: &str = "peritus-platform-qualification";
const PLATFORM_TERMINAL_INTERACTIVE: &str =
    "terminal_interactive_round_trip_is_observed_and_reaped";
const PLATFORM_TERMINAL_SIGNAL: &str = "terminal_control_signal_is_observed_and_reaped";
const PLATFORM_TERMINAL_CANCEL: &str = "terminal_cancellation_is_observed_and_reaped";

pub(crate) const SHARD_NAMES: [&str; 9] = [
    "foundation-state",
    "runtime-tools",
    "model-orchestration",
    "app-runner",
    "app-shell",
    "testing",
    "testing-platform",
    "testing-external",
    "edge",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Build,
    Test,
    DocTest,
    Clippy,
    Docs,
    TestPlatformTerminalInteractive,
    TestPlatformTerminalSignal,
    TestPlatformTerminalCancel,
    TestRunnerRecovery,
    TestRunnerProduct,
    VerusVerify,
    VerusVerifyStrict,
    VerusBuild,
    VerusBuildStrict,
}

impl Operation {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "build" => Some(Self::Build),
            "test" => Some(Self::Test),
            "doc-test" => Some(Self::DocTest),
            "clippy" => Some(Self::Clippy),
            "docs" => Some(Self::Docs),
            "test-platform-terminal-interactive" => Some(Self::TestPlatformTerminalInteractive),
            "test-platform-terminal-signal" => Some(Self::TestPlatformTerminalSignal),
            "test-platform-terminal-cancel" => Some(Self::TestPlatformTerminalCancel),
            "test-runner-recovery" => Some(Self::TestRunnerRecovery),
            "test-runner-product" => Some(Self::TestRunnerProduct),
            "verus-verify" => Some(Self::VerusVerify),
            "verus-verify-strict" => Some(Self::VerusVerifyStrict),
            "verus-build" => Some(Self::VerusBuild),
            "verus-build-strict" => Some(Self::VerusBuildStrict),
            _ => None,
        }
    }

    const fn is_verus(self) -> bool {
        matches!(
            self,
            Self::VerusVerify | Self::VerusVerifyStrict | Self::VerusBuild | Self::VerusBuildStrict
        )
    }

    const fn is_strict(self) -> bool {
        matches!(self, Self::VerusVerifyStrict | Self::VerusBuildStrict)
    }

    const fn is_platform_terminal(self) -> bool {
        self.platform_terminal_test().is_some()
    }

    const fn runner_tests(self) -> Option<&'static [&'static str]> {
        match self {
            Self::TestRunnerRecovery => Some(runner::RECOVERY_TESTS),
            Self::TestRunnerProduct => Some(runner::PRODUCT_TESTS),
            _ => None,
        }
    }

    const fn platform_terminal_test(self) -> Option<&'static str> {
        match self {
            Self::TestPlatformTerminalInteractive => Some(PLATFORM_TERMINAL_INTERACTIVE),
            Self::TestPlatformTerminalSignal => Some(PLATFORM_TERMINAL_SIGNAL),
            Self::TestPlatformTerminalCancel => Some(PLATFORM_TERMINAL_CANCEL),
            _ => None,
        }
    }
}

pub(crate) fn run(root: &Path, operation: Operation, shard: &str) -> Result<usize, XtaskError> {
    if !SHARD_NAMES.contains(&shard) {
        return Err(XtaskError::invocation(format!(
            "unknown CI shard `{shard}`; expected one of {}",
            SHARD_NAMES.join(", ")
        )));
    }
    let policy = metadata::architecture_policy(root)?;
    let cargo = metadata::cargo_metadata(root)?;
    validate_plan(&policy, &cargo)?;
    let packages = selected_packages(&policy, &cargo, operation, shard)?;
    let mut command = cargo_command(root, operation, &packages);
    let status = command
        .status()
        .map_err(|error| XtaskError::io("execute reviewed CI shard from", root, error))?;
    if !status.success() {
        return Err(XtaskError::metadata(format!(
            "CI shard `{shard}` failed during {operation:?} with status {status}"
        )));
    }
    Ok(packages.len())
}

fn selected_packages<'a>(
    policy: &ArchitecturePolicy,
    cargo: &'a CargoMetadata,
    operation: Operation,
    shard: &str,
) -> Result<Vec<&'a str>, XtaskError> {
    let policy_by_name: BTreeMap<_, _> =
        policy.packages.iter().map(|package| (package.name.as_str(), package)).collect();
    let workspace_ids: BTreeSet<_> = cargo.workspace_members.iter().map(String::as_str).collect();
    let mut selected = cargo
        .packages
        .iter()
        .filter(|package| workspace_ids.contains(package.id.as_str()))
        .filter(|package| {
            policy_by_name
                .get(package.name.as_str())
                .is_some_and(|entry| shard_for_package(&package.name, &entry.layer) == Some(shard))
        })
        .filter(|package| {
            verus_eligible(package, policy_by_name.get(package.name.as_str()), operation)
        })
        .filter(|package| !operation.is_platform_terminal() || package.name == PLATFORM_PACKAGE)
        .filter(|package| operation.runner_tests().is_none() || package.name == runner::PACKAGE)
        .map(|package| package.name.as_str())
        .collect::<Vec<_>>();
    selected.sort_unstable();
    if selected.is_empty() {
        return Err(XtaskError::metadata(format!(
            "CI shard `{shard}` selects no packages for {operation:?}"
        )));
    }
    Ok(selected)
}

fn verus_eligible(
    package: &CargoPackage,
    policy: Option<&&crate::model::PackagePolicy>,
    operation: Operation,
) -> bool {
    if !operation.is_verus() {
        return true;
    }
    let opted_in = package.metadata.verus.as_ref().is_some_and(|metadata| metadata.verify);
    opted_in
        && (!operation.is_strict()
            || policy.is_some_and(|entry| matches!(entry.verification_class.as_str(), "V" | "H")))
}

fn cargo_command(root: &Path, operation: Operation, packages: &[&str]) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(root);
    match operation {
        Operation::Build => {
            command.args(["build", "--locked", "--all-targets", "--all-features"]);
        }
        Operation::Test if packages == [runner::PACKAGE] => {
            command.args([
                "test",
                "--locked",
                "--lib",
                "--bins",
                "--examples",
                "--benches",
                "--all-features",
            ]);
        }
        Operation::Test => {
            command.args(["test", "--locked", "--all-targets", "--all-features"]);
        }
        Operation::TestRunnerRecovery | Operation::TestRunnerProduct => {
            command.args(["test", "--locked", "--all-features"]);
            for test in operation.runner_tests().into_iter().flatten() {
                command.args(["--test", test]);
            }
        }
        Operation::TestPlatformTerminalInteractive
        | Operation::TestPlatformTerminalSignal
        | Operation::TestPlatformTerminalCancel => {
            command.args(["test", "--locked", "--all-features"]);
        }
        Operation::DocTest => {
            command.args(["test", "--locked", "--doc", "--all-features"]);
        }
        Operation::Clippy => {
            command.args(["clippy", "--locked", "--all-targets", "--all-features"]);
        }
        Operation::Docs => {
            command.env("RUSTDOCFLAGS", "-D warnings");
            command.args(["doc", "--locked", "--all-features", "--no-deps"]);
        }
        Operation::VerusVerify | Operation::VerusVerifyStrict => {
            command.args(["verus", "verify"]);
        }
        Operation::VerusBuild | Operation::VerusBuildStrict => {
            command.args(["verus", "build", "--release"]);
        }
    }
    for package in packages {
        command.args(["--package", package]);
    }
    if operation.is_verus() {
        command.args([
            "--all-features",
            "--locked",
            "--check-toolchain",
            "--fwd-verus-args-to",
            "roots",
            "--",
        ]);
        if operation.is_strict() {
            command.arg("--no-cheating");
        }
        command.args(["--rlimit", "20"]);
    } else if let Some(test) = operation.platform_terminal_test() {
        command.args(["--test", "general_capability", test, "--", "--exact", "--test-threads=1"]);
    } else if matches!(operation, Operation::Test) || operation.runner_tests().is_some() {
        command.args(["--", "--test-threads=1"]);
        if packages == [PLATFORM_PACKAGE] {
            for test in
                [PLATFORM_TERMINAL_INTERACTIVE, PLATFORM_TERMINAL_SIGNAL, PLATFORM_TERMINAL_CANCEL]
            {
                command.args(["--skip", test]);
            }
        }
    } else if matches!(operation, Operation::Clippy) {
        command.args(["--", "-D", "warnings"]);
    }
    command
}

fn validate_plan(policy: &ArchitecturePolicy, cargo: &CargoMetadata) -> Result<(), XtaskError> {
    let workspace_ids: BTreeSet<_> = cargo.workspace_members.iter().map(String::as_str).collect();
    let workspace_names: BTreeSet<_> = cargo
        .packages
        .iter()
        .filter(|package| workspace_ids.contains(package.id.as_str()))
        .map(|package| package.name.as_str())
        .collect();
    let policy_names: BTreeSet<_> =
        policy.packages.iter().map(|package| package.name.as_str()).collect();
    if workspace_names != policy_names {
        return Err(XtaskError::metadata(
            "CI shard plan requires architecture policy to cover every workspace package exactly",
        ));
    }
    let unknown_layers = policy
        .packages
        .iter()
        .filter(|package| shard_for_layer(&package.layer).is_none())
        .map(|package| package.layer.as_str())
        .collect::<BTreeSet<_>>();
    if !unknown_layers.is_empty() {
        return Err(XtaskError::metadata(format!(
            "CI shard plan has unmapped architecture layers: {}",
            unknown_layers.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    runner::validate(cargo)
}

fn shard_for_layer(layer: &str) -> Option<&'static str> {
    match layer {
        "foundation" | "state" => Some("foundation-state"),
        "runtime" | "tools" => Some("runtime-tools"),
        "model" | "orchestration" => Some("model-orchestration"),
        "app" => Some("app-shell"),
        "testing" => Some("testing"),
        "analysis" | "observe" | "extensions" | "engineering" => Some("edge"),
        _ => None,
    }
}

fn shard_for_package(name: &str, layer: &str) -> Option<&'static str> {
    if layer == "app" && name == "peritus-product-runner" {
        Some("app-runner")
    } else if layer == "testing" {
        match name {
            "peritus-platform-qualification" => Some("testing-platform"),
            "peritus-external-benchmarks" => Some("testing-external"),
            _ => Some("testing"),
        }
    } else {
        shard_for_layer(layer)
    }
}

#[cfg(test)]
mod tests;
