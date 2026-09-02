//! Minimal checked acceptance contract for one internally authorized product command.

use peritus_codec::sha256;
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{AcceptanceSpecId, EnvironmentId, GateId, RunId, Sha256Digest};

pub(super) fn command_contract(run_id: RunId, ordinal: u64) -> Result<AcceptanceContract, String> {
    let acceptance = AcceptanceSpecId::new(id(run_id, ordinal, "acceptance"))
        .map_err(|error| format!("construct command acceptance identity: {error:?}"))?;
    let environment = EnvironmentId::new(id(run_id, ordinal, "gate-environment"))
        .map_err(|error| format!("construct command gate environment: {error:?}"))?;
    let gate_id = GateId::new(id(run_id, ordinal, "gate"))
        .map_err(|error| format!("construct command gate identity: {error:?}"))?;
    let gate_evidence = EvidenceRequirementId::new(digest(run_id, ordinal, "gate-evidence"));
    let review_evidence = EvidenceRequirementId::new(digest(run_id, ordinal, "review-evidence"));
    let category = ReviewCategory::new(digest(run_id, ordinal, "review-category"));
    let gate = GateDefinition::new(
        gate_id,
        GateExecutionPlan::new(
            content(run_id, ordinal, "gate-action"),
            environment,
            content(run_id, ordinal, "gate-inputs"),
            content(run_id, ordinal, "gate-parser"),
            GateSuccessRule::ExitCodeZero,
            1,
            content(run_id, ordinal, "gate-resources"),
            GateFreshnessScope::ExactRevisionTuple,
        )
        .map_err(|error| format!("construct command gate plan: {error:?}"))?,
        Vec::new(),
        vec![gate_evidence],
    )
    .map_err(|error| format!("construct command gate: {error:?}"))?;
    let documents = ContractDocuments::new(
        content(run_id, ordinal, "objective"),
        content(run_id, ordinal, "scope"),
        content(run_id, ordinal, "constraints"),
        content(run_id, ordinal, "inputs"),
        content(run_id, ordinal, "outputs"),
        content(run_id, ordinal, "acceptance"),
        content(run_id, ordinal, "review"),
        content(run_id, ordinal, "terminal"),
    );
    let mut evidence_requirements = vec![
        EvidenceRequirement::new(
            gate_evidence,
            content(run_id, ordinal, "gate-evidence-text"),
            EvidenceSource::Gate(gate_id),
            ExportClassification::Internal,
        ),
        EvidenceRequirement::new(
            review_evidence,
            content(run_id, ordinal, "review-evidence-text"),
            EvidenceSource::Review(category),
            ExportClassification::Internal,
        ),
    ];
    evidence_requirements.sort_by_key(EvidenceRequirement::id);
    AcceptanceContract::new(
        acceptance,
        digest(run_id, ordinal, "contract"),
        documents,
        vec![Requirement::new(
            RequirementId::new(digest(run_id, ordinal, "requirement")),
            content(run_id, ordinal, "requirement-text"),
        )],
        vec![Exclusion::new(content(run_id, ordinal, "exclusion"))],
        vec![Assumption::new(content(run_id, ordinal, "assumption"))],
        GateGraph::new(vec![gate])
            .map_err(|error| format!("construct command gate graph: {error:?}"))?,
        ReviewPolicy::new(
            vec![category],
            1,
            ReviewerIndependence::new(true, true, true, true, true, true),
            FindingSeverity::High,
        )
        .map_err(|error| format!("construct command review policy: {error:?}"))?,
        evidence_requirements,
        CompletionPolicy::new(1, 1)
            .map_err(|error| format!("construct command completion policy: {error:?}"))?,
        HumanApprovalPolicy::NotRequired,
        WaiverPolicy::Forbidden,
    )
    .map_err(|error| format!("construct command acceptance contract: {error:?}"))
}

pub(super) fn digest(run_id: RunId, ordinal: u64, label: &str) -> Sha256Digest {
    let mut bytes = b"peritus/product-command/v1\0".to_vec();
    bytes.extend_from_slice(run_id.as_bytes());
    bytes.extend_from_slice(&ordinal.to_be_bytes());
    bytes.extend_from_slice(label.as_bytes());
    sha256(&bytes)
}

pub(super) fn id(run_id: RunId, ordinal: u64, label: &str) -> [u8; 16] {
    let digest = digest(run_id, ordinal, label);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[0] |= 1;
    bytes
}

fn content(run_id: RunId, ordinal: u64, label: &str) -> ContentReference {
    ContentReference::new(digest(run_id, ordinal, label))
}
