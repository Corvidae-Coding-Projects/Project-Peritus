use crate::error::{Diagnostic, ErrorCode, XtaskError};
use crate::metadata;
use crate::model::ArchitecturePolicy;
use crate::source;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "api_contract/expansion.rs"]
mod expansion;
#[path = "api_contract/scanner.rs"]
mod scanner;
#[path = "api_contract/signature.rs"]
mod signature;
#[path = "api_contract/verifier_only.rs"]
mod verifier_only;
#[path = "api_contract/violation.rs"]
mod violation;

/// Successful ordinary-Rust boundary audit statistics.
#[derive(Debug)]
pub(crate) struct Report {
    pub(crate) files: usize,
    pub(crate) executable_entry_points: usize,
}

/// Checks the source-level ordinary-Rust contract of every formal-boundary package.
///
/// This deliberately conservative repository check covers the API properties enforced by
/// Verus's experimental `check-api-safety` pass without applying that pass to imported `vstd`.
/// The pinned `vstd` currently contains intentional external specifications that make the
/// whole-import pass unusable as a repository gate.
pub(crate) fn check(root: &Path, policy: &ArchitecturePolicy) -> Result<Report, XtaskError> {
    let cargo = metadata::cargo_metadata(root)?;
    let workspace_ids: BTreeSet<_> = cargo.workspace_members.iter().map(String::as_str).collect();
    let formal_packages: BTreeSet<_> = policy
        .packages
        .iter()
        .filter(|package| matches!(package.verification_class.as_str(), "V" | "H" | "T"))
        .map(|package| package.name.as_str())
        .collect();
    let target_roots = cargo
        .packages
        .iter()
        .filter(|package| workspace_ids.contains(package.id.as_str()))
        .filter(|package| formal_packages.contains(package.name.as_str()))
        .flat_map(|package| package.targets.iter().map(|target| target.src_path.clone()))
        .collect::<Vec<_>>();
    check_with_roots(root, policy, &target_roots)
}

fn check_with_roots(
    root: &Path,
    policy: &ArchitecturePolicy,
    target_roots: &[PathBuf],
) -> Result<Report, XtaskError> {
    let formal_roots: Vec<_> = policy
        .packages
        .iter()
        .filter(|package| matches!(package.verification_class.as_str(), "V" | "H" | "T"))
        .map(|package| package.path.as_path())
        .collect();
    let b1_production_roots: Vec<_> = policy
        .packages
        .iter()
        .filter(|package| package.owner == "B1")
        .map(|package| package.path.join("src"))
        .collect();
    let discovery = source::discover_compilation_sources(root, policy, target_roots)?;
    let mut diagnostics = discovery.diagnostics;
    let mut scanned = 0;
    let mut entry_points = 0;

    for file in discovery.files {
        let relative = file.strip_prefix(root).unwrap_or(&file);
        if !formal_roots.iter().any(|package| relative.starts_with(package)) {
            continue;
        }
        let metadata =
            fs::symlink_metadata(&file).map_err(|error| XtaskError::io("inspect", &file, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            diagnostics.push(Diagnostic::at(
                relative,
                "formal-boundary source is not a repository-owned regular file",
                "replace it with checked-in regular Rust source before auditing its public API",
            ));
            continue;
        }
        let contents =
            fs::read_to_string(&file).map_err(|error| XtaskError::io("read", &file, error))?;
        let result = scanner::scan(&contents);
        scanned += 1;
        entry_points += result.executable_entry_points;
        diagnostics.extend(
            result
                .violations
                .into_iter()
                .map(|violation| Diagnostic::at(relative, violation.message(), violation.help())),
        );
        if b1_production_roots.iter().any(|source| relative.starts_with(source)) {
            diagnostics.extend(verifier_only::violations(&contents).into_iter().map(|violation| {
                Diagnostic::at(relative, violation.message(), verifier_only::Violation::help())
            }));
        }
    }

    if diagnostics.is_empty() {
        Ok(Report { files: scanned, executable_entry_points: entry_points })
    } else {
        Err(XtaskError::violations(ErrorCode::ApiContract, "ordinary-api-check", diagnostics))
    }
}

#[cfg(test)]
#[path = "api_contract/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "api_contract/integration_tests.rs"]
mod integration_tests;

#[cfg(test)]
#[path = "api_contract/signature_tests.rs"]
mod signature_tests;

#[cfg(test)]
#[path = "api_contract/policy_tests.rs"]
mod policy_tests;
