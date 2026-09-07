//! Synthetic checked D2 input; it never represents an independent review or approval.

use peritus_context::ContextPlanId;
use peritus_quality_policy::{ReviewCycleOrdinal, ReviewerIdentity};
use peritus_review::{
    Confidence, Finding, FindingSource, ReviewAssignment, ReviewBinding, ReviewLimits,
    ReviewSubmission,
};
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    ActorId, EnvironmentId, FindingId, GateId, ReviewCycleId, RevisionTuple, Sha256Digest,
};

use crate::{SubjectError, identity::IdentitySource};

pub struct EventFixture {
    pub binding: ReviewBinding,
    pub limits: ReviewLimits,
    category: ReviewCategory,
}

impl EventFixture {
    pub fn new(revision: RevisionTuple) -> Result<Self, SubjectError> {
        let limits = ReviewLimits::new(
            1,
            1,
            1,
            1,
            1,
            1,
            1,
            1,
            1,
            1,
            4_096,
            262_144,
            1_048_576,
            ReviewLimits::MAX_PAYLOAD_BYTES,
            ReviewLimits::MAX_STATE_BYTES,
        )?;
        let gate = GateId::new([1; 16]).map_err(SubjectError::Identifier)?;
        let evidence = EvidenceRequirementId::new(digest(2));
        let category = ReviewCategory::new(digest(3));
        let plan = GateExecutionPlan::new(
            content(4),
            EnvironmentId::new([5; 16]).map_err(SubjectError::Identifier)?,
            content(6),
            content(7),
            GateSuccessRule::ExitCodeZero,
            10_000,
            content(8),
            GateFreshnessScope::ExactRevisionTuple,
        )
        .map_err(invalid)?;
        let graph = GateGraph::new(vec![
            GateDefinition::new(gate, plan, Vec::new(), vec![evidence]).map_err(invalid)?,
        ])
        .map_err(invalid)?;
        let contract = AcceptanceContract::new(
            revision.acceptance_spec_id(),
            digest(9),
            ContractDocuments::new(
                content(10),
                content(11),
                content(12),
                content(13),
                content(14),
                content(15),
                content(16),
                content(17),
            ),
            vec![Requirement::new(RequirementId::new(digest(18)), content(19))],
            vec![Exclusion::new(content(20))],
            vec![Assumption::new(content(21))],
            graph,
            ReviewPolicy::new(
                vec![category],
                1,
                ReviewerIndependence::new(true, true, true, true, true, true),
                FindingSeverity::High,
            )
            .map_err(invalid)?,
            vec![
                EvidenceRequirement::new(
                    evidence,
                    content(22),
                    EvidenceSource::Gate(gate),
                    ExportClassification::Internal,
                ),
                EvidenceRequirement::new(
                    EvidenceRequirementId::new(digest(23)),
                    content(24),
                    EvidenceSource::Review(category),
                    ExportClassification::Internal,
                ),
            ],
            CompletionPolicy::new(1, 1).map_err(invalid)?,
            HumanApprovalPolicy::NotRequired,
            WaiverPolicy::Forbidden,
        )
        .map_err(invalid)?;
        let binding = ReviewBinding::from_contract(
            &contract,
            revision,
            digest(25),
            digest(26),
            vec![ActorId::new([27; 16]).map_err(SubjectError::Identifier)?],
            vec![digest(28)],
            limits,
        )?;
        Ok(Self { binding, limits, category })
    }

    pub fn assignment(
        &self,
        identities: &mut IdentitySource,
    ) -> Result<ReviewAssignment, SubjectError> {
        let reviewer = ReviewerIdentity::new(
            identities.next(ActorId::new)?,
            digest(40),
            digest(41),
            digest(42),
            digest(43),
            digest(44),
            true,
        );
        Ok(ReviewAssignment::new(
            identities.next(ReviewCycleId::new)?,
            ReviewCycleOrdinal::new(1).expect("one is a valid one-based ordinal"),
            &self.binding,
            reviewer,
            vec![self.category],
            ContextPlanId::new(digest(43)),
            true,
            self.limits,
        )?)
    }

    pub fn submission(
        &self,
        assignment: &ReviewAssignment,
        id: FindingId,
        text: String,
    ) -> Result<ReviewSubmission, SubjectError> {
        let finding = Finding::new(
            id,
            FindingSource::new(assignment.cycle_id(), assignment.reviewer().actor_id()),
            self.category,
            FindingSeverity::Low,
            FindingSeverity::High,
            Confidence::new(10_000)?,
            Vec::new(),
            Vec::new(),
            vec![peritus_evidence::EvidenceId::new([45; 16]).map_err(SubjectError::Identifier)?],
            text,
            "synthetic H3 event fixture".to_owned(),
            "exact durable bytes".to_owned(),
            "no real review or approval asserted".to_owned(),
            self.binding.revision(),
            self.limits,
        )?;
        Ok(ReviewSubmission::new(
            assignment.cycle_id(),
            self.binding.revision(),
            vec![self.category],
            vec![finding],
            FindingSeverity::High,
            self.limits,
        )?)
    }
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
const fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}
const fn invalid(error: peritus_spec::SpecError) -> SubjectError {
    SubjectError::EventFixture(error)
}
