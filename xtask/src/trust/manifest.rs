use super::manifest_actor;
use super::manifest_actor_model::ActorsDocument;
use super::manifest_context::ManifestContext;
use super::manifest_coverage::{validate_exclusions, validate_obligations};
use super::manifest_file;
use super::manifest_impact;
use super::manifest_model::{
    ExclusionsDocument, ObligationsDocument, ProofImpactDocument, TrustDocument,
};
use super::manifest_trust;
use crate::error::{Diagnostic, XtaskError};
use crate::model::{ArchitecturePolicy, CargoMetadata};
use std::fs;
use std::path::{Path, PathBuf};

const ACTORS_PATH: &str = "verification/actors.toml";
const TRUST_PATH: &str = "verification/trust.toml";
const EXCLUSIONS_PATH: &str = "verification/exclusions.toml";
const OBLIGATIONS_PATH: &str = "verification/obligations.toml";
const PROOF_IMPACT_PATH: &str = "verification/proof-impact.toml";
const TCB_SOURCE_ROOT: &str = "crates/foundation/peritus-tcb/src";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TrustedOccurrence {
    pub(super) source: PathBuf,
    pub(super) line: u64,
    pub(super) construct: &'static str,
    pub(super) symbol: String,
}

pub(super) fn validate(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
    compilation_sources: &[PathBuf],
    occurrences: &[TrustedOccurrence],
    enforce_review_base: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    validate_with_proof_impact(
        root,
        policy,
        cargo,
        compilation_sources,
        occurrences,
        if enforce_review_base {
            ProofImpactValidation::Enforced
        } else {
            ProofImpactValidation::Unprotected
        },
        diagnostics,
    )
}

pub(super) fn validate_local(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
    compilation_sources: &[PathBuf],
    occurrences: &[TrustedOccurrence],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    validate_with_proof_impact(
        root,
        policy,
        cargo,
        compilation_sources,
        occurrences,
        ProofImpactValidation::Local,
        diagnostics,
    )
}

pub(super) fn validate_candidate(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
    compilation_sources: &[PathBuf],
    occurrences: &[TrustedOccurrence],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    validate_with_proof_impact(
        root,
        policy,
        cargo,
        compilation_sources,
        occurrences,
        ProofImpactValidation::Candidate,
        diagnostics,
    )
}

