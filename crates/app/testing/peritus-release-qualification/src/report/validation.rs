//! Private validation and deterministic policy-reduction helpers.

use peritus_release_artifacts::Sha256Digest;

use crate::{
    DeterministicReleasePolicy, EvidenceDisposition, EvidenceKind, EvidenceReference,
    PolicyCriterionInput, PolicyDecision, QualificationError, ReleasePolicyInput,
    SignedEvidenceRecord,
};

use super::{AcReference, Blocker, QualificationInputs, RequiredInput};

#[derive(Clone, Copy)]
pub(super) struct PolicyDigests {
    pub(super) artifact_inventory: Option<Sha256Digest>,
    pub(super) evidence_manifest: Option<Sha256Digest>,
    pub(super) criterion_map: Option<Sha256Digest>,
    pub(super) final_audit: Option<Sha256Digest>,
}

pub(super) fn collect_records<'a>(
    inputs: &'a QualificationInputs,
    blockers: &mut Vec<Blocker>,
) -> Vec<&'a SignedEvidenceRecord> {
    let mut records: Vec<&SignedEvidenceRecord> = inputs.evidence.iter().collect();
    if let Some(run) = &inputs.collection_run {
        records.extend(run.records());
        if run.binding() != &inputs.binding {
            blockers.push(Blocker::BindingMismatch);
        }
        if !run.is_complete() {
            blockers.push(Blocker::CollectionIncomplete);
        }
    } else {
        blockers.push(Blocker::MissingInput(RequiredInput::CollectionRun));
    }
    records
}

pub(super) fn validate_required_records(
    inputs: &QualificationInputs,
    records: &[&SignedEvidenceRecord],
    blockers: &mut Vec<Blocker>,
) {
    if records.iter().any(|record| record.binding() != &inputs.binding) {
        blockers.push(Blocker::BindingMismatch);
    }
    if records.iter().any(|record| record.evidence_reference().kind() == EvidenceKind::FinalAudit) {
        blockers.push(Blocker::DuplicateSignedEvidence(EvidenceKind::FinalAudit));
    }
    for kind in EvidenceKind::required_signed_inputs() {
        match records.iter().filter(|record| record.evidence_reference().kind() == kind).count() {
            0 => blockers.push(Blocker::MissingSignedEvidence(kind)),
            1 => {}
            _ => blockers.push(Blocker::DuplicateSignedEvidence(kind)),
        }
        if records.iter().any(|record| {
            record.evidence_reference().kind() == kind
                && record.evidence_reference().disposition() != EvidenceDisposition::Satisfied
        }) {
            blockers.push(Blocker::UnsatisfiedSignedEvidence(kind));
        }
    }
}

pub(super) fn validate_artifact_inventory(
    inputs: &QualificationInputs,
    records: &[&SignedEvidenceRecord],
    blockers: &mut Vec<Blocker>,
) -> Result<Option<Sha256Digest>, QualificationError> {
    let Some(inventory) = &inputs.artifact_inventory else {
        blockers.push(Blocker::MissingInput(RequiredInput::ArtifactInventory));
        return Ok(None);
    };
    if inventory.binding() != &inputs.binding {
        blockers.push(Blocker::BindingMismatch);
    }
    let digest = inventory.digest().map_err(|error| {
        QualificationError::new(
            crate::QualificationErrorCode::Integrity,
            "digest artifact inventory for H4 report",
            error.to_string(),
        )
    })?;
    if !record_digest_matches(records, EvidenceKind::ArtifactInventory, digest) {
        blockers.push(Blocker::ArtifactInventoryDigestMismatch);
    }
    Ok(Some(digest))
}

pub(super) fn validate_reproducibility(
    inputs: &QualificationInputs,
    records: &[&SignedEvidenceRecord],
    blockers: &mut Vec<Blocker>,
) -> Result<(), QualificationError> {
    let Some(comparison) = &inputs.reproducibility else {
        blockers.push(Blocker::MissingInput(RequiredInput::ReproducibilityComparison));
        return Ok(());
    };
    if comparison.binding() != &inputs.binding {
        blockers.push(Blocker::BindingMismatch);
    }
    if !comparison.is_reproducible() {
        blockers.push(Blocker::ArtifactsNotReproducible);
    }
    let digest = comparison.digest().map_err(|error| {
        QualificationError::new(
            crate::QualificationErrorCode::Integrity,
            "digest reproducibility comparison for H4 report",
            error.to_string(),
        )
    })?;
    if !record_digest_matches(records, EvidenceKind::Reproducibility, digest) {
        blockers.push(Blocker::ReproducibilityDigestMismatch);
    }
    Ok(())
}

