use crate::api_contract::{Configuration, Mode};
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
pub(crate) use manifest_impact::render_snapshot as proof_impact_inventory;
#[path = "trust/manifest_model.rs"]
mod manifest_model;
#[path = "trust/manifest_support.rs"]
mod manifest_support;
#[path = "trust/manifest_symbol.rs"]
mod manifest_symbol;
#[path = "trust/manifest_trust.rs"]
mod manifest_trust;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RegisteredProofSymbol {
    pub(crate) obligation: String,
    pub(crate) owner: String,
    pub(crate) symbol: String,
    pub(crate) mode: Mode,
}

/// Returns the exact compiler-visible symbols claimed as Verus evidence by the register.
pub(crate) fn registered_proof_symbols(
    root: &Path,
) -> Result<Vec<RegisteredProofSymbol>, XtaskError> {
    let document: manifest_model::ObligationsDocument =
        metadata::read_toml(&root.join("verification/obligations.toml"))?;
    let mut registered = Vec::new();
    for entry in document.entries {
        for evidence in entry.evidence.into_iter().filter(|evidence| {
            matches!(evidence.kind, manifest_model::ProofEvidenceKind::VerusProof)
        }) {
            let source = root.join(&evidence.source_file);
            let contents = fs::read_to_string(&source)
                .map_err(|error| XtaskError::io("read registered proof source", &source, error))?;
            let name = evidence.symbol.rsplit("::").next().unwrap_or(&evidence.symbol);
            let matches: Vec<_> = manifest_symbol::owned_function_declarations(
                &entry.owning_crate,
                &source,
                &contents,
                name,
            )
            .into_iter()
            .filter(|declaration| declaration.path == evidence.symbol)
            .collect();
            let [declaration] = matches.as_slice() else {
                return Err(XtaskError::metadata(format!(
                    "registered proof `{}` does not resolve to exactly one declaration",
                    evidence.symbol
                )));
            };
            let Some(mode) = declaration.declaration.mode else {
                return Err(XtaskError::metadata(format!(
                    "registered proof `{}` has an unrecognized Verus function signature",
                    evidence.symbol
                )));
            };
            if !declaration.declaration.in_verus
                || declaration.declaration.nested
                || declaration.declaration.configuration != Configuration::Unconditional
            {
                return Err(XtaskError::metadata(format!(
                    "registered proof `{}` is not an unconditional non-local verus! declaration",
                    evidence.symbol
                )));
            }
            registered.push(RegisteredProofSymbol {
                obligation: entry.id.clone(),
                owner: entry.owning_crate.clone(),
                symbol: evidence.symbol,
                mode,
            });
        }
    }
    Ok(registered)
}

pub(crate) fn check(root: &Path, policy: &ArchitecturePolicy) -> Result<usize, XtaskError> {
    check_workspace(root, policy, ManifestValidation::Enforced)
}
/// Runs local trust checks and validates authorization against its verdict-declared base.
pub(crate) fn check_local(root: &Path, policy: &ArchitecturePolicy) -> Result<usize, XtaskError> {
    check_workspace(root, policy, ManifestValidation::Local)
}

/// Validates a materialized candidate without selecting its proof-impact authorization record.
pub(crate) fn check_candidate(
    root: &Path,
    policy: &ArchitecturePolicy,
) -> Result<usize, XtaskError> {
    check_workspace(root, policy, ManifestValidation::Candidate)
}

/// Validates a local authorization before `all` redirects ordinary policy to its exact candidate.
pub(crate) fn check_local_authorization(
    root: &Path,
    policy: &ArchitecturePolicy,
) -> Result<bool, XtaskError> {
    let cargo = metadata::cargo_metadata(root)?;
    let mut diagnostics = Vec::new();
    let authorization =
        manifest::validate_local_authorization(root, policy, &cargo, &mut diagnostics)?;
    if diagnostics.is_empty() {
        Ok(authorization)
    } else {
        Err(XtaskError::violations(ErrorCode::Trust, "verify-trust", diagnostics))
    }
}

#[derive(Clone, Copy)]
enum ManifestValidation {
    Enforced,
    Local,
    Candidate,
}

fn check_workspace(
    root: &Path,
    policy: &ArchitecturePolicy,
    manifest_validation: ManifestValidation,
) -> Result<usize, XtaskError> {
    let cargo = metadata::cargo_metadata(root)?;
    let dependencies = metadata::cargo_metadata_with_dependencies(root)?;
    let (target_roots, mut diagnostics) = workspace_target_policy(root, &cargo);
    dependency_execution::validate(root, &dependencies, &mut diagnostics);
    check_with_policy_diagnostics(
        root,
        policy,
        Some(&cargo),
        &target_roots,
        diagnostics,
        manifest_validation,
    )
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
    check_with_policy_diagnostics(
        root,
        policy,
        None,
        &target_roots,
        diagnostics,
        ManifestValidation::Enforced,
    )
}

#[cfg(test)]
fn check_with_roots(
    root: &Path,
    policy: &ArchitecturePolicy,
    target_roots: &[PathBuf],
) -> Result<usize, XtaskError> {
    check_with_policy_diagnostics(
        root,
        policy,
        None,
        target_roots,
        Vec::new(),
        ManifestValidation::Enforced,
    )
}

fn check_with_policy_diagnostics(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: Option<&CargoMetadata>,
    target_roots: &[PathBuf],
    mut diagnostics: Vec<Diagnostic>,
    manifest_validation: ManifestValidation,
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
        validate_manifests(
            root,
            policy,
            cargo,
            &compilation_sources,
            &trusted_occurrences,
            manifest_validation,
            &mut diagnostics,
        )?;
    }

    if diagnostics.is_empty() {
        Ok(scanned)
    } else {
        Err(XtaskError::violations(ErrorCode::Trust, "verify-trust", diagnostics))
    }
}

fn validate_manifests(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
    compilation_sources: &[PathBuf],
    trusted_occurrences: &[manifest::TrustedOccurrence],
    validation: ManifestValidation,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    match validation {
        ManifestValidation::Enforced => manifest::validate(
            root,
            policy,
            cargo,
            compilation_sources,
            trusted_occurrences,
            true,
            diagnostics,
        ),
        ManifestValidation::Local => manifest::validate_local(
            root,
            policy,
            cargo,
            compilation_sources,
            trusted_occurrences,
            diagnostics,
        ),
        ManifestValidation::Candidate => manifest::validate_candidate(
            root,
            policy,
            cargo,
            compilation_sources,
            trusted_occurrences,
            diagnostics,
        ),
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
