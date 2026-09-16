//! Exact finite reduction model for supplied release-artifact observations.

use crate::{
    AcceptanceCriterion, EvidenceObservation, EvidenceRequirement, ReleaseCandidate, ReleaseEvidence,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

pub struct ArtifactState {
    pub contributing_count: u16,
    pub stale_count: u16,
    pub mismatched_count: u16,
    pub wrong_source_count: u16,
    pub unreviewed_count: u16,
    pub unsigned_count: u16,
    pub conflicting: bool,
    pub first_digest: Option<Sha256Digest>,
    pub aggregate_digest: Seq<u8>,
}

pub open spec fn initial() -> ArtifactState {
    ArtifactState {
        contributing_count: 0,
        stale_count: 0,
        mismatched_count: 0,
        wrong_source_count: 0,
        unreviewed_count: 0,
        unsigned_count: 0,
        conflicting: false,
        first_digest: None,
        aggregate_digest: Seq::new(32, |index: int| 0u8),
    }
}

pub open spec fn step(
    state: ArtifactState,
    observation: EvidenceObservation,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> ArtifactState {
    if observation.spec_requirement() != requirement {
        state
    } else if observation.spec_binding().spec_is_mismatched(candidate) {
        ArtifactState {
            mismatched_count: super::super::saturated_increment(state.mismatched_count),
            ..state
        }
    } else if observation.spec_binding().spec_is_stale_at(candidate, evaluated_at) {
        ArtifactState {
            stale_count: super::super::saturated_increment(state.stale_count),
            ..state
        }
    } else if observation.spec_source_kind() != requirement.spec_source_kind() {
        ArtifactState {
            wrong_source_count: super::super::saturated_increment(state.wrong_source_count),
            ..state
        }
    } else {
        let unreviewed_count = if observation.spec_reviewed() {
            state.unreviewed_count
        } else {
            super::super::saturated_increment(state.unreviewed_count)
        };
        let unsigned_count = if observation.spec_signed() {
            state.unsigned_count
        } else {
            super::super::saturated_increment(state.unsigned_count)
        };
        if observation.spec_reviewed() && observation.spec_signed() {
            let digest_conflict = match state.first_digest {
                Some(previous) => !crate::candidate::digest_matches(
                    previous,
                    observation.spec_artifact_digest(),
                ),
                None => false,
            };
            let first_digest = match state.first_digest {
                Some(previous) => Some(previous),
                None => Some(observation.spec_artifact_digest()),
            };
            ArtifactState {
                contributing_count: super::super::saturated_increment(state.contributing_count),
                unreviewed_count,
                unsigned_count,
                conflicting: state.conflicting || digest_conflict,
                first_digest,
                aggregate_digest: super::super::xor_digest_bytes(
                    state.aggregate_digest,
                    observation.spec_artifact_digest(),
                ),
                ..state
            }
        } else {
            ArtifactState { unreviewed_count, unsigned_count, ..state }
        }
    }
}

pub open spec fn through(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> ArtifactState
    decreases end,
{
    if end == 0 {
        initial()
    } else {
        step(
            through(values, requirement, candidate, evaluated_at, (end - 1) as nat),
            values[(end - 1) as int],
            requirement,
            candidate,
            evaluated_at,
        )
    }
}

pub open spec fn state_satisfied(state: ArtifactState) -> bool {
    state.contributing_count > 0
        && state.stale_count == 0
        && state.mismatched_count == 0
        && state.wrong_source_count == 0
        && state.unreviewed_count == 0
        && state.unsigned_count == 0
        && !state.conflicting
}

pub open spec fn corresponds(
    state: ArtifactState,
    contributing_count: u16,
    stale_count: u16,
    mismatched_count: u16,
    wrong_source_count: u16,
    unreviewed_count: u16,
    unsigned_count: u16,
    conflicting: bool,
    first_digest: Option<Sha256Digest>,
    aggregate_digest: Seq<u8>,
) -> bool {
    state.contributing_count == contributing_count
        && state.stale_count == stale_count
        && state.mismatched_count == mismatched_count
        && state.wrong_source_count == wrong_source_count
        && state.unreviewed_count == unreviewed_count
        && state.unsigned_count == unsigned_count
        && state.conflicting == conflicting
        && state.first_digest == first_digest
        && state.aggregate_digest == aggregate_digest
}

pub open spec fn requirement_satisfied(
    evidence: &ReleaseEvidence,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    state_satisfied(through(
        evidence.spec_observations(),
        requirement,
        candidate,
        evaluated_at,
        evidence.spec_observations().len() as nat,
    ))
}

pub open spec fn criterion_satisfied(
    evidence: &ReleaseEvidence,
    criterion: AcceptanceCriterion,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    match criterion {
        AcceptanceCriterion::CleanTierOneSuite => super::requirement_satisfied(evidence, EvidenceRequirement::GateA, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::FoundationQualityMatrix, candidate, evaluated_at),
        AcceptanceCriterion::VerifiedWorkspaceBuild => super::requirement_satisfied(evidence, EvidenceRequirement::FoundationVerusVerify, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::FoundationVerusBuild, candidate, evaluated_at),
        AcceptanceCriterion::ProofObligationInventory => super::requirement_satisfied(evidence, EvidenceRequirement::ProofInventory, candidate, evaluated_at),
        AcceptanceCriterion::TrustedConstructAudit => super::requirement_satisfied(evidence, EvidenceRequirement::TrustBoundaryAudit, candidate, evaluated_at),
        AcceptanceCriterion::PrivilegedConstruction => super::requirement_satisfied(evidence, EvidenceRequirement::PrivilegedConstructionConformance, candidate, evaluated_at),
        AcceptanceCriterion::IllegalLifecycleEdges => super::requirement_satisfied(evidence, EvidenceRequirement::IllegalLifecycleEdgeMatrix, candidate, evaluated_at),
        AcceptanceCriterion::CrashRecovery => super::requirement_satisfied(evidence, EvidenceRequirement::CrashInjectionCampaign, candidate, evaluated_at),
        AcceptanceCriterion::DeterministicReplay => super::requirement_satisfied(evidence, EvidenceRequirement::DeterministicReplayCorpus, candidate, evaluated_at),
        AcceptanceCriterion::MaliciousRepository => super::requirement_satisfied(evidence, EvidenceRequirement::MaliciousRepositorySuite, candidate, evaluated_at),
        AcceptanceCriterion::NativeSandboxSecurity => super::requirement_satisfied(evidence, EvidenceRequirement::LinuxNativeQualification, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::MacOsNativeQualification, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::WindowsNativeQualification, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::SandboxEscapeReview, candidate, evaluated_at),
        AcceptanceCriterion::RoleIsolation => super::requirement_satisfied(evidence, EvidenceRequirement::RoleIsolationMatrix, candidate, evaluated_at),
        AcceptanceCriterion::EvidenceInvalidation => super::requirement_satisfied(evidence, EvidenceRequirement::EvidenceInvalidationMatrix, candidate, evaluated_at),
        AcceptanceCriterion::ExhaustionFailsClosed => super::requirement_satisfied(evidence, EvidenceRequirement::ExhaustionFailClosedMatrix, candidate, evaluated_at),
        AcceptanceCriterion::DaemonLifecycleRecovery => super::requirement_satisfied(evidence, EvidenceRequirement::DaemonRecoveryCampaign, candidate, evaluated_at),
        AcceptanceCriterion::ProviderContracts => super::requirement_satisfied(evidence, EvidenceRequirement::ProviderContractMatrix, candidate, evaluated_at),
        AcceptanceCriterion::MigrationAndExport => super::requirement_satisfied(evidence, EvidenceRequirement::MigrationCorpus, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::EvidenceExport, candidate, evaluated_at),
        AcceptanceCriterion::EvolutionIsolation => super::requirement_satisfied(evidence, EvidenceRequirement::EvolutionRedTeam, candidate, evaluated_at),
        AcceptanceCriterion::PromotionAndRollback => super::requirement_satisfied(evidence, EvidenceRequirement::PromotionGateMatrix, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::AtomicRollback, candidate, evaluated_at),
        AcceptanceCriterion::ObservabilityAndRedaction => super::requirement_satisfied(evidence, EvidenceRequirement::ObservabilityCitations, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::SecretRedaction, candidate, evaluated_at),
        AcceptanceCriterion::LoadAndSoak => super::requirement_satisfied(evidence, EvidenceRequirement::LoadSlo, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::EightHourSoak, candidate, evaluated_at),
        AcceptanceCriterion::PublicSurfaceDocumentation => super::requirement_satisfied(evidence, EvidenceRequirement::PublicReferenceDocumentation, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::CommandProtocolEndToEnd, candidate, evaluated_at),
        AcceptanceCriterion::ArchitectureIntegrity => super::requirement_satisfied(evidence, EvidenceRequirement::ArchitectureAudit, candidate, evaluated_at),
        AcceptanceCriterion::RepresentativeCampaign => super::requirement_satisfied(evidence, EvidenceRequirement::RepresentativeCampaign, candidate, evaluated_at),
        AcceptanceCriterion::ReleaseArtifacts => super::requirement_satisfied(evidence, EvidenceRequirement::ReproducibleArtifacts, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::ArtifactSignatures, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::Sbom, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::Provenance, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::LicenseNotices, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::MigrationRecoveryDocumentation, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::CompletedSecurityReview, candidate, evaluated_at),
        AcceptanceCriterion::NoReleaseDebt => super::requirement_satisfied(evidence, EvidenceRequirement::TestQuarantineAudit, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::ReleaseFindingAudit, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::UnsafeInventory, candidate, evaluated_at)
            && super::requirement_satisfied(evidence, EvidenceRequirement::PlaceholderAudit, candidate, evaluated_at),
    }
}

} // verus!
