//! Exact reduction of supplied release-artifact observations and their criteria.

#[cfg(verus_only)]
pub mod model;
#[cfg(verus_only)]
mod declarative;
mod assessment;
mod criteria;

#[cfg(verus_only)]
pub use self::criteria::all_criteria_satisfied;
#[cfg(verus_only)]
pub(super) use self::criteria::{
    assessments_match_inputs as criteria_assessments_match_inputs,
    criteria_cover_all_requirements,
};
pub(super) use self::criteria::assess_all_criteria;

use crate::{EvidenceAssessment, EvidenceRequirement, ReleaseCandidate, ReleaseEvidence};
use vstd::prelude::*;

verus! {

pub open spec fn requirement_satisfied(
    evidence: &ReleaseEvidence,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    declarative::requirement_inputs_ready(
        evidence.spec_observations(), requirement, candidate, evaluated_at,
    )
}

/// Direct all-observation characterization of one required artifact class.
pub open spec fn requirement_inputs_ready(
    evidence: &ReleaseEvidence,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    requirement_satisfied(evidence, requirement, candidate, evaluated_at)
}

pub open spec fn all_requirements_satisfied(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    requirement_satisfied(evidence, EvidenceRequirement::GateA, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::FoundationQualityMatrix, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::FoundationVerusVerify, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::FoundationVerusBuild, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::ProofInventory, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::TrustBoundaryAudit, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::PrivilegedConstructionConformance, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::IllegalLifecycleEdgeMatrix, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::CrashInjectionCampaign, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::DeterministicReplayCorpus, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::MaliciousRepositorySuite, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::LinuxNativeQualification, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::MacOsNativeQualification, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::WindowsNativeQualification, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::SandboxEscapeReview, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::RoleIsolationMatrix, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::EvidenceInvalidationMatrix, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::ExhaustionFailClosedMatrix, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::DaemonRecoveryCampaign, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::ProviderContractMatrix, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::MigrationCorpus, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::EvidenceExport, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::EvolutionRedTeam, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::PromotionGateMatrix, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::AtomicRollback, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::ObservabilityCitations, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::SecretRedaction, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::LoadSlo, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::EightHourSoak, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::PublicReferenceDocumentation, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::CommandProtocolEndToEnd, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::ArchitectureAudit, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::RepresentativeCampaign, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::ReproducibleArtifacts, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::ArtifactSignatures, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::Sbom, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::Provenance, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::LicenseNotices, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::MigrationRecoveryDocumentation, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::CompletedSecurityReview, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::TestQuarantineAudit, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::ReleaseFindingAudit, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::UnsafeInventory, candidate, evaluated_at)
        && requirement_satisfied(evidence, EvidenceRequirement::PlaceholderAudit, candidate, evaluated_at)
}

/// Every evidence assessment retains its canonical requirement and exact finite reduction fields.
pub open spec fn assessments_match_inputs(
    assessments: Seq<EvidenceAssessment>,
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    assessments.len() == 44
        && assessment::assessment_matches_reduction(&assessments[0], evidence, EvidenceRequirement::GateA, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[1], evidence, EvidenceRequirement::FoundationQualityMatrix, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[2], evidence, EvidenceRequirement::FoundationVerusVerify, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[3], evidence, EvidenceRequirement::FoundationVerusBuild, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[4], evidence, EvidenceRequirement::ProofInventory, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[5], evidence, EvidenceRequirement::TrustBoundaryAudit, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[6], evidence, EvidenceRequirement::PrivilegedConstructionConformance, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[7], evidence, EvidenceRequirement::IllegalLifecycleEdgeMatrix, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[8], evidence, EvidenceRequirement::CrashInjectionCampaign, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[9], evidence, EvidenceRequirement::DeterministicReplayCorpus, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[10], evidence, EvidenceRequirement::MaliciousRepositorySuite, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[11], evidence, EvidenceRequirement::LinuxNativeQualification, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[12], evidence, EvidenceRequirement::MacOsNativeQualification, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[13], evidence, EvidenceRequirement::WindowsNativeQualification, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[14], evidence, EvidenceRequirement::SandboxEscapeReview, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[15], evidence, EvidenceRequirement::RoleIsolationMatrix, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[16], evidence, EvidenceRequirement::EvidenceInvalidationMatrix, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[17], evidence, EvidenceRequirement::ExhaustionFailClosedMatrix, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[18], evidence, EvidenceRequirement::DaemonRecoveryCampaign, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[19], evidence, EvidenceRequirement::ProviderContractMatrix, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[20], evidence, EvidenceRequirement::MigrationCorpus, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[21], evidence, EvidenceRequirement::EvidenceExport, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[22], evidence, EvidenceRequirement::EvolutionRedTeam, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[23], evidence, EvidenceRequirement::PromotionGateMatrix, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[24], evidence, EvidenceRequirement::AtomicRollback, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[25], evidence, EvidenceRequirement::ObservabilityCitations, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[26], evidence, EvidenceRequirement::SecretRedaction, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[27], evidence, EvidenceRequirement::LoadSlo, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[28], evidence, EvidenceRequirement::EightHourSoak, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[29], evidence, EvidenceRequirement::PublicReferenceDocumentation, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[30], evidence, EvidenceRequirement::CommandProtocolEndToEnd, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[31], evidence, EvidenceRequirement::ArchitectureAudit, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[32], evidence, EvidenceRequirement::RepresentativeCampaign, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[33], evidence, EvidenceRequirement::ReproducibleArtifacts, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[34], evidence, EvidenceRequirement::ArtifactSignatures, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[35], evidence, EvidenceRequirement::Sbom, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[36], evidence, EvidenceRequirement::Provenance, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[37], evidence, EvidenceRequirement::LicenseNotices, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[38], evidence, EvidenceRequirement::MigrationRecoveryDocumentation, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[39], evidence, EvidenceRequirement::CompletedSecurityReview, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[40], evidence, EvidenceRequirement::TestQuarantineAudit, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[41], evidence, EvidenceRequirement::ReleaseFindingAudit, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[42], evidence, EvidenceRequirement::UnsafeInventory, candidate, evaluated_at)
        && assessment::assessment_matches_reduction(&assessments[43], evidence, EvidenceRequirement::PlaceholderAudit, candidate, evaluated_at)
}


#[allow(clippy::too_many_lines, reason = "the closed evidence order is intentionally auditable")]
pub(super) fn assess_all_evidence(
    candidate: &ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> (assessments: [EvidenceAssessment; 44])
    ensures
        assessments_match_inputs(assessments@, evidence, *candidate, evaluated_at),
        crate::decision::spec_evidence_complete(&assessments)
            == all_requirements_satisfied(evidence, *candidate, evaluated_at),
        super::diagnostics::evidence_clear(&assessments)
            == all_requirements_satisfied(evidence, *candidate, evaluated_at),
        all_requirements_satisfied(evidence, *candidate, evaluated_at)
            ==> super::required_evidence_admitted(evidence, *candidate, evaluated_at),
{
    let assessments = [
        assessment::assess_evidence(EvidenceRequirement::GateA, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::FoundationQualityMatrix, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::FoundationVerusVerify, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::FoundationVerusBuild, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::ProofInventory, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::TrustBoundaryAudit, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::PrivilegedConstructionConformance, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::IllegalLifecycleEdgeMatrix, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::CrashInjectionCampaign, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::DeterministicReplayCorpus, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::MaliciousRepositorySuite, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::LinuxNativeQualification, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::MacOsNativeQualification, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::WindowsNativeQualification, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::SandboxEscapeReview, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::RoleIsolationMatrix, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::EvidenceInvalidationMatrix, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::ExhaustionFailClosedMatrix, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::DaemonRecoveryCampaign, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::ProviderContractMatrix, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::MigrationCorpus, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::EvidenceExport, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::EvolutionRedTeam, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::PromotionGateMatrix, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::AtomicRollback, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::ObservabilityCitations, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::SecretRedaction, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::LoadSlo, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::EightHourSoak, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::PublicReferenceDocumentation, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::CommandProtocolEndToEnd, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::ArchitectureAudit, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::RepresentativeCampaign, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::ReproducibleArtifacts, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::ArtifactSignatures, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::Sbom, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::Provenance, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::LicenseNotices, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::MigrationRecoveryDocumentation, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::CompletedSecurityReview, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::TestQuarantineAudit, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::ReleaseFindingAudit, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::UnsafeInventory, *candidate, evaluated_at, evidence),
        assessment::assess_evidence(EvidenceRequirement::PlaceholderAudit, *candidate, evaluated_at, evidence),
    ];
    proof {
        reveal(crate::decision::spec_evidence_complete);
        reveal(all_requirements_satisfied);
        reveal(assessments_match_inputs);
        reveal(super::required_evidence_admitted);
        reveal(super::diagnostics::evidence_clear);
        reveal_with_fuel(super::diagnostics::evidence_clear_through, 45);
    }
    assessments
}


} // verus!
