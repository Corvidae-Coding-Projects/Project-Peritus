#![allow(dead_code)]
#![allow(
    clippy::too_many_arguments,
    clippy::unused_self,
    reason = "fixture methods keep adversarial test setup explicit and uniform"
)]

use peritus_quality_policy::{
    AcceptanceEvidence, ApprovalObservation, ApprovalOutcome, ApprovalSubject, EvidenceObservation,
    FindingObservation, GateAttemptOrdinal, GateObservation, GateOutcome, ReviewCycleOrdinal,
    ReviewObservation, ReviewerIdentity, WaiverObservation,
};
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, ApprovalRequestId, EnvironmentId, FindingId, GateExecutionId,
    GateId, Generation, HarnessId, PolicyId, ProviderProfileId, ReviewCycleId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};

pub struct Fixture {
    pub acceptance_id: AcceptanceSpecId,
    pub gate_id: GateId,
    pub category_a: ReviewCategory,
    pub category_b: ReviewCategory,
    pub gate_evidence: EvidenceRequirementId,
    pub review_evidence: EvidenceRequirementId,
    pub human_evidence: EvidenceRequirementId,
    pub waiver_evidence: EvidenceRequirementId,
    pub approval_authority: ContentReference,
    pub waiver_authority: ContentReference,
}

#[derive(Clone, Copy)]
pub struct ContractOptions {
    pub quorum: u16,
    pub independence: ReviewerIndependence,
    pub blocking_severity: FindingSeverity,
    pub approval_policy: HumanApprovalPolicy,
    pub waiver_policy: WaiverPolicy,
    pub max_gate_attempts: u16,
    pub max_review_cycles: u16,
}

