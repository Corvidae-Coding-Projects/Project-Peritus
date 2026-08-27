//! Deterministic reduction of native campaign evidence into the V-class policy input.

use peritus_security_policy::{
    AcceptanceCriterion, ArtifactObservation, CriterionObservation, EvidenceArtifactKind,
    IndependentSecurityReview, InventoryKind, InventoryObservation, RequirementObservation,
    SecurityControlOutcome, SecurityEvidence, SecurityRequirement,
};
use peritus_types::Sha256Digest;

use crate::{
    CaseFailure, CaseOutcome, CaseReport, ProbeId, QualificationError, QualificationErrorCode,
    QualificationRecovery, QualificationRun, digest_bytes,
};

#[allow(
    clippy::redundant_pub_crate,
    reason = "the private report reducer consumes this sibling policy bridge"
)]
pub(super) fn build_policy_evidence(
    run: &QualificationRun,
    review: Option<IndependentSecurityReview>,
) -> Result<SecurityEvidence, QualificationError> {
    let requirements = build_requirements(run);
    let criteria = build_criteria(run);
    let inventories = build_inventories(run);
    let artifacts = build_artifacts(run, review.as_ref());
    SecurityEvidence::new(requirements, criteria, inventories, artifacts, review).map_err(|error| {
        QualificationError::new(
            QualificationErrorCode::PolicyEvidence,
            QualificationRecovery::Quarantine,
            "construct verified H0 policy evidence",
            format!(
                "canonical evidence rejected: {:?}/{:?}/{}",
                error.collection(),
                error.kind(),
                error.index()
            ),
        )
    })
}

fn build_requirements(run: &QualificationRun) -> Vec<RequirementObservation> {
    let candidate = run.candidate();
    SecurityRequirement::ALL
        .into_iter()
        .map(|requirement| {
            RequirementObservation::new(
                requirement,
                candidate,
                outcome_for(run, |case| case.spec().requirement() == requirement),
                digest_for(run, |case| case.spec().requirement() == requirement),
            )
        })
        .collect()
}

fn build_criteria(run: &QualificationRun) -> Vec<CriterionObservation> {
    let candidate = run.candidate();
    AcceptanceCriterion::ALL
        .into_iter()
        .map(|criterion| {
            CriterionObservation::new(
                criterion,
                candidate,
                outcome_for(run, |case| case.spec().criterion() == criterion),
                digest_for(run, |case| case.spec().criterion() == criterion),
            )
        })
        .collect()
}

fn build_inventories(run: &QualificationRun) -> Vec<InventoryObservation> {
    let candidate = run.candidate();
    [
        (InventoryKind::Threats, ProbeId::ThreatInventory),
        (InventoryKind::Controls, ProbeId::ControlInventory),
        (InventoryKind::UnsafeCode, ProbeId::UnsafeInventory),
        (InventoryKind::TrustedComputingBase, ProbeId::TcbInventory),
    ]
    .into_iter()
    .map(|(kind, probe)| {
        let case = case_by_id(run, probe);
        InventoryObservation::new(
            kind,
            candidate,
            case.is_some_and(|value| value.outcome() == CaseOutcome::Passed),
            case.and_then(CaseReport::evidence_digest)
                .unwrap_or_else(|| Sha256Digest::new([0; 32])),
        )
    })
    .collect()
}

fn build_artifacts(
    run: &QualificationRun,
    review: Option<&IndependentSecurityReview>,
) -> Vec<ArtifactObservation> {
    let candidate = run.candidate();
    let run_digest = digest_for(run, |_| true);
    let cleanup_digest = digest_cleanup(run);
    let resource_digest = digest_resources(run);
    let threat_control_digest = digest_for(run, |case| {
        matches!(case.spec().id(), ProbeId::ThreatInventory | ProbeId::ControlInventory)
    });
    let unsafe_tcb_digest = digest_for(run, |case| {
        matches!(case.spec().id(), ProbeId::UnsafeInventory | ProbeId::TcbInventory)
    });
    let supply_chain_digest = digest_for(run, |case| {
        matches!(
            case.spec().id(),
            ProbeId::DependencyReproducibility
                | ProbeId::ReleaseSignatureSbom
                | ProbeId::MigrationRecoveryDocumentation
        )
    });
    let mut artifacts = vec![
        ArtifactObservation::new(
            EvidenceArtifactKind::CampaignPlan,
            candidate,
            candidate.qualification_plan_digest(),
        ),
        ArtifactObservation::new(EvidenceArtifactKind::NativeProbeResults, candidate, run_digest),
        ArtifactObservation::new(
            EvidenceArtifactKind::ResourceAccounting,
            candidate,
            resource_digest,
        ),
        ArtifactObservation::new(EvidenceArtifactKind::CleanupLedger, candidate, cleanup_digest),
        ArtifactObservation::new(
            EvidenceArtifactKind::ThreatControlInventory,
            candidate,
            threat_control_digest,
        ),
        ArtifactObservation::new(
            EvidenceArtifactKind::UnsafeTcbInventory,
            candidate,
            unsafe_tcb_digest,
        ),
    ];
    if let Some(external) = review {
        artifacts.push(ArtifactObservation::new(
            EvidenceArtifactKind::ExternalReviewReport,
            candidate,
            external.report_digest(),
        ));
        artifacts.push(ArtifactObservation::new(
            EvidenceArtifactKind::FindingRegister,
            candidate,
            digest_findings(external),
        ));
    }
    artifacts.push(ArtifactObservation::new(
        EvidenceArtifactKind::SupplyChainAttestation,
        candidate,
        supply_chain_digest,
    ));
    artifacts.push(ArtifactObservation::new(
        EvidenceArtifactKind::ReleaseManifest,
        candidate,
        candidate.release_manifest_digest(),
    ));
    artifacts.sort_by_key(ArtifactObservation::kind);
    artifacts
}

