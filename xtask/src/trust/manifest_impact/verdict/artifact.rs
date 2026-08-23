//! Retained review-artifact inventory and raw-byte validation.

use super::{sha256_hex, valid_sha256};
use crate::error::Diagnostic;
use crate::trust::manifest_file;
use crate::trust::manifest_model::{
    ProofImpactVerdict, ProofImpactVerdictArtifact, ProofImpactVerdictArtifactKind,
    ProofImpactVerdictArtifactRef,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(super) fn validate(
    root: &Path,
    verdict_path: &Path,
    verdict: &ProofImpactVerdict,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let expected = expected_artifacts(verdict);
    let expected_count = 1 + verdict.gate_evidence.len() + verdict.findings.len() * 2;
    let actual: BTreeSet<_> = verdict.artifacts.iter().cloned().collect();
    let canonical = verdict.artifacts.windows(2).all(|pair| pair[0] < pair[1]);
    if expected.len() != expected_count
        || actual.len() != verdict.artifacts.len()
        || !canonical
        || actual != expected
    {
        diagnostics.push(Diagnostic::at(
            verdict_path,
            "detached verdict artifact inventory is not canonical and one-to-one",
            "retain one sorted unique report, every gate output, and every finding detail/evidence artifact",
        ));
    }
    if !valid_sha256(&verdict.artifact_inventory_sha256)
        || verdict.artifact_inventory_sha256
            != super::digest::artifact_inventory(&verdict.artifacts)
    {
        diagnostics.push(Diagnostic::at(
            verdict_path,
            "detached verdict artifact-inventory digest does not match its declared artifacts",
            "recompute the domain-separated inventory digest after all retained artifacts are final",
        ));
    }
    for artifact in &verdict.artifacts {
        validate_file(root, verdict_path, verdict, artifact, diagnostics);
    }
}

pub(super) fn declared_paths(verdict: &ProofImpactVerdict) -> impl Iterator<Item = &str> {
    verdict.artifacts.iter().map(|artifact| artifact.path.as_str())
}

fn expected_artifacts(verdict: &ProofImpactVerdict) -> BTreeSet<ProofImpactVerdictArtifact> {
    let mut expected = BTreeSet::from([from_ref(
        ProofImpactVerdictArtifactKind::ReviewReport,
        &verdict.review_report,
    )]);
    expected.extend(
        verdict
            .gate_evidence
            .iter()
            .map(|gate| from_ref(ProofImpactVerdictArtifactKind::GateOutput, &gate.output)),
    );
    for finding in &verdict.findings {
        expected.insert(from_ref(ProofImpactVerdictArtifactKind::FindingDetail, &finding.detail));
        expected
            .insert(from_ref(ProofImpactVerdictArtifactKind::FindingEvidence, &finding.evidence));
    }
    expected
}

fn from_ref(
    kind: ProofImpactVerdictArtifactKind,
    reference: &ProofImpactVerdictArtifactRef,
) -> ProofImpactVerdictArtifact {
    ProofImpactVerdictArtifact {
        kind,
        path: reference.path.clone(),
        sha256: reference.sha256.clone(),
    }
}

fn validate_file(
    root: &Path,
    verdict_path: &Path,
    verdict: &ProofImpactVerdict,
    artifact: &ProofImpactVerdictArtifact,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let relative = Path::new(&artifact.path);
    let expected_prefix = PathBuf::from(format!("verification/reviews/{}/", verdict.pcr_id));
    if !manifest_file::repository_relative(relative)
        || !relative.starts_with(&expected_prefix)
        || !valid_sha256(&artifact.sha256)
    {
        diagnostics.push(Diagnostic::at(
            verdict_path,
            format!("detached verdict contains malformed artifact `{}`", artifact.path),
            format!(
                "use a regular repository-relative file beneath `{}` and its lowercase raw-byte SHA-256",
                expected_prefix.display()
            ),
        ));
        return;
    }
    let Some(bytes) = manifest_file::read_bytes(root, relative, diagnostics) else { return };
    if bytes.is_empty() || sha256_hex(&bytes) != artifact.sha256 {
        diagnostics.push(Diagnostic::at(
            relative,
            "retained detached-verdict artifact is empty or differs from its content address",
            "restore the non-empty independently reviewed raw bytes and their exact SHA-256",
        ));
    }
}
