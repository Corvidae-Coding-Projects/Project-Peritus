//! Exact input admission predicates for release evidence.

#[cfg(verus_only)]
use crate::{EvidenceRequirement, ReleaseCandidate, ReleaseEvidence};
use vstd::prelude::*;

verus! {

/// At least one supplied observation satisfies one exact artifact requirement.
pub open spec fn evidence_requirement_admitted(
    evidence: &ReleaseEvidence,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    exists |index: int| 0 <= index < evidence.spec_observations().len()
        && #[trigger] evidence.spec_observations()[index].spec_contributes_to(
            requirement,
            candidate,
            evaluated_at,
        )
}

/// Every closed H4 artifact requirement has an exact supplied contributor.
pub open spec fn required_evidence_admitted(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    evidence_requirement_admitted(evidence, EvidenceRequirement::GateA, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::FoundationQualityMatrix, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::FoundationVerusVerify, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::FoundationVerusBuild, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::ProofInventory, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::TrustBoundaryAudit, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::PrivilegedConstructionConformance, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::IllegalLifecycleEdgeMatrix, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::CrashInjectionCampaign, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::DeterministicReplayCorpus, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::MaliciousRepositorySuite, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::LinuxNativeQualification, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::MacOsNativeQualification, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::WindowsNativeQualification, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::SandboxEscapeReview, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::RoleIsolationMatrix, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::EvidenceInvalidationMatrix, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::ExhaustionFailClosedMatrix, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::DaemonRecoveryCampaign, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::ProviderContractMatrix, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::MigrationCorpus, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::EvidenceExport, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::EvolutionRedTeam, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::PromotionGateMatrix, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::AtomicRollback, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::ObservabilityCitations, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::SecretRedaction, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::LoadSlo, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::EightHourSoak, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::PublicReferenceDocumentation, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::CommandProtocolEndToEnd, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::ArchitectureAudit, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::RepresentativeCampaign, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::ReproducibleArtifacts, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::ArtifactSignatures, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::Sbom, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::Provenance, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::LicenseNotices, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::MigrationRecoveryDocumentation, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::CompletedSecurityReview, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::TestQuarantineAudit, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::ReleaseFindingAudit, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::UnsafeInventory, candidate, evaluated_at)
        && evidence_requirement_admitted(evidence, EvidenceRequirement::PlaceholderAudit, candidate, evaluated_at)
}

} // verus!
