use super::MANIFEST;
use crate::error::Diagnostic;
use crate::reproducibility;
use crate::trust::manifest_model::{ProofImpactChange, ProofImpactEvidenceKind};
use std::collections::BTreeMap;

pub(super) fn validate(change: &ProofImpactChange, diagnostics: &mut Vec<Diagnostic>) {
    let mut impacted = BTreeMap::new();
    for source in &change.source_changes {
        for snapshot in [source.previous.as_ref(), source.current.as_ref()].into_iter().flatten() {
            for package in &snapshot.affected_packages {
                if let Some(previous) =
                    impacted.insert(package.package.as_str(), package.verification_class.as_str())
                    && previous != package.verification_class
                {
                    diagnostics.push(Diagnostic::at(
                        MANIFEST,
                        format!(
                            "change `{}` assigns conflicting classes to `{}`",
                            change.id, package.package
                        ),
                        "use the one exact architecture class for every affected package",
                    ));
                }
            }
        }
    }
    for (package, class) in &impacted {
        for kind in [ProofImpactEvidenceKind::OrdinaryTest, ProofImpactEvidenceKind::VerusVerify] {
            let matching: Vec<_> = change
                .evidence
                .iter()
                .filter(|item| item.owning_crate == *package && item.kind == kind)
                .collect();
            if matching.len() != 1
                || !reproducibility::is_exact_evidence_command(
                    &matching[0].command,
                    &matching[0].owning_crate,
                    class,
                )
            {
                diagnostics.push(Diagnostic::at(
                    MANIFEST,
                    format!(
                        "change `{}` lacks one exact {kind:?} command for `{package}`",
                        change.id
                    ),
                    "record the canonical locked package test and class-correct Cargo-Verus verification commands",
                ));
            }
        }
    }
    if change.evidence.len() != impacted.len() * 2 {
        diagnostics.push(Diagnostic::at(
            MANIFEST,
            format!("change `{}` has duplicate or unrelated evidence commands", change.id),
            "retain exactly one ordinary test and one full verification command per affected package",
        ));
    }
}
