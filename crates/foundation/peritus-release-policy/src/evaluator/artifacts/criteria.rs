//! Canonical reduction of artifact assessments into release criteria.

use crate::{AcceptanceCriterion, CriterionAssessment, EvidenceAssessment};

pub(in crate::evaluator) const fn assess_all_criteria(
    assessments: &[EvidenceAssessment; 44],
) -> [CriterionAssessment; 25] {
    [
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
    ]
}
pub(super) const fn assess_criterion(
    criterion: AcceptanceCriterion,
    assessments: &[EvidenceAssessment; 44],
) -> CriterionAssessment {
    let satisfied = match criterion {
        AcceptanceCriterion::CleanTierOneSuite => {
            assessments[0].is_satisfied() && assessments[1].is_satisfied()
        }
        AcceptanceCriterion::VerifiedWorkspaceBuild => {
            assessments[2].is_satisfied() && assessments[3].is_satisfied()
        }
        AcceptanceCriterion::ProofObligationInventory => assessments[4].is_satisfied(),
        AcceptanceCriterion::TrustedConstructAudit => assessments[5].is_satisfied(),
        AcceptanceCriterion::PrivilegedConstruction => assessments[6].is_satisfied(),
        AcceptanceCriterion::IllegalLifecycleEdges => assessments[7].is_satisfied(),
        AcceptanceCriterion::CrashRecovery => assessments[8].is_satisfied(),
        AcceptanceCriterion::DeterministicReplay => assessments[9].is_satisfied(),
        AcceptanceCriterion::MaliciousRepository => assessments[10].is_satisfied(),
        AcceptanceCriterion::NativeSandboxSecurity => {
            assessments[11].is_satisfied()
                && assessments[12].is_satisfied()
                && assessments[13].is_satisfied()
                && assessments[14].is_satisfied()
        }
        AcceptanceCriterion::RoleIsolation => assessments[15].is_satisfied(),
        AcceptanceCriterion::EvidenceInvalidation => assessments[16].is_satisfied(),
        AcceptanceCriterion::ExhaustionFailsClosed => assessments[17].is_satisfied(),
        AcceptanceCriterion::DaemonLifecycleRecovery => assessments[18].is_satisfied(),
        AcceptanceCriterion::ProviderContracts => assessments[19].is_satisfied(),
        AcceptanceCriterion::MigrationAndExport => {
            assessments[20].is_satisfied() && assessments[21].is_satisfied()
        }
        AcceptanceCriterion::EvolutionIsolation => assessments[22].is_satisfied(),
        AcceptanceCriterion::PromotionAndRollback => {
            assessments[23].is_satisfied() && assessments[24].is_satisfied()
        }
        AcceptanceCriterion::ObservabilityAndRedaction => {
            assessments[25].is_satisfied() && assessments[26].is_satisfied()
        }
        AcceptanceCriterion::LoadAndSoak => {
            assessments[27].is_satisfied() && assessments[28].is_satisfied()
        }
        AcceptanceCriterion::PublicSurfaceDocumentation => {
            assessments[29].is_satisfied() && assessments[30].is_satisfied()
        }
        AcceptanceCriterion::ArchitectureIntegrity => assessments[31].is_satisfied(),
        AcceptanceCriterion::RepresentativeCampaign => assessments[32].is_satisfied(),
        AcceptanceCriterion::ReleaseArtifacts => {
            assessments[33].is_satisfied()
                && assessments[34].is_satisfied()
                && assessments[35].is_satisfied()
                && assessments[36].is_satisfied()
                && assessments[37].is_satisfied()
                && assessments[38].is_satisfied()
                && assessments[39].is_satisfied()
        }
        AcceptanceCriterion::NoReleaseDebt => {
            assessments[40].is_satisfied()
                && assessments[41].is_satisfied()
                && assessments[42].is_satisfied()
                && assessments[43].is_satisfied()
        }
    };

    CriterionAssessment::new(criterion, satisfied)
}
