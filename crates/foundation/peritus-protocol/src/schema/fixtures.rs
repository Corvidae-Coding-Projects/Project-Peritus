//! Deterministic version-one compatibility frames.

use crate::{
    AcceptanceContractDto, ActionIntentDto, BudgetAmountsDto, CommandEnvelopeDto,
    GateDefinitionDto, KernelCommandDto, PolicyAmendmentDto, RestrictionLayerDto, ReviewPolicyDto,
};
use peritus_budget::BudgetAmounts;
use peritus_codec::{CodecError, CodecLimits, encode_message};
use peritus_kernel::{CommandEnvelope, KernelCommand};
use peritus_policy::{ActorRole, OperationClass, PolicyTier, RestrictionLayer};
use peritus_spec::{
    Assumption, CompletionPolicy, ContentReference, ContractDocuments, EvidenceRequirement,
    EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification, FindingSeverity,
    GateExecutionPlan, GateFreshnessScope, GateSuccessRule, HumanApprovalPolicy, Requirement,
    RequirementId, ReviewCategory, ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, CapabilityName, CommandId, EnvironmentId, EventId, GateId,
    Generation, HarnessId, PolicyId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple,
    Sha256Digest, WorkspaceId,
};

/// One generated repository-relative binary compatibility fixture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedBinaryArtifact {
    /// Path below the selected output root.
    pub path: &'static str,
    /// Complete canonical frame bytes.
    pub content: Vec<u8>,
}

/// Builds representative golden frames across every major B3 domain surface.
///
/// # Errors
///
/// Returns a codec error if a canonical fixture cannot be encoded within the
/// production limits.
pub fn generated_binary_artifacts() -> Result<Vec<GeneratedBinaryArtifact>, CodecError> {
    let limits = CodecLimits::PRODUCTION;
    let pause = encode_message(&KernelCommandDto::from(KernelCommand::PauseSession), limits)?;
    let envelope = encode_message(&CommandEnvelopeDto::from(fixture_envelope()), limits)?;
    let budget = encode_message(
        &BudgetAmountsDto::from(BudgetAmounts::from_units(100, 200, 300, 4, 5)),
        limits,
    )?;
    let action = encode_message(&fixture_action(), limits)?;
    let amendment = encode_message(&fixture_amendment(limits)?, limits)?;
    let contract = encode_message(&fixture_contract(limits)?, limits)?;
    Ok(vec![
        binary("protocol/fixtures/v1/kernel-command-pause-session.bin", pause),
        binary("protocol/fixtures/v1/command-envelope.bin", envelope),
        binary("protocol/fixtures/v1/budget-amounts.bin", budget),
        binary("protocol/fixtures/v1/action-intent.bin", action),
        binary("protocol/fixtures/v1/policy-amendment.bin", amendment),
        binary("protocol/fixtures/v1/acceptance-contract.bin", contract),
    ])
}

const fn binary(path: &'static str, content: Vec<u8>) -> GeneratedBinaryArtifact {
    GeneratedBinaryArtifact { path, content }
}

fn fixture_envelope() -> CommandEnvelope {
    CommandEnvelope::new(
        CommandId::new([20; 16]).expect("fixture id"),
        EventId::new([21; 16]).expect("fixture id"),
        Some(EventId::new([22; 16]).expect("fixture id")),
        revision(),
    )
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        acceptance_id(),
        HarnessId::new([2; 16]).expect("fixture id"),
        WorkspaceId::new([3; 16]).expect("fixture id"),
        Generation::new(4).expect("one based"),
        RevisionNumber::new(5).expect("one based"),
        PolicyId::new([6; 16]).expect("fixture id"),
        ProviderProfileId::new([7; 16]).expect("fixture id"),
    )
}

