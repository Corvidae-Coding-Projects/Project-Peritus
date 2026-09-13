//! Compiler-observed proof selection for the existing strict verification shards.

mod inputs;
mod report;

pub(crate) use report::observation;

use super::{Operation, cargo_command, selected_packages, validate_plan};
use crate::error::XtaskError;
use crate::{metadata, trust};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Clear generated reports before policy or Cargo metadata can fail in the shard dispatcher.
fn prepare_reports(root: &Path, shard: &str) -> Result<(), XtaskError> {
    let directory = root.join("target/formal-scope").join(shard);
    // This directory contains only this shard's generated reports. An early failure must not
    // leave an earlier run's raw compiler output or success sidecars for the artifact uploader.
    match fs::remove_dir_all(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(XtaskError::io("clear previous proof reports at", &directory, error));
        }
    }
    fs::create_dir_all(&directory)
        .map_err(|error| XtaskError::io("create proof scope output at", &directory, error))
}

/// Requires each claimed symbol in its own package's fresh compiler invocation.
///
/// Executable and proof evidence must produce a successful query in its source-declared mode;
/// spec evidence must be selected because a spec definition need not produce a standalone query.
/// Compiler observation does not discharge an obligation or establish production correspondence.
pub(super) fn run(root: &Path, shard: &str) -> Result<usize, XtaskError> {
    prepare_reports(root, shard)?;
    let directory = root.join("target/formal-scope").join(shard);
    let before = inputs::capture(root)?;
    save_json(&directory.join("source-inputs.json"), &before)?;
    // All metadata used for selection is read after the first snapshot. The discovery metadata
    // inside capture only enumerates inputs and is never used to select verification roots.
    let policy = metadata::architecture_policy(root)?;
    let cargo = metadata::cargo_metadata(root)?;
    validate_plan(&policy, &cargo)?;
    let packages = selected_packages(&policy, &cargo, Operation::VerusVerifyStrict, shard)?;
    let tools = metadata::toolchain_policy(root)?;
    let expected = trust::registered_proof_symbols(root)?;
    let target = root.join("target/formal-verify");
    let mut scopes = BTreeMap::new();
    for package in &packages {
        let metadata =
            cargo.packages.iter().find(|entry| entry.name == *package).ok_or_else(|| {
                XtaskError::metadata(format!("missing proof package `{package}`"))
            })?;
        let roots: Vec<_> = metadata
            .targets
            .iter()
            .filter(|entry| entry.kind.iter().any(|kind| matches!(kind.as_str(), "lib" | "bin")))
            .map(|entry| entry.name.as_str())
            .collect();
        let output = verify_package(root, &target, &directory, package)?;
        let reports = report::parse(&output)?;
        let scope = report::check(&reports, package, &roots, &expected, &tools)?;
        let path = directory.join(format!("{package}-selected-functions.json"));
        save_json(&path, &scope)?;
        writeln!(
            io::stdout().lock(),
            "{package}: {} registered symbols selected; {} solver-query functions",
            scope.registered_symbols,
            scope.queried_functions.len()
        )
        .map_err(|error| XtaskError::io("report proof scope for", root, error))?;
        scopes.insert(*package, scope);
    }
    // Publish a complete shard report only after every requested package has succeeded and
    // both endpoint snapshots match. This does not detect changed-and-restored inputs during
    // compilation, or attest external tool trust; runner isolation remains a separate boundary.
    let after = inputs::capture(root)?;
    before.require_unchanged(&after)?;
    save_json(&directory.join("selected-functions.json"), &scopes)?;
    Ok(packages.len())
}

fn verify_package(
    root: &Path,
    target: &Path,
    directory: &Path,
    package: &str,
) -> Result<Vec<u8>, XtaskError> {
    // Cargo can skip checked roots without re-emitting their reports. Clear this root only in
    // the dedicated verification target, keeping dependency builds available to later packages.
    let status = Command::new("cargo")
        .current_dir(root)
        .args(["clean", "--locked", "--target-dir"])
        .arg(target)
        .args(["--package", package])
        .status()
        .map_err(|error| XtaskError::io("prepare fresh proof root at", target, error))?;
    if !status.success() {
        return Err(XtaskError::metadata(format!(
            "cannot prepare proof root `{package}`: {status}"
        )));
    }
    let mut command = cargo_command(root, Operation::VerusVerifyStrict, &[package]);
    command.env("CARGO_TARGET_DIR", target);
    command.args(["--output-json", "--time"]);
    command.stderr(Stdio::inherit());
    let output = command
        .output()
        .map_err(|error| XtaskError::io("execute strict proof scope from", root, error))?;
    let raw = directory.join(format!("{package}-verus-output.txt"));
    fs::write(&raw, &output.stdout)
        .map_err(|error| XtaskError::io("retain compiler proof output at", &raw, error))?;
    if !output.status.success() {
        return Err(XtaskError::metadata(format!(
            "strict Verus package `{package}` failed with {}; output: {}",
            output.status,
            raw.display()
        )));
    }
    Ok(output.stdout)
}

fn save_json(path: &Path, value: &impl serde::Serialize) -> Result<(), XtaskError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| XtaskError::metadata(format!("cannot encode proof scope: {error}")))?;
    fs::write(path, bytes)
        .map_err(|error| XtaskError::io("retain selected proof scope at", path, error))
}

#[cfg(test)]
mod tests;
