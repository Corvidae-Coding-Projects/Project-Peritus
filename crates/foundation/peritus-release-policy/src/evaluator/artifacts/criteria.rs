//! Canonical reduction of artifact assessments into release criteria.

use crate::{AcceptanceCriterion, CriterionAssessment, EvidenceAssessment};
#[cfg(verus_only)]
use crate::{
    decision::{spec_criteria_complete, spec_evidence_complete}, ReleaseCandidate, ReleaseEvidence,
};
use vstd::prelude::*;

verus! {

pub open spec fn all_criteria_satisfied(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    super::model::criterion_satisfied(evidence, AcceptanceCriterion::CleanTierOneSuite, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::VerifiedWorkspaceBuild, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::ProofObligationInventory, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::TrustedConstructAudit, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::PrivilegedConstruction, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::IllegalLifecycleEdges, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::CrashRecovery, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::DeterministicReplay, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::MaliciousRepository, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::NativeSandboxSecurity, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::RoleIsolation, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::EvidenceInvalidation, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::ExhaustionFailsClosed, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::DaemonLifecycleRecovery, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::ProviderContracts, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::MigrationAndExport, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::EvolutionIsolation, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::PromotionAndRollback, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::ObservabilityAndRedaction, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::LoadAndSoak, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::PublicSurfaceDocumentation, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::ArchitectureIntegrity, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::RepresentativeCampaign, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::ReleaseArtifacts, candidate, evaluated_at)
        && super::model::criterion_satisfied(evidence, AcceptanceCriterion::NoReleaseDebt, candidate, evaluated_at)
}

/// Exact criterion identity and mapped evidence conjunction stored in one assessment.
pub open spec fn assessment_matches_inputs(
    assessment: &CriterionAssessment,
    criterion: AcceptanceCriterion,
    evidence: Seq<EvidenceAssessment>,
) -> bool {
    assessment.spec_criterion() == criterion
        && assessment.spec_is_satisfied() == criterion_assessment_satisfied(evidence, criterion)
}

/// Every criterion slot retains its stable identity and exact mapped evidence conjunction.
pub open spec fn assessments_match_inputs(
    criteria: Seq<CriterionAssessment>,
    evidence: Seq<EvidenceAssessment>,
) -> bool {
    criteria.len() == 25 && evidence.len() == 44
        && assessment_matches_inputs(&criteria[0], AcceptanceCriterion::CleanTierOneSuite, evidence)
        && assessment_matches_inputs(&criteria[1], AcceptanceCriterion::VerifiedWorkspaceBuild, evidence)
        && assessment_matches_inputs(&criteria[2], AcceptanceCriterion::ProofObligationInventory, evidence)
        && assessment_matches_inputs(&criteria[3], AcceptanceCriterion::TrustedConstructAudit, evidence)
        && assessment_matches_inputs(&criteria[4], AcceptanceCriterion::PrivilegedConstruction, evidence)
        && assessment_matches_inputs(&criteria[5], AcceptanceCriterion::IllegalLifecycleEdges, evidence)
        && assessment_matches_inputs(&criteria[6], AcceptanceCriterion::CrashRecovery, evidence)
        && assessment_matches_inputs(&criteria[7], AcceptanceCriterion::DeterministicReplay, evidence)
        && assessment_matches_inputs(&criteria[8], AcceptanceCriterion::MaliciousRepository, evidence)
        && assessment_matches_inputs(&criteria[9], AcceptanceCriterion::NativeSandboxSecurity, evidence)
        && assessment_matches_inputs(&criteria[10], AcceptanceCriterion::RoleIsolation, evidence)
        && assessment_matches_inputs(&criteria[11], AcceptanceCriterion::EvidenceInvalidation, evidence)
        && assessment_matches_inputs(&criteria[12], AcceptanceCriterion::ExhaustionFailsClosed, evidence)
        && assessment_matches_inputs(&criteria[13], AcceptanceCriterion::DaemonLifecycleRecovery, evidence)
        && assessment_matches_inputs(&criteria[14], AcceptanceCriterion::ProviderContracts, evidence)
        && assessment_matches_inputs(&criteria[15], AcceptanceCriterion::MigrationAndExport, evidence)
        && assessment_matches_inputs(&criteria[16], AcceptanceCriterion::EvolutionIsolation, evidence)
        && assessment_matches_inputs(&criteria[17], AcceptanceCriterion::PromotionAndRollback, evidence)
        && assessment_matches_inputs(&criteria[18], AcceptanceCriterion::ObservabilityAndRedaction, evidence)
        && assessment_matches_inputs(&criteria[19], AcceptanceCriterion::LoadAndSoak, evidence)
        && assessment_matches_inputs(&criteria[20], AcceptanceCriterion::PublicSurfaceDocumentation, evidence)
        && assessment_matches_inputs(&criteria[21], AcceptanceCriterion::ArchitectureIntegrity, evidence)
        && assessment_matches_inputs(&criteria[22], AcceptanceCriterion::RepresentativeCampaign, evidence)
        && assessment_matches_inputs(&criteria[23], AcceptanceCriterion::ReleaseArtifacts, evidence)
        && assessment_matches_inputs(&criteria[24], AcceptanceCriterion::NoReleaseDebt, evidence)
}


pub(in crate::evaluator) const fn assess_all_criteria(
    assessments: &[EvidenceAssessment; 44],
) -> (criteria: [CriterionAssessment; 25])
    ensures
        assessments_match_inputs(criteria@, assessments@),
        spec_criteria_complete(&criteria) == spec_evidence_complete(assessments),
{
    let criteria = [
        assess_criterion(AcceptanceCriterion::CleanTierOneSuite, assessments),
        assess_criterion(AcceptanceCriterion::VerifiedWorkspaceBuild, assessments),
        assess_criterion(AcceptanceCriterion::ProofObligationInventory, assessments),
        assess_criterion(AcceptanceCriterion::TrustedConstructAudit, assessments),
        assess_criterion(AcceptanceCriterion::PrivilegedConstruction, assessments),
        assess_criterion(AcceptanceCriterion::IllegalLifecycleEdges, assessments),
        assess_criterion(AcceptanceCriterion::CrashRecovery, assessments),
        assess_criterion(AcceptanceCriterion::DeterministicReplay, assessments),
        assess_criterion(AcceptanceCriterion::MaliciousRepository, assessments),
        assess_criterion(AcceptanceCriterion::NativeSandboxSecurity, assessments),
        assess_criterion(AcceptanceCriterion::RoleIsolation, assessments),
        assess_criterion(AcceptanceCriterion::EvidenceInvalidation, assessments),
        assess_criterion(AcceptanceCriterion::ExhaustionFailsClosed, assessments),
        assess_criterion(AcceptanceCriterion::DaemonLifecycleRecovery, assessments),
        assess_criterion(AcceptanceCriterion::ProviderContracts, assessments),
        assess_criterion(AcceptanceCriterion::MigrationAndExport, assessments),
        assess_criterion(AcceptanceCriterion::EvolutionIsolation, assessments),
        assess_criterion(AcceptanceCriterion::PromotionAndRollback, assessments),
        assess_criterion(AcceptanceCriterion::ObservabilityAndRedaction, assessments),
        assess_criterion(AcceptanceCriterion::LoadAndSoak, assessments),
        assess_criterion(AcceptanceCriterion::PublicSurfaceDocumentation, assessments),
        assess_criterion(AcceptanceCriterion::ArchitectureIntegrity, assessments),
        assess_criterion(AcceptanceCriterion::RepresentativeCampaign, assessments),
        assess_criterion(AcceptanceCriterion::ReleaseArtifacts, assessments),
        assess_criterion(AcceptanceCriterion::NoReleaseDebt, assessments),
    ];
    proof {
        reveal(spec_criteria_complete);
        reveal(spec_evidence_complete);
        reveal(criterion_assessment_satisfied);
        reveal(assessments_match_inputs);
        reveal(assessment_matches_inputs);
    }
    criteria
}

pub(in crate::evaluator) proof fn criteria_cover_all_requirements(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
)
    ensures all_criteria_satisfied(evidence, candidate, evaluated_at)
        == super::all_requirements_satisfied(evidence, candidate, evaluated_at),
{
    reveal(all_criteria_satisfied);
    reveal(super::all_requirements_satisfied);
    reveal(super::model::criterion_satisfied);
}

pub open spec fn criterion_assessment_satisfied(
    assessments: Seq<EvidenceAssessment>,
    criterion: AcceptanceCriterion,
) -> bool {
    match criterion {
        AcceptanceCriterion::CleanTierOneSuite => assessments[0].spec_is_satisfied() && assessments[1].spec_is_satisfied(),
        AcceptanceCriterion::VerifiedWorkspaceBuild => assessments[2].spec_is_satisfied() && assessments[3].spec_is_satisfied(),
        AcceptanceCriterion::ProofObligationInventory => assessments[4].spec_is_satisfied(),
        AcceptanceCriterion::TrustedConstructAudit => assessments[5].spec_is_satisfied(),
        AcceptanceCriterion::PrivilegedConstruction => assessments[6].spec_is_satisfied(),
        AcceptanceCriterion::IllegalLifecycleEdges => assessments[7].spec_is_satisfied(),
        AcceptanceCriterion::CrashRecovery => assessments[8].spec_is_satisfied(),
        AcceptanceCriterion::DeterministicReplay => assessments[9].spec_is_satisfied(),
        AcceptanceCriterion::MaliciousRepository => assessments[10].spec_is_satisfied(),
        AcceptanceCriterion::NativeSandboxSecurity => assessments[11].spec_is_satisfied() && assessments[12].spec_is_satisfied() && assessments[13].spec_is_satisfied() && assessments[14].spec_is_satisfied(),
        AcceptanceCriterion::RoleIsolation => assessments[15].spec_is_satisfied(),
        AcceptanceCriterion::EvidenceInvalidation => assessments[16].spec_is_satisfied(),
        AcceptanceCriterion::ExhaustionFailsClosed => assessments[17].spec_is_satisfied(),
        AcceptanceCriterion::DaemonLifecycleRecovery => assessments[18].spec_is_satisfied(),
        AcceptanceCriterion::ProviderContracts => assessments[19].spec_is_satisfied(),
        AcceptanceCriterion::MigrationAndExport => assessments[20].spec_is_satisfied() && assessments[21].spec_is_satisfied(),
        AcceptanceCriterion::EvolutionIsolation => assessments[22].spec_is_satisfied(),
        AcceptanceCriterion::PromotionAndRollback => assessments[23].spec_is_satisfied() && assessments[24].spec_is_satisfied(),
        AcceptanceCriterion::ObservabilityAndRedaction => assessments[25].spec_is_satisfied() && assessments[26].spec_is_satisfied(),
        AcceptanceCriterion::LoadAndSoak => assessments[27].spec_is_satisfied() && assessments[28].spec_is_satisfied(),
        AcceptanceCriterion::PublicSurfaceDocumentation => assessments[29].spec_is_satisfied() && assessments[30].spec_is_satisfied(),
        AcceptanceCriterion::ArchitectureIntegrity => assessments[31].spec_is_satisfied(),
        AcceptanceCriterion::RepresentativeCampaign => assessments[32].spec_is_satisfied(),
        AcceptanceCriterion::ReleaseArtifacts => assessments[33].spec_is_satisfied() && assessments[34].spec_is_satisfied() && assessments[35].spec_is_satisfied() && assessments[36].spec_is_satisfied() && assessments[37].spec_is_satisfied() && assessments[38].spec_is_satisfied() && assessments[39].spec_is_satisfied(),
        AcceptanceCriterion::NoReleaseDebt => assessments[40].spec_is_satisfied() && assessments[41].spec_is_satisfied() && assessments[42].spec_is_satisfied() && assessments[43].spec_is_satisfied(),
    }
}

pub(super) const fn assess_criterion(
    criterion: AcceptanceCriterion,
    assessments: &[EvidenceAssessment; 44],
) -> (assessment: CriterionAssessment)
    ensures
        assessment_matches_inputs(&assessment, criterion, assessments@),
        assessment.spec_is_satisfied()
            == criterion_assessment_satisfied(assessments@, criterion),
{
    let satisfied = match criterion {
        AcceptanceCriterion::CleanTierOneSuite => assessments[0].is_satisfied() && assessments[1].is_satisfied(),
        AcceptanceCriterion::VerifiedWorkspaceBuild => assessments[2].is_satisfied() && assessments[3].is_satisfied(),
        AcceptanceCriterion::ProofObligationInventory => assessments[4].is_satisfied(),
        AcceptanceCriterion::TrustedConstructAudit => assessments[5].is_satisfied(),
        AcceptanceCriterion::PrivilegedConstruction => assessments[6].is_satisfied(),
        AcceptanceCriterion::IllegalLifecycleEdges => assessments[7].is_satisfied(),
        AcceptanceCriterion::CrashRecovery => assessments[8].is_satisfied(),
        AcceptanceCriterion::DeterministicReplay => assessments[9].is_satisfied(),
        AcceptanceCriterion::MaliciousRepository => assessments[10].is_satisfied(),
        AcceptanceCriterion::NativeSandboxSecurity => assessments[11].is_satisfied() && assessments[12].is_satisfied() && assessments[13].is_satisfied() && assessments[14].is_satisfied(),
        AcceptanceCriterion::RoleIsolation => assessments[15].is_satisfied(),
        AcceptanceCriterion::EvidenceInvalidation => assessments[16].is_satisfied(),
        AcceptanceCriterion::ExhaustionFailsClosed => assessments[17].is_satisfied(),
        AcceptanceCriterion::DaemonLifecycleRecovery => assessments[18].is_satisfied(),
        AcceptanceCriterion::ProviderContracts => assessments[19].is_satisfied(),
        AcceptanceCriterion::MigrationAndExport => assessments[20].is_satisfied() && assessments[21].is_satisfied(),
        AcceptanceCriterion::EvolutionIsolation => assessments[22].is_satisfied(),
        AcceptanceCriterion::PromotionAndRollback => assessments[23].is_satisfied() && assessments[24].is_satisfied(),
        AcceptanceCriterion::ObservabilityAndRedaction => assessments[25].is_satisfied() && assessments[26].is_satisfied(),
        AcceptanceCriterion::LoadAndSoak => assessments[27].is_satisfied() && assessments[28].is_satisfied(),
        AcceptanceCriterion::PublicSurfaceDocumentation => assessments[29].is_satisfied() && assessments[30].is_satisfied(),
        AcceptanceCriterion::ArchitectureIntegrity => assessments[31].is_satisfied(),
        AcceptanceCriterion::RepresentativeCampaign => assessments[32].is_satisfied(),
        AcceptanceCriterion::ReleaseArtifacts => assessments[33].is_satisfied() && assessments[34].is_satisfied() && assessments[35].is_satisfied() && assessments[36].is_satisfied() && assessments[37].is_satisfied() && assessments[38].is_satisfied() && assessments[39].is_satisfied(),
        AcceptanceCriterion::NoReleaseDebt => assessments[40].is_satisfied() && assessments[41].is_satisfied() && assessments[42].is_satisfied() && assessments[43].is_satisfied(),
    };
    let assessment = CriterionAssessment::new(criterion, satisfied);
    proof {
        reveal(assessment_matches_inputs);
    }
    assessment
}


} // verus!
