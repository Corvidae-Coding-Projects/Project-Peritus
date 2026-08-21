use crate::error::{Diagnostic, ErrorCode, XtaskError};
use crate::metadata;
use crate::model::{ArchitecturePolicy, CargoMetadata};
use crate::source;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "trust/construct.rs"]
mod construct;
#[path = "trust_lexer.rs"]
mod lexer;

pub(crate) fn check(root: &Path, policy: &ArchitecturePolicy) -> Result<usize, XtaskError> {
    let cargo = metadata::cargo_metadata(root)?;
    let (target_roots, diagnostics) = workspace_target_policy(root, &cargo);
    check_with_policy_diagnostics(root, policy, &target_roots, diagnostics)
}

fn workspace_target_policy(root: &Path, cargo: &CargoMetadata) -> (Vec<PathBuf>, Vec<Diagnostic>) {
    let workspace_ids: BTreeSet<_> = cargo.workspace_members.iter().map(String::as_str).collect();
    let mut roots = Vec::new();
    let mut diagnostics = Vec::new();
    for package in
        cargo.packages.iter().filter(|package| workspace_ids.contains(package.id.as_str()))
    {
        for target in &package.targets {
            roots.push(target.src_path.clone());
            if target.kind.iter().chain(&target.crate_types).any(|kind| kind == "proc-macro") {
                let relative = target.src_path.strip_prefix(root).unwrap_or(&target.src_path);
                diagnostics.push(Diagnostic::at(
                    relative,
                    format!(
                        "workspace package `{}` defines a procedural-macro target; generated tokens cannot be trust-scanned",
                        package.name
                    ),
                    "remove the workspace procedural macro; A0 permits external pinned proc macros only through the dependency and full-verification boundary",
                ));
            }
        }
    }
    (roots, diagnostics)
}

#[cfg(test)]
fn check_with_roots(
    root: &Path,
    policy: &ArchitecturePolicy,
    target_roots: &[PathBuf],
) -> Result<usize, XtaskError> {
    check_with_policy_diagnostics(root, policy, target_roots, Vec::new())
}

fn check_with_policy_diagnostics(
    root: &Path,
    policy: &ArchitecturePolicy,
    target_roots: &[PathBuf],
    mut diagnostics: Vec<Diagnostic>,
) -> Result<usize, XtaskError> {
    let discovery = source::discover_compilation_sources(root, policy, target_roots)?;
    diagnostics.extend(discovery.diagnostics);
    let mut scanned = 0;

    for file in discovery.files {
        let relative = file.strip_prefix(root).unwrap_or(&file);
        let contents =
            fs::read_to_string(&file).map_err(|error| XtaskError::io("read", &file, error))?;
        scanned += 1;
        let occurrences = lexer::scan(&contents);
        let is_trusted_root =
            policy.trusted_source_roots.iter().any(|allowed| relative.starts_with(allowed));

        if is_trusted_root {
            // Trusted roots allow audited constructs, but scanning them here keeps the
            // occurrence inventory available for the A1 manifest reconciliation.
            continue;
        }

        diagnostics.extend(occurrences.into_iter().map(|occurrence| {
            Diagnostic::at(
                relative,
                format!(
                    "line {} contains trusted construct `{}` outside an allowed trust root",
                    occurrence.line,
                    occurrence.construct.label()
                ),
                "remove the construct or move the narrowly audited boundary into peritus-tcb with a manifest entry",
            )
        }));
    }

    if diagnostics.is_empty() {
        Ok(scanned)
    } else {
        Err(XtaskError::violations(ErrorCode::Trust, "verify-trust", diagnostics))
    }
}

#[cfg(test)]
fn check_fixture(root: &Path, policy: &ArchitecturePolicy) -> Result<usize, XtaskError> {
    check_with_roots(root, policy, &[])
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "trust/source_discovery_tests.rs"]
mod source_discovery_tests;

#[cfg(test)]
#[path = "trust/include_policy_tests.rs"]
mod include_policy_tests;

#[cfg(test)]
#[path = "trust/cargo_target_tests.rs"]
mod cargo_target_tests;