impl ContractOptions {
    pub const fn basic() -> Self {
        Self {
            quorum: 1,
            independence: ReviewerIndependence::new(true, true, false, false, false, false),
            blocking_severity: FindingSeverity::High,
            approval_policy: HumanApprovalPolicy::NotRequired,
            waiver_policy: WaiverPolicy::Forbidden,
            max_gate_attempts: 3,
            max_review_cycles: 4,
        }
    }
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            acceptance_id: AcceptanceSpecId::new(bytes(1)).expect("acceptance id"),
            gate_id: GateId::new(bytes(20)).expect("gate id"),
            category_a: ReviewCategory::new(digest(40)),
            category_b: ReviewCategory::new(digest(41)),
            gate_evidence: EvidenceRequirementId::new(digest(100)),
            review_evidence: EvidenceRequirementId::new(digest(101)),
            human_evidence: EvidenceRequirementId::new(digest(102)),
            waiver_evidence: EvidenceRequirementId::new(digest(103)),
            approval_authority: content(110),
            waiver_authority: content(111),
        }
    }

    pub fn revision(&self) -> RevisionTuple {
        self.revision_from([1, 2, 3, 1, 1, 4, 5])
    }

    pub fn revision_from(&self, parts: [u8; 7]) -> RevisionTuple {
        RevisionTuple::new(
            AcceptanceSpecId::new(bytes(parts[0])).expect("acceptance id"),
            HarnessId::new(bytes(parts[1])).expect("harness id"),
            WorkspaceId::new(bytes(parts[2])).expect("workspace id"),
            Generation::new(u64::from(parts[3])).expect("generation"),
            RevisionNumber::new(u64::from(parts[4])).expect("workspace revision"),
            PolicyId::new(bytes(parts[5])).expect("policy id"),
            ProviderProfileId::new(bytes(parts[6])).expect("provider profile id"),
        )
    }

    pub fn contract(&self, options: ContractOptions) -> AcceptanceContract {
        let plan = GateExecutionPlan::new(
            content(10),
            EnvironmentId::new(bytes(11)).expect("environment"),
            content(12),
            content(13),
            GateSuccessRule::ExitCodeZero,
            60_000,
            content(14),
            GateFreshnessScope::ExactRevisionTuple,
        )
        .expect("gate plan");
        let gate = GateDefinition::new(self.gate_id, plan, Vec::new(), vec![self.gate_evidence])
            .expect("gate");
        let graph = GateGraph::new(vec![gate]).expect("gate graph");
        let review = ReviewPolicy::new(
            vec![self.category_a, self.category_b],
            options.quorum,
            options.independence,
            options.blocking_severity,
        )
        .expect("review policy");
        let mut requirements = vec![
            EvidenceRequirement::new(
                self.gate_evidence,
                content(120),
                EvidenceSource::Gate(self.gate_id),
                ExportClassification::Internal,
            ),
            EvidenceRequirement::new(
                self.review_evidence,
                content(121),
                EvidenceSource::Review(self.category_a),
                ExportClassification::Internal,
            ),
        ];
        if options.approval_policy.is_required() {
            requirements.push(EvidenceRequirement::new(
                self.human_evidence,
                content(122),
                EvidenceSource::HumanApproval,
                ExportClassification::Restricted,
            ));
        }
        if options.waiver_policy.is_allowed() {
            requirements.push(EvidenceRequirement::new(
                self.waiver_evidence,
                content(123),
                EvidenceSource::WaiverAuthorization,
                ExportClassification::Restricted,
            ));
        }
        AcceptanceContract::new(
            self.acceptance_id,
            digest(6),
            ContractDocuments::new(
                content(16),
                content(17),
                content(18),
                content(19),
                content(21),
                content(22),
                content(23),
                content(24),
            ),
            vec![Requirement::new(RequirementId::new(digest(7)), content(8))],
            vec![Exclusion::new(content(9))],
            vec![Assumption::new(content(15))],
            graph,
            review,
            requirements,
            CompletionPolicy::new(options.max_gate_attempts, options.max_review_cycles)
                .expect("completion policy"),
            options.approval_policy,
            options.waiver_policy,
        )
        .expect("acceptance contract")
    }

    pub fn gate(&self, revision: RevisionTuple, outcome: GateOutcome) -> GateObservation {
        self.gate_at(revision, outcome, 1)
    }

    pub fn gate_at(
        &self,
        revision: RevisionTuple,
        outcome: GateOutcome,
        attempt: u16,
    ) -> GateObservation {
        GateObservation::new(
            GateExecutionId::new(bytes(30)).expect("gate execution"),
            self.gate_id,
            GateAttemptOrdinal::new(attempt).expect("gate attempt"),
            revision,
            outcome,
            digest(31),
        )
    }

    pub fn review(
        &self,
        revision: RevisionTuple,
        cycle: u8,
        actor: u8,
        categories: Vec<ReviewCategory>,
        findings: Vec<FindingObservation>,
        provenance: u8,
        independent_from_producer: bool,
    ) -> ReviewObservation {
        self.review_at_cycle(
            revision,
            cycle,
            u16::from(cycle.checked_sub(69).expect("fixture cycle starts at 70")),
            actor,
            categories,
            findings,
            provenance,
            independent_from_producer,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "review-cycle fixture keeps all facts explicit")]
    pub fn review_at_cycle(
        &self,
        revision: RevisionTuple,
        cycle: u8,
        cycle_ordinal: u16,
        actor: u8,
        categories: Vec<ReviewCategory>,
        findings: Vec<FindingObservation>,
        provenance: u8,
        independent_from_producer: bool,
    ) -> ReviewObservation {
        ReviewObservation::new(
            ReviewCycleId::new(bytes(cycle)).expect("review cycle"),
            ReviewCycleOrdinal::new(cycle_ordinal).expect("review cycle ordinal"),
            revision,
            ReviewerIdentity::new(
                ActorId::new(bytes(actor)).expect("reviewer actor"),
                digest(provenance),
                digest(provenance.wrapping_add(1)),
                digest(provenance.wrapping_add(2)),
                digest(provenance.wrapping_add(3)),
                digest(provenance.wrapping_add(4)),
                independent_from_producer,
            ),
            categories,
            findings,
            digest(cycle.wrapping_add(1)),
        )
        .expect("review")
    }

    pub fn required_evidence(
        &self,
        contract: &AcceptanceContract,
        revision: RevisionTuple,
    ) -> Vec<EvidenceObservation> {
        contract
            .evidence_requirements()
            .iter()
            .map(|requirement| {
                EvidenceObservation::new(requirement.id(), revision, requirement.id().digest())
            })
            .collect()
    }

    pub fn acceptance_approval(
        &self,
        revision: RevisionTuple,
        outcome: ApprovalOutcome,
    ) -> ApprovalObservation {
        ApprovalObservation::new(
            ApprovalRequestId::new(bytes(90)).expect("approval request"),
            revision,
            ApprovalSubject::Acceptance,
            ActorId::new(bytes(91)).expect("human actor"),
            self.approval_authority,
            outcome,
            digest(92),
        )
    }

    pub fn evidence_set(
        &self,
        contract: &AcceptanceContract,
        revision: RevisionTuple,
        reviews: Vec<ReviewObservation>,
        approvals: Vec<ApprovalObservation>,
        waivers: Vec<WaiverObservation>,
    ) -> AcceptanceEvidence {
        AcceptanceEvidence::new(
            vec![self.gate(revision, GateOutcome::Passed)],
            reviews,
            self.required_evidence(contract, revision),
            approvals,
            waivers,
        )
        .expect("canonical evidence")
    }
}

pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

pub const fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}

pub const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

pub fn finding_id(value: u8) -> FindingId {
    FindingId::new(bytes(value)).expect("finding id")
}