pub(super) fn available_references<'a>(
    inputs: &'a QualificationInputs,
    records: &[&'a SignedEvidenceRecord],
) -> Vec<&'a EvidenceReference> {
    let mut available =
        records.iter().map(|record| record.evidence_reference()).collect::<Vec<_>>();
    if let Some(audit) = &inputs.final_audit {
        available.push(audit.evidence_reference());
    }
    available
}

pub(super) fn validate_criterion_map(
    inputs: &QualificationInputs,
    available: &[&EvidenceReference],
    blockers: &mut Vec<Blocker>,
) -> Result<Option<Sha256Digest>, QualificationError> {
    let Some(map) = &inputs.criterion_map else {
        blockers.push(Blocker::MissingInput(RequiredInput::CriterionEvidenceMap));
        return Ok(None);
    };
    for mapping in map.mappings() {
        if mapping.evidence().iter().any(|reference| !available.contains(&reference)) {
            blockers.push(Blocker::CriterionEvidenceUnavailable(AcReference::from_criterion(
                mapping.criterion(),
            )));
        }
    }
    let digest = map.digest()?;
    if !available.iter().any(|reference| {
        reference.kind() == EvidenceKind::CriterionMap && reference.payload_digest() == digest
    }) {
        blockers.push(Blocker::CriterionMapDigestMismatch);
    }
    Ok(Some(digest))
}

pub(super) fn validate_manifest(
    inputs: &QualificationInputs,
    available: &[&EvidenceReference],
    blockers: &mut Vec<Blocker>,
) -> Result<Option<Sha256Digest>, QualificationError> {
    let Some(manifest) = &inputs.evidence_manifest else {
        blockers.push(Blocker::MissingInput(RequiredInput::EvidenceManifest));
        return Ok(None);
    };
    if manifest.binding() != &inputs.binding {
        blockers.push(Blocker::BindingMismatch);
    }
    if !manifest.is_complete() {
        blockers.push(Blocker::ManifestIncomplete);
    }
    for reference in available {
        if !manifest.contains_reference(reference) {
            blockers.push(Blocker::ManifestReferenceMissing(reference.kind()));
        }
    }
    Ok(Some(manifest.digest()?))
}

pub(super) fn validate_audit(
    inputs: &QualificationInputs,
    blockers: &mut Vec<Blocker>,
) -> Result<Option<Sha256Digest>, QualificationError> {
    let Some(audit) = &inputs.final_audit else {
        blockers.push(Blocker::MissingInput(RequiredInput::FinalAudit));
        return Ok(None);
    };
    if audit.binding() != &inputs.binding {
        blockers.push(Blocker::BindingMismatch);
    }
    if !audit.is_independent() {
        blockers.push(Blocker::AuditNotIndependent);
    }
    if !audit.blocking_findings_closed() {
        blockers.push(Blocker::AuditBlockingFindingOpen);
    }
    if let Some(manifest) = &inputs.evidence_manifest
        && audit.reviewed_evidence_set_digest() != manifest.pre_audit_digest()?
    {
        blockers.push(Blocker::AuditSubjectMismatch);
    }
    Ok(Some(audit.digest()))
}

pub(super) fn evaluate_policy<P: DeterministicReleasePolicy>(
    inputs: &QualificationInputs,
    policy: &P,
    digests: PolicyDigests,
    blockers: &mut Vec<Blocker>,
) -> Option<PolicyDecision> {
    if !blockers.is_empty() {
        return None;
    }
    let (
        Some(artifact_inventory),
        Some(evidence_manifest),
        Some(criterion_map),
        Some(final_audit),
        Some(mappings),
    ) = (
        digests.artifact_inventory,
        digests.evidence_manifest,
        digests.criterion_map,
        digests.final_audit,
        inputs.criterion_map.as_ref(),
    )
    else {
        return None;
    };
    let criteria = mappings
        .mappings()
        .iter()
        .map(|mapping| PolicyCriterionInput::new(mapping.criterion(), mapping.evidence().to_vec()))
        .collect();
    let policy_input = ReleasePolicyInput::new(
        inputs.binding.clone(),
        artifact_inventory,
        evidence_manifest,
        criterion_map,
        final_audit,
        criteria,
    );
    let decision = policy.evaluate(&policy_input);
    match &decision {
        PolicyDecision::Ready => {}
        PolicyDecision::NotReady { .. } => blockers.push(Blocker::PolicyRejected),
        PolicyDecision::Unavailable { .. } => blockers.push(Blocker::PolicyUnavailable),
    }
    Some(decision)
}

fn record_digest_matches(
    records: &[&SignedEvidenceRecord],
    kind: EvidenceKind,
    expected: Sha256Digest,
) -> bool {
    records.iter().any(|record| {
        record.evidence_reference().kind() == kind
            && record.evidence_reference().payload_digest() == expected
    })
}