pub(super) fn validate_local_authorization(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<bool, XtaskError> {
    validate_inventory(root, diagnostics);
    let Some(document) = manifest_file::read_toml::<ProofImpactDocument>(
        root,
        Path::new(PROOF_IMPACT_PATH),
        diagnostics,
    ) else {
        return Ok(false);
    };
    let context = ManifestContext::new(root, policy, cargo);
    if !manifest_impact::is_authorization_phase(&context, &document, diagnostics)? {
        return Ok(false);
    }
    manifest_impact::validate_authorization(&context, &document, false, diagnostics)?;
    Ok(true)
}

fn validate_with_proof_impact(
    root: &Path,
    policy: &ArchitecturePolicy,
    cargo: &CargoMetadata,
    compilation_sources: &[PathBuf],
    occurrences: &[TrustedOccurrence],
    proof_impact: ProofImpactValidation,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    if policy.trusted_source_roots != [PathBuf::from(TCB_SOURCE_ROOT)] {
        diagnostics.push(Diagnostic::at(
            "architecture.toml",
            "A1 trusted roots are not the exact peritus-tcb source boundary",
            format!("set trusted_source_roots to exactly [`{TCB_SOURCE_ROOT}`]"),
        ));
    }
    validate_inventory(root, diagnostics);
    let proof_impact_document: Option<ProofImpactDocument> =
        manifest_file::read_toml(root, Path::new(PROOF_IMPACT_PATH), diagnostics);
    let context = ManifestContext::new(root, policy, cargo);
    if proof_impact.allows_authorization()
        && let Some(document) = &proof_impact_document
        && manifest_impact::is_authorization_phase(&context, document, diagnostics)?
    {
        manifest_impact::validate_authorization(
            &context,
            document,
            proof_impact.enforces_review_base(),
            diagnostics,
        )?;
        return Ok(());
    }
    let actors: Option<ActorsDocument> =
        manifest_file::read_toml(root, Path::new(ACTORS_PATH), diagnostics);
    let trust: Option<TrustDocument> =
        manifest_file::read_toml(root, Path::new(TRUST_PATH), diagnostics);
    let exclusions: Option<ExclusionsDocument> =
        manifest_file::read_toml(root, Path::new(EXCLUSIONS_PATH), diagnostics);
    let obligations: Option<ObligationsDocument> =
        manifest_file::read_toml(root, Path::new(OBLIGATIONS_PATH), diagnostics);
    let (Some(actors), Some(trust), Some(exclusions), Some(obligations)) =
        (actors, trust, exclusions, obligations)
    else {
        return Ok(());
    };
    let Some(actors) = manifest_actor::validate(root, &actors, diagnostics) else {
        return Ok(());
    };
    manifest_trust::validate(&context, &actors, &trust, occurrences, diagnostics);
    let indexed_exclusions = validate_exclusions(&context, &actors, &exclusions, diagnostics);
    validate_obligations(&context, &actors, &obligations, &indexed_exclusions, diagnostics);
    if let (Some(enforce_review_base), Some(proof_impact)) =
        (proof_impact.review_base_setting(), proof_impact_document)
    {
        manifest_impact::validate(
            &context,
            &actors,
            &proof_impact,
            compilation_sources,
            enforce_review_base,
            diagnostics,
        )?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ProofImpactValidation {
    Enforced,
    Unprotected,
    Local,
    Candidate,
}

impl ProofImpactValidation {
    const fn allows_authorization(self) -> bool {
        matches!(self, Self::Enforced | Self::Local)
    }

    const fn enforces_review_base(self) -> bool {
        matches!(self, Self::Enforced)
    }

    const fn review_base_setting(self) -> Option<bool> {
        match self {
            Self::Enforced => Some(true),
            Self::Unprotected => Some(false),
            Self::Local | Self::Candidate => None,
        }
    }
}

fn validate_inventory(root: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let required = [ACTORS_PATH, TRUST_PATH, EXCLUSIONS_PATH, OBLIGATIONS_PATH, PROOF_IMPACT_PATH];
    let registered = required;
    for relative in required {
        if !manifest_file::is_regular_without_symlink(root, Path::new(relative)) {
            diagnostics.push(Diagnostic::at(
                relative,
                "verification manifest is missing, non-regular, or reached through a symlink",
                "restore the exact checked-in TOML manifest as a regular file",
            ));
        }
    }
    let directory = root.join("verification");
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                "verification",
                format!("verification inventory cannot be read: {error}"),
                "restore the readable non-symlink verification directory",
            ));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                diagnostics.push(Diagnostic::at(
                    "verification",
                    format!("verification inventory entry cannot be read: {error}"),
                    "restore readable repository-owned inventory entries",
                ));
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                diagnostics.push(Diagnostic::at(
                    path.strip_prefix(root).unwrap_or(&path),
                    format!("verification inventory entry cannot be inspected: {error}"),
                    "restore a readable regular inventory file",
                ));
                continue;
            }
        };
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let registered_review_directory = relative == Path::new("verification/reviews");
        if file_type.is_symlink() || file_type.is_dir() && !registered_review_directory {
            diagnostics.push(Diagnostic::at(
                relative,
                "verification inventory contains a symlink or nested directory",
                "keep the manifest directory flat except for the recursively reconciled verification/reviews tree",
            ));
        }
        let is_toml = path.extension().and_then(|extension| extension.to_str()) == Some("toml");
        if is_toml && !registered.iter().any(|expected| path == root.join(expected)) {
            diagnostics.push(Diagnostic::at(
                path.strip_prefix(root).unwrap_or(&path),
                "unregistered verification TOML can evade the canonical schemas",
                "merge the record into one of the five registered verification manifests",
            ));
        }
    }
}
