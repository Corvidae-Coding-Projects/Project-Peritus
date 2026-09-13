//! Additional feature configurations in existing ordinary package shards.

use super::Operation;
use crate::error::XtaskError;
use std::path::Path;
use std::process::Command;

const AGENT: &str = "peritus-agent";
const SHARD: &str = "model-orchestration";

/// Run after the shard's normal command succeeds. A separate Cargo invocation prevents the
/// other selected packages' default features from enabling the agent's optional protocol bridge.
pub(super) fn run(
    root: &Path,
    operation: Operation,
    shard: &str,
    packages: &[&str],
) -> Result<(), XtaskError> {
    if let Some(mut command) = command(root, operation, shard, packages)? {
        execute(root, operation, &mut command)?;
    }
    Ok(())
}

fn command(
    root: &Path,
    operation: Operation,
    shard: &str,
    packages: &[&str],
) -> Result<Option<Command>, XtaskError> {
    let name = match operation {
        Operation::Build => "build",
        Operation::Test => "test",
        Operation::Clippy => "clippy",
        _ => return Ok(None),
    };
    if shard != SHARD {
        return Ok(None);
    }
    if !packages.contains(&AGENT) {
        return Err(XtaskError::metadata(
            "model-orchestration feature checks require peritus-agent in the selected packages",
        ));
    }
    let mut command = Command::new("cargo");
    command.current_dir(root).args([
        name,
        "--locked",
        "--all-targets",
        "--no-default-features",
        "--package",
        AGENT,
    ]);
    if operation == Operation::Test {
        command.args(["--", "--test-threads=1"]);
    } else if operation == Operation::Clippy {
        command.args(["--", "-D", "warnings"]);
    }
    Ok(Some(command))
}

fn execute(root: &Path, operation: Operation, command: &mut Command) -> Result<(), XtaskError> {
    let status = command.status().map_err(|error| {
        XtaskError::io("execute agent no-default-features check from", root, error)
    })?;
    if !status.success() {
        return Err(XtaskError::metadata(format!(
            "CI no-default-features check for `{AGENT}` failed during {operation:?}: {status}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
