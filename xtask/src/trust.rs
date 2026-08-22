use crate::error::{Diagnostic, ErrorCode, XtaskError};
use crate::metadata;
use crate::model::{ArchitecturePolicy, CargoMetadata};
use crate::source;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "trust/construct.rs"]
mod construct;
#[path = "trust/dependency_execution.rs"]
mod dependency_execution;
#[path = "trust_lexer.rs"]
mod lexer;
#[path = "trust/manifest.rs"]
mod manifest;
#[path = "trust/manifest_actor.rs"]
mod manifest_actor;
#[path = "trust/manifest_actor_model.rs"]
mod manifest_actor_model;
#[path = "trust/manifest_context.rs"]
mod manifest_context;
#[path = "trust/manifest_coverage.rs"]
mod manifest_coverage;
#[path = "trust/manifest_date.rs"]
mod manifest_date;
#[path = "trust/manifest_evidence.rs"]
mod manifest_evidence;
#[path = "trust/manifest_file.rs"]
mod manifest_file;
#[path = "trust/manifest_impact.rs"]
mod manifest_impact;
#[path = "trust/manifest_model.rs"]
mod manifest_model;
#[path = "trust/manifest_support.rs"]
mod manifest_support;
#[path = "trust/manifest_trust.rs"]
mod manifest_trust;

pub(crate) fn check(root: &Path, policy: &ArchitecturePolicy) -> Result<usize, XtaskError> {
    let cargo = metadata::cargo_metadata(root)?;
    let dependencies = metadata::cargo_metadata_with_dependencies(root)?;
    let (target_roots, mut diagnostics) = workspace_target_policy(root, &cargo);
    dependency_execution::validate(root, &dependencies, &mut diagnostics);
    check_with_policy_diagnostics(root, policy, Some(&cargo), &target_roots, diagnostics)
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
            if target.kind.iter().any(|kind| kind == "custom-build") {
                let relative = target.src_path.strip_prefix(root).unwrap_or(&target.src_path);
                diagnostics.push(Diagnostic::at(
                    relative,
                    format!(
                        "workspace package `{}` defines a build-script target; candidate code would execute before Gate A completes",
                        package.name
                    ),
                    "remove build.rs and package build configuration; foundation workspace build scripts require a separately isolated and reviewed execution model",
                ));
            }
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
fn check_cargo_fixture(root: &Path, policy: &ArchitecturePolicy) -> Result<usize, XtaskError> {
    let cargo = metadata::cargo_metadata(root)?;
    let dependencies = metadata::cargo_metadata_with_dependencies(root)?;
    let (target_roots, mut diagnostics) = workspace_target_policy(root, &cargo);
    dependency_execution::validate(root, &dependencies, &mut diagnostics);
    check_with_policy_diagnostics(root, policy, None, &target_roots, diagnostics)
}

#[cfg(test)]
fn check_with_roots(
    root: &Path,
    policy: &ArchitecturePolicy,
    target_roots: &[PathBuf],
) -> Result<usize, XtaskError> {
    check_with_policy_diagnostics(root, policy, None, target_roots, Vec::new())
}

fn check_with_policy_diagnostics(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: Option<&CargoMetadata>,
    target_roots: &[PathBuf],
    mut diagnostics: Vec<Diagnostic>,
) -> Result<usize, XtaskError> {
    let discovery = source::discover_compilation_sources(root, policy, target_roots)?;
    diagnostics.extend(discovery.diagnostics);
    let compilation_sources = discovery.files.clone();
    let mut scanned = 0;
    let mut trusted_occurrences = Vec::new();

    for file in discovery.files {
        let relative = file.strip_prefix(root).unwrap_or(&file);
        let contents =
            fs::read_to_string(&file).map_err(|error| XtaskError::io("read", &file, error))?;
        scanned += 1;
        let occurrences = lexer::scan(&contents);
        let is_trusted_root =
            policy.trusted_source_roots.iter().any(|allowed| relative.starts_with(allowed));

        let (prohibited, occurrences): (Vec<_>, Vec<_>) = occurrences
            .into_iter()
            .partition(|occurrence| occurrence.construct.is_prohibited_everywhere());
        diagnostics.extend(prohibited.into_iter().map(|occurrence| {
            Diagnostic::at(
                relative,
                format!(
                    "line {} imports, reexports, or aliases a trusted operation or constructor",
                    occurrence.line
                ),
                "use only canonical spellings at call sites so every trusted occurrence is independently countable",
            )
        }));

        if is_trusted_root {
            diagnostics.extend(
                occurrences
                    .iter()
                    .filter(|occurrence| occurrence.nested_item_scope)
                    .map(|occurrence| {
                        Diagnostic::at(
                            relative,
                            format!(
                                "line {} trusted construct `{}` is nested in an inline module, impl, trait, or type",
                                occurrence.line,
                                occurrence.construct.label()
                            ),
                            "place the narrowly reviewed boundary in a file-level item so its exact symbol is mechanically unambiguous",
                        )
                    }),
            );
            trusted_occurrences.extend(occurrences.into_iter().map(|occurrence| {
                let line = u64::try_from(occurrence.line).unwrap_or(u64::MAX);
                manifest::TrustedOccurrence {
                    source: relative.to_path_buf(),
                    line,
                    construct: occurrence.construct.label(),
                    symbol: manifest_support::governing_symbol(
                        "peritus-tcb",
                        relative,
                        &contents,
                        line,
                        occurrence.construct.label(),
                    )
                    .unwrap_or_else(|| "<unresolved>".to_owned()),
                }
            }));
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

    if let Some(cargo) = cargo {
        manifest::validate(
            root,
            policy,
            cargo,
            &compilation_sources,
            &trusted_occurrences,
            true,
            &mut diagnostics,
        )?;
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

#[cfg(test)]
#[path = "trust/manifest_tests.rs"]
mod manifest_tests;