fn outcome_for(
    run: &QualificationRun,
    select: impl Fn(&CaseReport) -> bool,
) -> SecurityControlOutcome {
    let selected = run.cases().iter().filter(|case| select(case)).collect::<Vec<_>>();
    if selected.is_empty() || selected.iter().any(|case| case.outcome() == CaseOutcome::NotExecuted)
    {
        SecurityControlOutcome::NotExecuted
    } else if selected.iter().any(|case| {
        case.failures().iter().any(|failure| matches!(failure, CaseFailure::Unsupported))
    }) {
        SecurityControlOutcome::Unsupported
    } else if selected.iter().all(|case| case.outcome() == CaseOutcome::Passed) {
        SecurityControlOutcome::Passed
    } else {
        SecurityControlOutcome::Failed
    }
}

fn case_by_id(run: &QualificationRun, probe: ProbeId) -> Option<&CaseReport> {
    run.cases().iter().find(|case| case.spec().id() == probe)
}

fn digest_for(run: &QualificationRun, select: impl Fn(&CaseReport) -> bool) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(4_096);
    bytes.extend_from_slice(b"peritus/h0/case-aggregate/v1\0");
    bytes.extend_from_slice(run.candidate().source_digest().as_bytes());
    for case in run.cases().iter().filter(|case| select(case)) {
        push_bytes(&mut bytes, case.spec().id().as_str().as_bytes());
        bytes.push(match case.outcome() {
            CaseOutcome::NotExecuted => 0,
            CaseOutcome::Failed => 1,
            CaseOutcome::Passed => 2,
        });
        if let Some(digest) = case.evidence_digest() {
            bytes.extend_from_slice(digest.as_bytes());
        } else {
            bytes.extend_from_slice(&[0; 32]);
        }
    }
    digest_bytes(&bytes)
}

fn digest_resources(run: &QualificationRun) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(2_048);
    bytes.extend_from_slice(b"peritus/h0/resource-accounting/v1\0");
    for case in run.cases() {
        push_bytes(&mut bytes, case.spec().id().as_str().as_bytes());
        if let Some(usage) = case.resource_usage() {
            bytes.extend_from_slice(&usage.elapsed_millis().to_be_bytes());
            bytes.extend_from_slice(&usage.process_count().to_be_bytes());
            bytes.extend_from_slice(&usage.peak_memory_bytes().to_be_bytes());
            bytes.extend_from_slice(&usage.output_bytes().to_be_bytes());
            bytes.extend_from_slice(&usage.artifact_count().to_be_bytes());
        } else {
            bytes.extend_from_slice(&[0; 32]);
        }
    }
    digest_bytes(&bytes)
}

fn digest_cleanup(run: &QualificationRun) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(2_048);
    bytes.extend_from_slice(b"peritus/h0/cleanup-ledger/v1\0");
    for case in run.cases() {
        push_bytes(&mut bytes, case.spec().id().as_str().as_bytes());
        if let Some(cleanup) = case.cleanup() {
            push_bytes(&mut bytes, cleanup.subject_id().as_bytes());
            bytes.extend_from_slice(cleanup.cleanup_digest().as_bytes());
            bytes.push(u8::from(cleanup.complete()));
            bytes.extend_from_slice(&cleanup.remaining_processes().to_be_bytes());
            bytes.extend_from_slice(&cleanup.remaining_paths().to_be_bytes());
            bytes.extend_from_slice(&cleanup.remaining_mounts().to_be_bytes());
            bytes.extend_from_slice(&cleanup.remaining_endpoints().to_be_bytes());
        } else {
            bytes.extend_from_slice(&[0; 33]);
        }
    }
    digest_bytes(&bytes)
}

fn digest_findings(review: &IndependentSecurityReview) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(512);
    bytes.extend_from_slice(b"peritus/h0/finding-register/v1\0");
    bytes.extend_from_slice(review.candidate().source_digest().as_bytes());
    for finding in review.findings() {
        bytes.extend_from_slice(finding.finding_id().as_bytes());
        bytes.push(finding.severity() as u8);
        match finding.lifecycle() {
            peritus_security_policy::FindingLifecycle::Open => bytes.push(0),
            peritus_security_policy::FindingLifecycle::AcceptedRisk { authority_digest } => {
                bytes.push(1);
                bytes.extend_from_slice(authority_digest.as_bytes());
            }
            peritus_security_policy::FindingLifecycle::Resolved {
                remediation_digest,
                retest_digest,
            } => {
                bytes.push(2);
                bytes.extend_from_slice(remediation_digest.as_bytes());
                bytes.extend_from_slice(retest_digest.as_bytes());
            }
        }
    }
    digest_bytes(&bytes)
}

fn push_bytes(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}