fn fixture_action() -> ActionIntentDto {
    ActionIntentDto {
        action_id: ActionId::new([30; 16]).expect("fixture id"),
        actor_id: ActorId::new([31; 16]).expect("fixture id"),
        role: ActorRole::Writer,
        environment_id: EnvironmentId::new([32; 16]).expect("fixture id"),
        resource_id: ResourceId::new([33; 16]).expect("fixture id"),
        capability_name: CapabilityName::new("inspect".to_owned()).expect("fixture name"),
        operation_class: OperationClass::Inspection,
        media_type: "application/json".to_owned(),
        payload: br#"{"path":"src/lib.rs"}"#.to_vec(),
    }
}

fn fixture_amendment(limits: CodecLimits) -> Result<PolicyAmendmentDto, CodecError> {
    let replacement = RestrictionLayer::new(PolicyTier::Run, Vec::new()).expect("fixture layer");
    PolicyAmendmentDto::new(
        PolicyId::new([40; 16]).expect("fixture id"),
        PolicyId::new([41; 16]).expect("fixture id"),
        PolicyTier::Run,
        RestrictionLayerDto::from(&replacement),
        limits,
    )
}

fn fixture_contract(limits: CodecLimits) -> Result<AcceptanceContractDto, CodecError> {
    let evidence = [1, 2, 3, 4, 5, 6].map(evidence_id);
    AcceptanceContractDto::new(
        acceptance_id(),
        ContractDocuments::new(
            content(70),
            content(71),
            content(72),
            content(73),
            content(74),
            content(75),
            content(76),
            content(77),
        ),
        vec![Requirement::new(RequirementId::new(digest(1)), content(21))],
        vec![Exclusion::new(content(31))],
        vec![Assumption::new(content(41))],
        vec![gate(1, Vec::new()), gate(2, vec![gate_id(1)])],
        ReviewPolicyDto {
            required_categories: vec![category(1), category(2)],
            reviewer_quorum: 2,
            independence: ReviewerIndependence::new(true, true, true, false, false, true),
            blocking_severity: FindingSeverity::High,
        },
        vec![
            evidence_requirement(evidence[0], EvidenceSource::Gate(gate_id(1))),
            evidence_requirement(evidence[1], EvidenceSource::Gate(gate_id(2))),
            evidence_requirement(evidence[2], EvidenceSource::Review(category(1))),
            evidence_requirement(evidence[3], EvidenceSource::Review(category(2))),
            evidence_requirement(evidence[4], EvidenceSource::HumanApproval),
            evidence_requirement(evidence[5], EvidenceSource::WaiverAuthorization),
        ],
        CompletionPolicy::new(2, 4).expect("fixture completion"),
        HumanApprovalPolicy::Required(content(60)),
        WaiverPolicy::Allowed { authority: content(61), evidence: evidence[5] },
        limits,
    )
}

fn gate(value: u8, dependencies: Vec<GateId>) -> GateDefinitionDto {
    GateDefinitionDto {
        id: gate_id(value),
        plan: GateExecutionPlan::new(
            content(value),
            EnvironmentId::new([value; 16]).expect("fixture environment"),
            content(value + 1),
            content(value + 2),
            GateSuccessRule::ExitCodeZero,
            10_000,
            content(value + 3),
            GateFreshnessScope::ExactRevisionTuple,
        )
        .expect("fixture plan"),
        dependencies,
        required_evidence: vec![evidence_id(value)],
    }
}

const fn evidence_requirement(
    id: EvidenceRequirementId,
    source: EvidenceSource,
) -> EvidenceRequirement {
    EvidenceRequirement::new(
        id,
        ContentReference::new(id.digest()),
        source,
        ExportClassification::Internal,
    )
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
const fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}
const fn evidence_id(value: u8) -> EvidenceRequirementId {
    EvidenceRequirementId::new(digest(value))
}
const fn category(value: u8) -> ReviewCategory {
    ReviewCategory::new(digest(value))
}
fn acceptance_id() -> AcceptanceSpecId {
    AcceptanceSpecId::new([1; 16]).expect("fixture id")
}
fn gate_id(value: u8) -> GateId {
    GateId::new([value; 16]).expect("fixture id")
}
