#![allow(dead_code, reason = "each integration-test crate uses a different fixture subset")]

use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, EnvironmentId, GateId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

pub const fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}

pub const fn requirement_id(value: u8) -> RequirementId {
    RequirementId::new(digest(value))
}

pub const fn evidence_id(value: u8) -> EvidenceRequirementId {
    EvidenceRequirementId::new(digest(value))
}

pub const fn category(value: u8) -> ReviewCategory {
    ReviewCategory::new(digest(value))
}

pub fn gate_id(value: u8) -> GateId {
    GateId::new([value; 16]).expect("nonzero gate id")
}

pub fn environment(value: u8) -> EnvironmentId {
    EnvironmentId::new([value; 16]).expect("nonzero environment id")
}

pub fn plan(value: u8) -> GateExecutionPlan {
    GateExecutionPlan::new(
        content(value),
        environment(value),
        content(value.wrapping_add(1)),
        content(value.wrapping_add(2)),
        GateSuccessRule::ExitCodeZero,
        10_000,
        content(value.wrapping_add(3)),
        GateFreshnessScope::ExactRevisionTuple,
    )
    .expect("valid plan")
}

pub fn gate(
    value: u8,
    dependencies: Vec<GateId>,
    required_evidence: Vec<EvidenceRequirementId>,
) -> GateDefinition {
    GateDefinition::new(gate_id(value), plan(value), dependencies, required_evidence)
        .expect("valid gate")
}

pub fn review_policy() -> ReviewPolicy {
    ReviewPolicy::new(
        vec![category(1), category(2)],
        2,
        ReviewerIndependence::new(true, true, true, false, false, true),
        FindingSeverity::High,
    )
    .expect("valid review policy")
}

pub const fn evidence(value: u8, source: EvidenceSource) -> EvidenceRequirement {
    EvidenceRequirement::new(
        evidence_id(value),
        content(value),
        source,
        ExportClassification::Internal,
    )
}

pub fn graph() -> GateGraph {
    GateGraph::new(vec![
        gate(1, Vec::new(), vec![evidence_id(1)]),
        gate(2, vec![gate_id(1)], vec![evidence_id(2)]),
    ])
    .expect("valid graph")
}

pub const fn documents() -> ContractDocuments {
    ContractDocuments::new(
        content(70),
        content(71),
        content(72),
        content(73),
        content(74),
        content(75),
        content(76),
        content(77),
    )
}

pub fn contract() -> AcceptanceContract {
    AcceptanceContract::new(
        acceptance_id(1),
        digest(90),
        documents(),
        vec![Requirement::new(requirement_id(1), content(21))],
        vec![Exclusion::new(content(31))],
        vec![Assumption::new(content(41))],
        graph(),
        review_policy(),
        vec![
            evidence(1, EvidenceSource::Gate(gate_id(1))),
            evidence(2, EvidenceSource::Gate(gate_id(2))),
            evidence(3, EvidenceSource::Review(category(1))),
            evidence(4, EvidenceSource::Review(category(2))),
            evidence(5, EvidenceSource::HumanApproval),
            evidence(6, EvidenceSource::WaiverAuthorization),
        ],
        CompletionPolicy::new(2, 4).expect("valid completion policy"),
        HumanApprovalPolicy::Required(content(60)),
        WaiverPolicy::Allowed { authority: content(61), evidence: evidence_id(6) },
    )
    .expect("valid contract")
}

pub fn acceptance_id(value: u8) -> AcceptanceSpecId {
    AcceptanceSpecId::new([value; 16]).expect("nonzero acceptance id")
}

pub fn revision(specification: u8, workspace_revision: u64) -> RevisionTuple {
    RevisionTuple::new(
        acceptance_id(specification),
        HarnessId::new([2; 16]).expect("harness"),
        WorkspaceId::new([3; 16]).expect("workspace"),
        Generation::new(1).expect("generation"),
        RevisionNumber::new(workspace_revision).expect("revision"),
        PolicyId::new([4; 16]).expect("policy"),
        ProviderProfileId::new([5; 16]).expect("provider"),
    )
}
