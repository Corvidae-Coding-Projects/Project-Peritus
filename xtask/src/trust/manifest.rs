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
    if policy.trusted_source_roots != [PathBuf::from(TCB_SOURCE_ROOT)] {
        diagnostics.push(Diagnostic::at(
            "architecture.toml",
            "A1 trusted roots are not the exact peritus-tcb source boundary",
            format!("set trusted_source_roots to exactly [`{TCB_SOURCE_ROOT}`]"),
        ));
    }
    validate_inventory(root, diagnostics);
    let actors: Option<ActorsDocument> =
        manifest_file::read_toml(root, Path::new(ACTORS_PATH), diagnostics);
    let trust: Option<TrustDocument> =
        manifest_file::read_toml(root, Path::new(TRUST_PATH), diagnostics);
    let exclusions: Option<ExclusionsDocument> =
        manifest_file::read_toml(root, Path::new(EXCLUSIONS_PATH), diagnostics);
    let obligations: Option<ObligationsDocument> =
        manifest_file::read_toml(root, Path::new(OBLIGATIONS_PATH), diagnostics);
    let proof_impact: Option<ProofImpactDocument> =
        manifest_file::read_toml(root, Path::new(PROOF_IMPACT_PATH), diagnostics);
    let (Some(actors), Some(trust), Some(exclusions), Some(obligations), Some(proof_impact)) =
        (actors, trust, exclusions, obligations, proof_impact)
    else {
        return Ok(());
    };
    let context = ManifestContext::new(root, policy, cargo);
    let Some(actors) = manifest_actor::validate(root, &actors, diagnostics) else {
        return Ok(());
    };
    manifest_trust::validate(&context, &actors, &trust, occurrences, diagnostics);
    let indexed_exclusions = validate_exclusions(&context, &actors, &exclusions, diagnostics);
    validate_obligations(&context, &actors, &obligations, &indexed_exclusions, diagnostics);
    manifest_impact::validate(
        &context,
        &actors,
        &proof_impact,
        compilation_sources,
        enforce_review_base,
        diagnostics,
    )?;
    Ok(())
}

fn validate_inventory(root: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let expected = [ACTORS_PATH, TRUST_PATH, EXCLUSIONS_PATH, OBLIGATIONS_PATH, PROOF_IMPACT_PATH];
    for relative in expected {
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
        if file_type.is_symlink() || file_type.is_dir() {
            diagnostics.push(Diagnostic::at(
                path.strip_prefix(root).unwrap_or(&path),
                "verification inventory contains a symlink or nested directory",
                "keep the A1 manifest directory flat and fully scanned",
            ));
        }
        let is_toml = path.extension().and_then(|extension| extension.to_str()) == Some("toml");
        if is_toml && !expected.iter().any(|expected| path == root.join(expected)) {
            diagnostics.push(Diagnostic::at(
                path.strip_prefix(root).unwrap_or(&path),
                "unregistered verification TOML can evade the canonical schemas",
                "merge the record into one of the five registered verification manifests",
            ));
        }
    }
}
